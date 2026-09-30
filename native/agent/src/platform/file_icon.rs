//! 系统文件管理器为文件显示的图标（`agent.platform.fileIcon`）：Windows 资源管理器 / macOS Finder /
//! Linux GTK 图标主题，输出 `size`×`size` 的 PNG。
//!
//! # 为什么在 agent
//!
//! 界面进程（`crates/*`）恒为零 `unsafe`，而提取系统图标只有平台 FFI（Win32 Shell / GDI、
//! AppKit）可用：agent 的 `platform/*` 是已批准的 FFI 站点，界面经 JSON-RPC 取图标。整个功能
//! 挂在 `desktop` feature 下；headless 构建与其他平台返回 [`PlatformError::Unsupported`]。
//!
//! # 按文件 / 按扩展名
//!
//! 图标由扩展名关联决定，调用方按扩展名缓存即可，此时目标文件不必存在，系统只看虚拟文件名
//! `file.<ext>`（空扩展名为 `file`）。只有 `fluxdown_protocol::FILE_ICON_PER_FILE_EXTENSIONS`
//! （`exe` / `ico`，图标内嵌在文件自身里）才可能按文件取：`path` 必须是存在的绝对路径文件，且
//! 自身扩展名（小写）与 `extension` 一致，否则一律退回按扩展名。`.lnk` / `.url` 刻意排除：它们的
//! 图标路径可以指向远程共享，按文件解析会让系统向外发起认证（NTLM 泄露）。Linux 一律按扩展名。
//!
//! # 线程模型
//!
//! - Windows：Shell 图标 API 要求 STA COM 线程。专用常驻工作线程启动一次、线程内
//!   `CoInitializeEx(STA)` 一次，请求经 mpsc 串行处理，调用方阻塞等回复。
//! - macOS：同样用专用工作线程串行执行，每个请求包一层 `autoreleasepool`，不占用 tokio 的
//!   阻塞线程池，也避免并发的 AppKit 绘制。
//! - Linux：GTK 只能在主线程（tao 事件循环所在线程，拥有默认 `glib::MainContext`）调用；请求经
//!   `MainContext::invoke` 投递到主线程执行，调用方限时等待回复。
//!
//! # 系统图像列表为何常驻不 Release
//!
//! Windows 的 `SHGetImageList` 返回进程级单例系统图像列表（IImageList），按档位各取一次后常驻
//! （`LazyLock`）。windows-sys 不导出 `IUnknown` vtable，无法安全地 `Release`；系统也保证该列表
//! 在进程生命周期内有效，所以不释放，只泄漏一个引用计数。

use fluxdown_protocol::PlatformFileIconParams;

use super::PlatformError;

#[cfg(all(
    feature = "desktop",
    any(windows, target_os = "macos", target_os = "linux")
))]
use request::IconRequest;

/// 返回 `size`×`size`（`size` 限制在 16..=256）的 PNG 字节。同步阻塞，RPC 侧需放入
/// `spawn_blocking`。规则见模块文档。
///
/// # Errors
///
/// 当前构建 / 平台不支持时返回 [`PlatformError::Unsupported`]；系统取不到图标或编码失败时
/// 返回 [`PlatformError::Failed`]。
#[cfg(all(
    feature = "desktop",
    any(windows, target_os = "macos", target_os = "linux")
))]
pub fn file_icon_png(params: &PlatformFileIconParams) -> Result<Vec<u8>, PlatformError> {
    let request = IconRequest::new(params);
    platform_render(request).inspect_err(|error| {
        tracing::debug!(
            extension = %params.extension,
            size = params.size,
            %error,
            "file icon extraction failed"
        );
    })
}

/// 返回 `size`×`size`（`size` 限制在 16..=256）的 PNG 字节。同步阻塞，RPC 侧需放入
/// `spawn_blocking`。规则见模块文档。
///
/// # Errors
///
/// 当前构建 / 平台不支持时返回 [`PlatformError::Unsupported`]；系统取不到图标或编码失败时
/// 返回 [`PlatformError::Failed`]。
#[cfg(not(all(
    feature = "desktop",
    any(windows, target_os = "macos", target_os = "linux")
)))]
pub fn file_icon_png(_params: &PlatformFileIconParams) -> Result<Vec<u8>, PlatformError> {
    Err(PlatformError::Unsupported(
        "file icons require the desktop build on Windows, macOS or Linux",
    ))
}

#[cfg(all(feature = "desktop", windows))]
use windows::render as platform_render;

#[cfg(all(feature = "desktop", target_os = "macos"))]
use macos::render as platform_render;

#[cfg(all(feature = "desktop", target_os = "linux"))]
use linux::render as platform_render;

/// 请求归一化：与平台无关，任何构建都编译（headless 构建里其字段无人读取）。
#[cfg_attr(
    not(all(
        feature = "desktop",
        any(windows, target_os = "macos", target_os = "linux")
    )),
    allow(dead_code)
)]
mod request {
    use std::path::{Path, PathBuf};

    use fluxdown_protocol::{FILE_ICON_PER_FILE_EXTENSIONS, PlatformFileIconParams};

    const MIN_SIZE: u32 = 16;
    const MAX_SIZE: u32 = 256;

    /// 归一化后的取图标请求。
    pub(super) struct IconRequest {
        /// 小写、无前导点；可空（无扩展名的普通文件）。
        pub(super) extension: String,
        /// 仅 `exe` / `ico` 且文件存在时的按文件取图标路径；`None` = 按扩展名。
        /// Linux 一律按扩展名，不读取该字段。
        #[cfg_attr(target_os = "linux", allow(dead_code))]
        pub(super) file: Option<PathBuf>,
        /// 目标边长（物理像素），已限制在 16..=256。
        pub(super) size: u32,
    }

    impl IconRequest {
        pub(super) fn new(params: &PlatformFileIconParams) -> Self {
            let extension = normalize_extension(&params.extension);
            let file = per_file_path(&extension, params.path.as_deref());
            Self {
                extension,
                file,
                size: params.size.clamp(MIN_SIZE, MAX_SIZE),
            }
        }

        /// 按扩展名取图标时交给系统的虚拟文件名：`file.<ext>`，无扩展名为 `file`。
        #[cfg(any(windows, target_os = "linux"))]
        pub(super) fn virtual_name(&self) -> String {
            if self.extension.is_empty() {
                "file".to_owned()
            } else {
                format!("file.{}", self.extension)
            }
        }
    }

    /// 小写并去掉前导点。含路径分隔符、驱动器冒号、通配符或控制字符的输入不是扩展名，按
    /// 无扩展名处理：虚拟文件名只会是 `file.<ext>`，不会被构造成指向别处的路径。
    fn normalize_extension(raw: &str) -> String {
        let extension = raw.trim().trim_start_matches('.').to_lowercase();
        let invalid = extension.chars().any(|character| {
            character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
        });
        if invalid { String::new() } else { extension }
    }

    /// 契约：扩展名属于 [`FILE_ICON_PER_FILE_EXTENSIONS`]、`path` 是存在的绝对路径文件、且其
    /// 自身扩展名（小写）与 `extension` 一致时才按文件取图标。
    fn per_file_path(extension: &str, path: Option<&str>) -> Option<PathBuf> {
        if !FILE_ICON_PER_FILE_EXTENSIONS.contains(&extension) {
            return None;
        }
        let path = Path::new(path?);
        let own_extension = path.extension()?.to_string_lossy().to_lowercase();
        (path.is_absolute() && own_extension == extension && path.is_file())
            .then(|| path.to_path_buf())
    }

    #[allow(clippy::unwrap_used, clippy::expect_used)]
    #[cfg(test)]
    mod tests {
        use super::*;

        fn params(extension: &str, path: Option<&str>, size: u32) -> PlatformFileIconParams {
            PlatformFileIconParams {
                extension: extension.to_owned(),
                path: path.map(str::to_owned),
                size,
            }
        }

        /// 落盘的临时文件，测试结束删除。
        struct TempFile(PathBuf);

        impl TempFile {
            fn new(name: &str) -> Self {
                let dir =
                    std::env::temp_dir().join(format!("fluxdown-icon-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&dir).expect("create temp dir");
                let path = dir.join(name);
                std::fs::write(&path, b"x").expect("write temp file");
                Self(path)
            }

            fn path(&self) -> &str {
                self.0.to_str().expect("utf-8 temp path")
            }
        }

        impl Drop for TempFile {
            fn drop(&mut self) {
                if let Some(dir) = self.0.parent() {
                    let _ = std::fs::remove_dir_all(dir);
                }
            }
        }

        #[test]
        fn size_is_clamped_to_supported_range() {
            assert_eq!(IconRequest::new(&params("zip", None, 0)).size, 16);
            assert_eq!(IconRequest::new(&params("zip", None, 48)).size, 48);
            assert_eq!(IconRequest::new(&params("zip", None, 4096)).size, 256);
        }

        #[test]
        fn extension_is_lowercased_and_dot_stripped() {
            assert_eq!(IconRequest::new(&params(".ZIP", None, 32)).extension, "zip");
            assert_eq!(IconRequest::new(&params("", None, 32)).extension, "");
        }

        #[test]
        fn path_like_extension_is_treated_as_no_extension() {
            for hostile in ["a/b", "a\\b", "c:", "\\\\host\\share\\x", "x\0y", "a*"] {
                assert_eq!(IconRequest::new(&params(hostile, None, 32)).extension, "");
            }
        }

        #[test]
        fn executable_resolves_per_file_when_extension_matches() {
            let exe = TempFile::new("Setup.EXE");
            let request = IconRequest::new(&params("exe", Some(exe.path()), 32));
            assert_eq!(request.file.as_deref(), Some(exe.0.as_path()));
            let request = IconRequest::new(&params("EXE", Some(exe.path()), 32));
            assert_eq!(request.file.as_deref(), Some(exe.0.as_path()));
        }

        #[test]
        fn per_file_requires_existing_absolute_file_with_matching_extension() {
            let exe = TempFile::new("tool.exe");
            let dir = exe
                .0
                .parent()
                .expect("parent")
                .to_str()
                .expect("utf-8")
                .to_owned();
            // 路径自身扩展名与请求不一致：不按该文件取图标。
            assert!(
                IconRequest::new(&params("ico", Some(exe.path()), 32))
                    .file
                    .is_none()
            );
            // 文件不存在、是目录、是相对路径：一律按扩展名。
            let missing = format!("{dir}/missing.exe");
            assert!(
                IconRequest::new(&params("exe", Some(&missing), 32))
                    .file
                    .is_none()
            );
            assert!(
                IconRequest::new(&params("exe", Some(&dir), 32))
                    .file
                    .is_none()
            );
            assert!(
                IconRequest::new(&params("exe", Some("tool.exe"), 32))
                    .file
                    .is_none()
            );
            assert!(IconRequest::new(&params("exe", None, 32)).file.is_none());
        }

        #[test]
        fn shortcut_and_other_extensions_never_resolve_per_file() {
            for name in ["link.lnk", "site.url", "archive.zip"] {
                let file = TempFile::new(name);
                let extension = name.rsplit('.').next().expect("extension");
                assert!(
                    IconRequest::new(&params(extension, Some(file.path()), 32))
                        .file
                        .is_none(),
                    "{name}"
                );
            }
        }
    }
}

/// Windows / macOS 的专用常驻工作线程：请求串行执行，调用方阻塞等回复。
#[cfg(all(feature = "desktop", any(windows, target_os = "macos")))]
mod worker {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::mpsc;
    use std::thread;

    use super::{IconRequest, PlatformError};

    /// 在工作线程上把请求渲染成 PNG 字节。
    pub(super) type RenderFn = fn(&IconRequest) -> Result<Vec<u8>, PlatformError>;

    struct Job {
        request: IconRequest,
        reply: mpsc::Sender<Result<Vec<u8>, PlatformError>>,
    }

    pub(super) struct IconWorker {
        jobs: mpsc::Sender<Job>,
    }

    impl IconWorker {
        /// 启动工作线程。`setup` 在线程内先执行一次，其返回值（如 COM 初始化守卫）在线程存活
        /// 期间一直持有；线程起不来时返回错误文案。
        pub(super) fn spawn<G>(
            setup: impl FnOnce() -> G + Send + 'static,
            render: RenderFn,
        ) -> Result<Self, String> {
            let (jobs, inbox) = mpsc::channel::<Job>();
            thread::Builder::new()
                .name("fluxdown-file-icon".to_owned())
                .spawn(move || {
                    let _guard = setup();
                    for job in inbox {
                        // 渲染 panic 只让这一个请求失败，常驻线程继续服务后续请求。
                        let result = catch_unwind(AssertUnwindSafe(|| render(&job.request)))
                            .unwrap_or_else(|_| {
                                Err(PlatformError::Failed(
                                    "file icon renderer panicked".to_owned(),
                                ))
                            });
                        // 调用方已放弃等待（接收端被丢弃）时忽略发送失败。
                        let _ = job.reply.send(result);
                    }
                })
                .map(|_| Self { jobs })
                .map_err(|error| error.to_string())
        }
    }

    /// 把请求交给工作线程并阻塞等待回复；线程没起来或已退出时返回 `Failed`。
    pub(super) fn dispatch(
        worker: &Result<IconWorker, String>,
        request: IconRequest,
    ) -> Result<Vec<u8>, PlatformError> {
        let worker = worker.as_ref().map_err(|error| {
            PlatformError::Failed(format!("file icon worker failed to start: {error}"))
        })?;
        let stopped = || PlatformError::Failed("file icon worker stopped".to_owned());
        let (reply, answer) = mpsc::channel();
        worker
            .jobs
            .send(Job { request, reply })
            .map_err(|_| stopped())?;
        answer.recv().map_err(|_| stopped())?
    }
}

#[cfg(all(feature = "desktop", windows))]
mod windows {
    use std::ffi::OsStr;
    use std::io::Cursor;
    use std::os::windows::ffi::OsStrExt;
    use std::ptr;
    use std::sync::LazyLock;

    use image::imageops::{self, FilterType};
    use image::{ImageFormat, RgbaImage};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC,
        GetDIBits, GetObjectW, HBITMAP, HDC, RGBQUAD, ReleaseDC,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows_sys::Win32::System::Com::{
        COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
    };
    use windows_sys::Win32::UI::Controls::{HIMAGELIST, ILD_TRANSPARENT, ImageList_GetIcon};
    use windows_sys::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_SYSICONINDEX, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW, SHGetImageList,
        SHIL_EXTRALARGE, SHIL_LARGE, SHIL_SMALL,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    use windows_sys::core::GUID;

    use super::worker::{IconWorker, dispatch};
    use super::{IconRequest, PlatformError};

    /// `IID_IImageList`：windows-sys 0.59 不导出。
    const IID_IMAGE_LIST: GUID = GUID::from_u128(0x46eb5926_582e_4017_9fdf_e8998daa0950);

    /// 单张图标位图的合理边长上限（系统图像列表最大档 256px），防御异常返回值。
    const MAX_BITMAP_SIDE: i32 = 1024;

    static WORKER: LazyLock<Result<IconWorker, String>> =
        LazyLock::new(|| IconWorker::spawn(Com::init, render_on_worker));

    // 系统图像列表按档位各取一次，常驻不 Release（见模块文档）。取用只发生在已初始化 COM 的
    // 工作线程上。
    static SMALL_LIST: LazyLock<Option<HIMAGELIST>> =
        LazyLock::new(|| system_image_list(SHIL_SMALL as i32));
    static LARGE_LIST: LazyLock<Option<HIMAGELIST>> =
        LazyLock::new(|| system_image_list(SHIL_LARGE as i32));
    static EXTRA_LARGE_LIST: LazyLock<Option<HIMAGELIST>> =
        LazyLock::new(|| system_image_list(SHIL_EXTRALARGE as i32));

    pub(super) fn render(request: IconRequest) -> Result<Vec<u8>, PlatformError> {
        dispatch(&WORKER, request)
    }

    fn failed(what: &str) -> PlatformError {
        PlatformError::Failed(format!("{what} failed"))
    }

    /// 线程级 COM（STA）初始化守卫：文档要求调用 `SHGetFileInfo` 前先初始化。
    struct Com(bool);

    impl Com {
        fn init() -> Self {
            // SAFETY: reserved 参数必须为 null；成功（S_OK / S_FALSE）由 Drop 配对 CoUninitialize。
            let hr = unsafe { CoInitializeEx(ptr::null(), COINIT_APARTMENTTHREADED as _) };
            if hr < 0 {
                tracing::warn!(
                    hr = format!("{hr:#x}"),
                    "CoInitializeEx failed for file icons"
                );
            }
            Self(hr >= 0)
        }
    }

    impl Drop for Com {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: 与 init 中成功的 CoInitializeEx 配对。
                unsafe { CoUninitialize() };
            }
        }
    }

    /// `ImageList_GetIcon` 创建的 HICON，Drop 时销毁。
    struct Icon(HICON);

    impl Drop for Icon {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: HICON 由 ImageList_GetIcon 新建，仅此守卫拥有。
                unsafe { DestroyIcon(self.0) };
            }
        }
    }

    /// `GetIconInfo` 交出所有权的位图，Drop 时删除。
    struct Bitmap(HBITMAP);

    impl Drop for Bitmap {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: 位图由 GetIconInfo 交给调用方，仅此守卫拥有且未被选入任何 DC。
                unsafe { DeleteObject(self.0) };
            }
        }
    }

    /// 整个屏幕的 DC，Drop 时释放。
    struct ScreenDc(HDC);

    impl Drop for ScreenDc {
        fn drop(&mut self) {
            if !self.0.is_null() {
                // SAFETY: 由 GetDC(null) 取得，配对 ReleaseDC(null, dc)。
                unsafe { ReleaseDC(ptr::null_mut(), self.0) };
            }
        }
    }

    fn system_image_list(tier: i32) -> Option<HIMAGELIST> {
        let mut raw = ptr::null_mut();
        // SAFETY: riid 指向静态 GUID，raw 为有效输出槽。
        let hr = unsafe { SHGetImageList(tier, &IID_IMAGE_LIST, &mut raw) };
        if hr < 0 || raw.is_null() {
            tracing::debug!(hr = format!("{hr:#x}"), tier, "SHGetImageList failed");
            return None;
        }
        // IImageList 指针按文档可直接当 HIMAGELIST 使用。
        Some(raw as HIMAGELIST)
    }

    /// 按目标边长选系统图像列表档位：<=16 小图标，<=32 大图标，其余超大（48px）。不用 JUMBO：
    /// 缺 256px 图标时它返回左上角小图的 256 画布。
    fn image_list_for(size: u32) -> Option<HIMAGELIST> {
        match size {
            0..=16 => *SMALL_LIST,
            17..=32 => *LARGE_LIST,
            _ => *EXTRA_LARGE_LIST,
        }
    }

    fn render_on_worker(request: &IconRequest) -> Result<Vec<u8>, PlatformError> {
        let list = image_list_for(request.size).ok_or_else(|| failed("SHGetImageList"))?;
        // 按文件取失败（被占用 / 刚被删除 / 资源损坏）时退回按扩展名，不让这一行只剩类型图标。
        let index = match request.file.as_deref() {
            Some(path) => {
                system_icon_index(Some(path), request).or_else(|_| system_icon_index(None, request))
            }
            None => system_icon_index(None, request),
        }?;
        let image = icon_image(list, index)?;
        encode_png(image, request.size)
    }

    fn wide(text: &OsStr) -> Vec<u16> {
        text.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// 文件（`file`）或虚拟文件名（`None`，按扩展名）在系统图像列表中的图标索引。
    fn system_icon_index(
        file: Option<&std::path::Path>,
        request: &IconRequest,
    ) -> Result<i32, PlatformError> {
        let (name, attributes, flags) = match file {
            Some(path) => (wide(path.as_os_str()), 0, SHGFI_SYSICONINDEX),
            // 虚拟文件名 + USEFILEATTRIBUTES：文件不必存在，只按扩展名关联取图标。
            None => (
                wide(OsStr::new(&request.virtual_name())),
                FILE_ATTRIBUTE_NORMAL,
                SHGFI_SYSICONINDEX | SHGFI_USEFILEATTRIBUTES,
            ),
        };
        let mut info = SHFILEINFOW {
            hIcon: ptr::null_mut(),
            iIcon: 0,
            dwAttributes: 0,
            szDisplayName: [0; 260],
            szTypeName: [0; 80],
        };
        // SAFETY: name 以 NUL 结尾并在调用期间存活；info 为有效输出结构，cbFileInfo 与其大小一致。
        let found = unsafe {
            SHGetFileInfoW(
                name.as_ptr(),
                attributes,
                &mut info,
                size_of::<SHFILEINFOW>() as u32,
                flags,
            )
        };
        if found == 0 {
            return Err(failed("SHGetFileInfoW"));
        }
        Ok(info.iIcon)
    }

    /// 读取图标的 RGBA 位图（原生尺寸）。
    fn icon_image(list: HIMAGELIST, index: i32) -> Result<RgbaImage, PlatformError> {
        // SAFETY: list 为系统图像列表句柄，index 由 SHGetFileInfoW 返回；HICON 交给守卫销毁。
        let icon = Icon(unsafe { ImageList_GetIcon(list, index, ILD_TRANSPARENT) });
        if icon.0.is_null() {
            return Err(failed("ImageList_GetIcon"));
        }
        let mut icon_info = ICONINFO {
            fIcon: 0,
            xHotspot: 0,
            yHotspot: 0,
            hbmMask: ptr::null_mut(),
            hbmColor: ptr::null_mut(),
        };
        // SAFETY: icon 为有效 HICON，icon_info 为有效输出结构。
        let ok = unsafe { GetIconInfo(icon.0, &mut icon_info) };
        // 两张位图的所有权已交给调用方：先入守卫，任何返回路径都会删除。
        let color = Bitmap(icon_info.hbmColor);
        let mask = Bitmap(icon_info.hbmMask);
        if ok == 0 || color.0.is_null() {
            return Err(failed("GetIconInfo"));
        }
        let mut bitmap = BITMAP {
            bmType: 0,
            bmWidth: 0,
            bmHeight: 0,
            bmWidthBytes: 0,
            bmPlanes: 0,
            bmBitsPixel: 0,
            bmBits: ptr::null_mut(),
        };
        // SAFETY: 输出缓冲为 BITMAP 大小，color 为有效位图句柄。
        let got = unsafe {
            GetObjectW(
                color.0,
                size_of::<BITMAP>() as i32,
                (&raw mut bitmap).cast(),
            )
        };
        let (width, height) = (bitmap.bmWidth, bitmap.bmHeight);
        if got == 0
            || !(1..=MAX_BITMAP_SIDE).contains(&width)
            || !(1..=MAX_BITMAP_SIDE).contains(&height)
        {
            return Err(failed("GetObjectW"));
        }
        let (width, height) = (width.unsigned_abs(), height.unsigned_abs());
        // SAFETY: null = 整个屏幕 DC，交给守卫配对释放。
        let dc = ScreenDc(unsafe { GetDC(ptr::null_mut()) });
        if dc.0.is_null() {
            return Err(failed("GetDC"));
        }
        let mut image = read_dib(dc.0, color.0, width, height)?;
        // 老式无 alpha 图标：alpha 全 0，改由掩码（0 = 不透明）推出。
        if image.pixels().all(|pixel| pixel[3] == 0) {
            let mask_bits = read_dib(dc.0, mask.0, width, height)?;
            for (pixel, bit) in image.pixels_mut().zip(mask_bits.pixels()) {
                pixel[3] = if bit[0] == 0 { 255 } else { 0 };
            }
        }
        // GDI 给出 BGRA，转成 RGBA。
        for pixel in image.pixels_mut() {
            pixel.0.swap(0, 2);
        }
        Ok(image)
    }

    /// 以 32bpp 自上而下读出位图像素；返回缓冲里的通道顺序仍是 BGRA。
    fn read_dib(
        dc: HDC,
        bitmap: HBITMAP,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, PlatformError> {
        let signed_height = i32::try_from(height).map_err(|_| failed("bitmap height"))?;
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: i32::try_from(width).map_err(|_| failed("bitmap width"))?,
                biHeight: -signed_height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD {
                rgbBlue: 0,
                rgbGreen: 0,
                rgbRed: 0,
                rgbReserved: 0,
            }],
        };
        let mut image = RgbaImage::new(width, height);
        // SAFETY: 32bpp 自上而下的头与 width*height*4 字节的缓冲区大小一致；位图未选入任何 DC。
        let lines = unsafe {
            GetDIBits(
                dc,
                bitmap,
                0,
                height,
                image.as_mut_ptr().cast(),
                &mut info,
                DIB_RGB_COLORS,
            )
        };
        if lines == signed_height {
            Ok(image)
        } else {
            Err(failed("GetDIBits"))
        }
    }

    /// 缩放到 `size`×`size`（尺寸不符时）并编码为 PNG。
    fn encode_png(image: RgbaImage, size: u32) -> Result<Vec<u8>, PlatformError> {
        let image = if image.width() == size && image.height() == size {
            image
        } else {
            resize_straight_alpha(image, size)
        };
        let mut png = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
            .map_err(|error| PlatformError::Failed(format!("PNG encoding failed: {error}")))?;
        Ok(png)
    }

    /// `imageops::resize` 按预乘 alpha 采样；图标位图是直通 alpha，直接缩放会在半透明边缘
    /// 出现暗边，所以先预乘、缩放后再还原。
    fn resize_straight_alpha(mut image: RgbaImage, size: u32) -> RgbaImage {
        for pixel in image.pixels_mut() {
            let alpha = u32::from(pixel[3]);
            for channel in &mut pixel.0[..3] {
                *channel =
                    u8::try_from((u32::from(*channel) * alpha + 127) / 255).unwrap_or(u8::MAX);
            }
        }
        let mut resized = imageops::resize(&image, size, size, FilterType::Lanczos3);
        for pixel in resized.pixels_mut() {
            let alpha = u32::from(pixel[3]);
            if alpha == 0 || alpha == 255 {
                continue;
            }
            for channel in &mut pixel.0[..3] {
                *channel = u8::try_from((u32::from(*channel) * 255 + alpha / 2) / alpha)
                    .unwrap_or(u8::MAX);
            }
        }
        resized
    }
}

#[cfg(all(feature = "desktop", target_os = "macos"))]
mod macos {
    use std::ptr;
    use std::sync::LazyLock;

    use objc2::AnyThread as _;
    use objc2::rc::autoreleasepool;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSGraphicsContext, NSImageInterpolation,
        NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};

    use super::worker::{IconWorker, dispatch};
    use super::{IconRequest, PlatformError};

    /// AppKit 常量 `NSDeviceRGBColorSpace` 的取值。直接用同值 NSString 而不读取 extern static，
    /// 省掉一处 `unsafe`；AppKit 按 `isEqual:` 匹配，取值错了初始化会返回 nil 而不是静默出错。
    const DEVICE_RGB_COLOR_SPACE: &str = "NSDeviceRGBColorSpace";

    static WORKER: LazyLock<Result<IconWorker, String>> =
        LazyLock::new(|| IconWorker::spawn(|| (), render_on_worker));

    pub(super) fn render(request: IconRequest) -> Result<Vec<u8>, PlatformError> {
        dispatch(&WORKER, request)
    }

    fn failed(what: &str) -> PlatformError {
        PlatformError::Failed(format!("{what} failed"))
    }

    /// 工作线程不在主线程的自动释放池里：每个请求自带一个池，图标对象随之释放。
    fn render_on_worker(request: &IconRequest) -> Result<Vec<u8>, PlatformError> {
        autoreleasepool(|_| draw_png(request))
    }

    /// 取系统图标，绘制进精确 `size`×`size` 像素的 8-bit RGBA 位图并导出 PNG。
    ///
    /// 不走 `NSImage::TIFFRepresentation`（含 16–1024px 全部表示，单个图标数十 MB，App 图标
    /// 还是 `image` 解不了的 16-bit float TIFF）；`lockFocus` 得到的位图同样是 16-bit float
    /// 且尺寸随屏幕倍率。
    fn draw_png(request: &IconRequest) -> Result<Vec<u8>, PlatformError> {
        let workspace = NSWorkspace::sharedWorkspace();
        let icon = match &request.file {
            Some(path) => workspace.iconForFile(&NSString::from_str(&path.to_string_lossy())),
            None => {
                // `iconForContentType` 需要额外引入 objc2-uniform-type-identifiers，为一个
                // 仍可用的 API 不值得；deprecated 的 `iconForFileType` 直接接受扩展名。
                #[allow(deprecated)]
                workspace.iconForFileType(&NSString::from_str(&request.extension))
            }
        };

        let side = isize::try_from(request.size).map_err(|_| failed("icon size"))?;
        let color_space = NSString::from_str(DEVICE_RGB_COLOR_SPACE);
        // SAFETY: planes 传 null 由 AppKit 自行分配像素缓冲；其余参数按 8-bit RGBA 非平面格式取值，
        // bytesPerRow / bitsPerPixel 传 0 表示自动计算。
        let bitmap = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(),
                ptr::null_mut(),
                side,
                side,
                8,
                4,
                true,
                false,
                &color_space,
                0,
                0,
            )
        }
        .ok_or_else(|| failed("NSBitmapImageRep init"))?;
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)
            .ok_or_else(|| failed("NSGraphicsContext"))?;

        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        context.setImageInterpolation(NSImageInterpolation::High);
        let extent = NSSize::new(f64::from(request.size), f64::from(request.size));
        icon.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), extent));
        context.flushGraphics();
        NSGraphicsContext::restoreGraphicsState_class();

        // SAFETY: properties 字典的泛型即方法签名要求的类型，空字典满足。
        let data = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )
        }
        .ok_or_else(|| failed("PNG representation"))?;
        Ok(data.to_vec())
    }
}

#[cfg(all(feature = "desktop", target_os = "linux"))]
mod linux {
    use std::sync::mpsc;
    use std::time::Duration;

    use gtk::gdk_pixbuf::{InterpType, Pixbuf};
    use gtk::prelude::IconThemeExt;
    use gtk::{IconLookupFlags, IconTheme, gio, glib};

    use super::{IconRequest, PlatformError};

    /// 主线程响应上限：GTK 事件循环卡住（或已退出）时不无限阻塞调用方。
    const MAIN_THREAD_TIMEOUT: Duration = Duration::from_secs(2);

    /// 经默认 `MainContext` 把取图标投递到 GTK 主线程，限时等待回复。
    pub(super) fn render(request: IconRequest) -> Result<Vec<u8>, PlatformError> {
        if !gtk::is_initialized() {
            return Err(PlatformError::Unsupported("GTK is not initialized"));
        }
        let file_name = request.virtual_name();
        let size = request.size;
        let (reply, answer) = mpsc::channel();
        glib::MainContext::default().invoke(move || {
            // 调用方已超时放弃时接收端已丢弃，忽略发送失败。
            let _ = reply.send(render_on_main_thread(&file_name, size));
        });
        answer
            .recv_timeout(MAIN_THREAD_TIMEOUT)
            .map_err(|_| PlatformError::Failed("GTK main thread did not respond".to_owned()))?
    }

    fn failed(what: &str) -> PlatformError {
        PlatformError::Failed(format!("{what} failed"))
    }

    /// 虚拟文件名 → 内容类型 → 图标 → 当前 GTK 图标主题 → PNG。必须在 GTK 主线程调用。
    fn render_on_main_thread(file_name: &str, size: u32) -> Result<Vec<u8>, PlatformError> {
        // 默认 MainContext 没有被主线程持有时（事件循环没跑起来），`invoke` 会在调用方线程上
        // 直接执行闭包：此时拒绝，绝不在非主线程碰 GTK。
        if !gtk::is_initialized_main_thread() {
            return Err(PlatformError::Unsupported(
                "GTK main loop is not running on this thread",
            ));
        }
        let side = i32::try_from(size).map_err(|_| failed("icon size"))?;
        let (content_type, _uncertain) = gio::content_type_guess(Some(file_name), &[]);
        let icon = gio::content_type_get_icon(&content_type);
        let theme = IconTheme::default().ok_or_else(|| failed("default icon theme"))?;
        let info = theme
            .lookup_by_gicon(&icon, side, IconLookupFlags::FORCE_SIZE)
            .ok_or_else(|| failed("icon lookup"))?;
        let pixbuf = info
            .load_icon()
            .map_err(|error| PlatformError::Failed(format!("icon load failed: {error}")))?;
        // FORCE_SIZE 下非方形图标只保证不超过 size：统一缩放成 size×size。
        let pixbuf = if pixbuf.width() == side && pixbuf.height() == side {
            pixbuf
        } else {
            scale_square(&pixbuf, side)?
        };
        pixbuf
            .save_to_bufferv("png", &[])
            .map_err(|error| PlatformError::Failed(format!("PNG encoding failed: {error}")))
    }

    fn scale_square(pixbuf: &Pixbuf, side: i32) -> Result<Pixbuf, PlatformError> {
        pixbuf
            .scale_simple(side, side, InterpType::Bilinear)
            .ok_or_else(|| failed("icon scaling"))
    }
}
