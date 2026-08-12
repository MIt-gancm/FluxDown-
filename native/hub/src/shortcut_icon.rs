//! Rewrites the `IconLocation` of Windows Explorer shortcuts (`.lnk`) that
//! target this app's executable, so desktop / Start Menu / taskbar-pinned
//! shortcuts follow the user's chosen app icon.
//!
//! # Why this exists
//!
//! `installer/windows/setup.iss`'s `[Icons]` section creates shortcuts with
//! `Filename: "{app}\flux_down.exe"` and no explicit `IconFilename` — Inno
//! Setup then defaults `IconLocation` to the target exe itself (icon index
//! 0), which is the icon compiled into the exe's PE resources
//! (`windows/runner/Runner.rc` → `app_icon.ico`). That reference is written
//! once at install time and never changes afterwards.
//!
//! `AppIconService` (`lib/src/services/app_icon_service.dart`) switches the
//! *running* window/taskbar-button/Alt-Tab/tray icon via `WM_SETICON`
//! (`window_manager.setIcon`), which only affects the current process's own
//! window handle — it cannot touch a `.lnk` file's static `IconLocation`,
//! nor the exe's own on-disk PE resources. Without this module, changing
//! the app icon in Settings never propagates to the desktop icon, the Start
//! Menu icon, or an icon the user has pinned to the taskbar.
//!
//! # Approach
//!
//! `windows-sys` ships no COM interface bindings (only raw functions and
//! constants), so `IShellLinkW`/`IPersistFile` are hand-rolled here as
//! `#[repr(C)]` vtables matching the stable, decades-old Win32 ABI
//! (`shobjidl_core.h` / `objidl.h`). For each shortcut found:
//! `CoCreateInstance(CLSID_ShellLink)` → `QueryInterface(IPersistFile)` →
//! `Load` the existing `.lnk` → verify its target resolves to this app's
//! exe (guards the taskbar pin folder, which holds pins for every app, not
//! just FluxDown) → `SetIconLocation` → `Save` → `SHChangeNotify` to make
//! Explorer refresh that item's icon immediately.
//!
//! Linux/macOS handle the equivalent problem differently (see
//! `AppIconService._applyIcon`): Linux overwrites the user's XDG icon-theme
//! override, macOS uses `NSWorkspace.setIcon` via the native
//! `com.fluxdown/window` channel. Neither needs a Rust-side component, so
//! this module is Windows-only.

#[cfg(target_os = "windows")]
mod inner {
    use crate::logger::log_info;
    use crate::signals::UpdateShortcutIcons;
    use rinf::DartSignal;
    use std::ffi::{OsStr, c_void};
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Path, PathBuf};
    use windows_sys::Win32::Storage::FileSystem::WIN32_FIND_DATAW;
    use windows_sys::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
    };
    use windows_sys::Win32::UI::Shell::{
        FOLDERID_Desktop, FOLDERID_Programs, FOLDERID_QuickLaunch, SHChangeNotify,
        SHGetKnownFolderPath, ShellLink as CLSID_SHELL_LINK,
    };
    use windows_sys::core::{GUID, HRESULT, PCWSTR, PWSTR};

    /// `{000214F9-0000-0000-C000-000000000046}` — `IID_IShellLinkW`. Not
    /// exported by `windows-sys` (no COM interface bindings ship in this
    /// crate); value from `shobjidl_core.h`, stable since Windows 95.
    const IID_ISHELLLINKW: GUID = GUID::from_u128(0x000214F9_0000_0000_C000_000000000046);
    /// `{0000010B-0000-0000-C000-000000000046}` — `IID_IPersistFile`, from
    /// `objidl.h`.
    const IID_IPERSISTFILE: GUID = GUID::from_u128(0x0000010B_0000_0000_C000_000000000046);

    /// `STGM_READWRITE`, from `objidl.h`'s `STGM` enum.
    const STGM_READWRITE: u32 = 0x0000_0002;
    /// Win32 `MAX_PATH`.
    const MAX_PATH: usize = 260;
    /// `COINIT_APARTMENTTHREADED`.
    const COINIT_APARTMENTTHREADED: u32 = 2;
    /// `SHCNE_UPDATEITEM` — a single shell item's attributes/icon changed.
    const SHCNE_UPDATEITEM: i32 = 0x0000_2000;
    /// `SHCNF_PATHW` — the notification's item is a null-terminated wide path.
    const SHCNF_PATHW: u32 = 0x0005;

    // ---- hand-rolled COM vtables ------------------------------------------------
    // windows-sys 0.59 only generates plain FFI (functions/constants/plain
    // structs), not COM interface bindings. Layout matches the standard,
    // unchanged-since-NT4 Win32 ABI: a COM object pointer is `*mut *const Vtbl`
    // (first field of the object is a pointer to its vtable).

    #[repr(C)]
    struct IUnknownVtbl {
        query_interface: unsafe extern "system" fn(
            this: *mut c_void,
            riid: *const GUID,
            ppv: *mut *mut c_void,
        ) -> HRESULT,
        add_ref: unsafe extern "system" fn(this: *mut c_void) -> u32,
        release: unsafe extern "system" fn(this: *mut c_void) -> u32,
    }

    #[repr(C)]
    struct IPersistVtbl {
        unknown: IUnknownVtbl,
        get_class_id: unsafe extern "system" fn(this: *mut c_void, class_id: *mut GUID) -> HRESULT,
    }

    #[repr(C)]
    struct IPersistFileVtbl {
        persist: IPersistVtbl,
        is_dirty: unsafe extern "system" fn(this: *mut c_void) -> HRESULT,
        load: unsafe extern "system" fn(this: *mut c_void, file_name: PCWSTR, mode: u32) -> HRESULT,
        save: unsafe extern "system" fn(
            this: *mut c_void,
            file_name: PCWSTR,
            remember: i32,
        ) -> HRESULT,
        save_completed: unsafe extern "system" fn(this: *mut c_void, file_name: PCWSTR) -> HRESULT,
        get_cur_file:
            unsafe extern "system" fn(this: *mut c_void, file_name: *mut PWSTR) -> HRESULT,
    }

    #[repr(C)]
    struct IShellLinkWVtbl {
        unknown: IUnknownVtbl,
        get_path: unsafe extern "system" fn(
            this: *mut c_void,
            file: PWSTR,
            cch_max_path: i32,
            find_data: *mut WIN32_FIND_DATAW,
            flags: u32,
        ) -> HRESULT,
        get_id_list:
            unsafe extern "system" fn(this: *mut c_void, ppidl: *mut *mut c_void) -> HRESULT,
        set_id_list: unsafe extern "system" fn(this: *mut c_void, pidl: *const c_void) -> HRESULT,
        get_description:
            unsafe extern "system" fn(this: *mut c_void, name: PWSTR, cch_max_name: i32) -> HRESULT,
        set_description: unsafe extern "system" fn(this: *mut c_void, name: PCWSTR) -> HRESULT,
        get_working_directory:
            unsafe extern "system" fn(this: *mut c_void, dir: PWSTR, cch_max_path: i32) -> HRESULT,
        set_working_directory: unsafe extern "system" fn(this: *mut c_void, dir: PCWSTR) -> HRESULT,
        get_arguments:
            unsafe extern "system" fn(this: *mut c_void, args: PWSTR, cch_max_path: i32) -> HRESULT,
        set_arguments: unsafe extern "system" fn(this: *mut c_void, args: PCWSTR) -> HRESULT,
        get_hotkey: unsafe extern "system" fn(this: *mut c_void, hotkey: *mut u16) -> HRESULT,
        set_hotkey: unsafe extern "system" fn(this: *mut c_void, hotkey: u16) -> HRESULT,
        get_show_cmd: unsafe extern "system" fn(this: *mut c_void, show_cmd: *mut i32) -> HRESULT,
        set_show_cmd: unsafe extern "system" fn(this: *mut c_void, show_cmd: i32) -> HRESULT,
        get_icon_location: unsafe extern "system" fn(
            this: *mut c_void,
            icon_path: PWSTR,
            cch_icon_path: i32,
            icon_index: *mut i32,
        ) -> HRESULT,
        set_icon_location: unsafe extern "system" fn(
            this: *mut c_void,
            icon_path: PCWSTR,
            icon_index: i32,
        ) -> HRESULT,
        set_relative_path: unsafe extern "system" fn(
            this: *mut c_void,
            path_rel: PCWSTR,
            reserved: u32,
        ) -> HRESULT,
        resolve:
            unsafe extern "system" fn(this: *mut c_void, hwnd: *mut c_void, flags: u32) -> HRESULT,
        set_path: unsafe extern "system" fn(this: *mut c_void, file: PCWSTR) -> HRESULT,
    }

    unsafe fn vtbl<T>(obj: *mut c_void) -> *const T {
        unsafe { *(obj as *mut *const T) }
    }

    unsafe fn unknown_query_interface(obj: *mut c_void, iid: &GUID) -> Option<*mut c_void> {
        unsafe {
            let v = vtbl::<IUnknownVtbl>(obj);
            let mut out: *mut c_void = std::ptr::null_mut();
            let hr = ((*v).query_interface)(obj, iid as *const GUID, &mut out as *mut *mut c_void);
            if hr >= 0 && !out.is_null() {
                Some(out)
            } else {
                None
            }
        }
    }

    unsafe fn unknown_release(obj: *mut c_void) {
        unsafe {
            let v = vtbl::<IUnknownVtbl>(obj);
            ((*v).release)(obj);
        }
    }

    unsafe fn shelllink_get_path(obj: *mut c_void, buf: &mut [u16]) -> HRESULT {
        unsafe {
            let v = vtbl::<IShellLinkWVtbl>(obj);
            let len = i32::try_from(buf.len()).unwrap_or(i32::MAX);
            ((*v).get_path)(obj, buf.as_mut_ptr(), len, std::ptr::null_mut(), 0)
        }
    }

    unsafe fn shelllink_set_icon_location(obj: *mut c_void, icon_path: PCWSTR) -> HRESULT {
        unsafe {
            let v = vtbl::<IShellLinkWVtbl>(obj);
            ((*v).set_icon_location)(obj, icon_path, 0)
        }
    }

    unsafe fn persistfile_load(obj: *mut c_void, path: PCWSTR, mode: u32) -> HRESULT {
        unsafe {
            let v = vtbl::<IPersistFileVtbl>(obj);
            ((*v).load)(obj, path, mode)
        }
    }

    unsafe fn persistfile_save(obj: *mut c_void, path: PCWSTR, remember: bool) -> HRESULT {
        unsafe {
            let v = vtbl::<IPersistFileVtbl>(obj);
            ((*v).save)(obj, path, i32::from(remember))
        }
    }

    // ---- helpers -----------------------------------------------------------------

    fn to_wide(s: &str) -> Vec<u16> {
        OsStr::new(s)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn wide_to_string(buf: &[u16]) -> String {
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
    }

    /// Canonical path of the running executable (resolves symlinks, strips
    /// the `\\?\` extended-length prefix for clean comparison).
    fn exe_path() -> Result<PathBuf, io::Error> {
        let path = std::env::current_exe()?;
        let canonical = std::fs::canonicalize(&path).unwrap_or(path);
        let s = canonical.to_string_lossy();
        Ok(PathBuf::from(s.strip_prefix(r"\\?\").unwrap_or(&s)))
    }

    /// Case-insensitive path comparison after best-effort canonicalization
    /// (resolves 8.3 short names / casing differences between the
    /// installer-written target and the live exe path).
    fn paths_match(a: &Path, b: &Path) -> bool {
        let ca = std::fs::canonicalize(a).unwrap_or_else(|_| a.to_path_buf());
        let cb = std::fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
        ca.to_string_lossy()
            .eq_ignore_ascii_case(&cb.to_string_lossy())
    }

    unsafe fn known_folder_dir(folder_id: &GUID) -> Option<PathBuf> {
        unsafe {
            let mut raw: PWSTR = std::ptr::null_mut();
            let hr = SHGetKnownFolderPath(
                folder_id as *const GUID,
                0,
                std::ptr::null_mut(),
                &mut raw as *mut PWSTR,
            );
            if hr < 0 || raw.is_null() {
                return None;
            }
            let mut len = 0usize;
            while *raw.add(len) != 0 {
                len += 1;
            }
            let path = PathBuf::from(String::from_utf16_lossy(std::slice::from_raw_parts(
                raw, len,
            )));
            CoTaskMemFree(raw as *const c_void);
            Some(path)
        }
    }

    unsafe fn notify_shell_item(path: &Path) {
        unsafe {
            let wide = to_wide(&path.to_string_lossy());
            SHChangeNotify(
                SHCNE_UPDATEITEM,
                SHCNF_PATHW,
                wide.as_ptr() as *const c_void,
                std::ptr::null(),
            );
        }
    }

    /// Rewrites one `.lnk`'s `IconLocation` and saves it, but only if its
    /// resolved target matches `exe`. Returns `Ok(true)` when updated,
    /// `Ok(false)` when skipped because the target didn't match (expected
    /// for unrelated pins in the shared taskbar folder).
    unsafe fn update_one(lnk_path: &Path, exe: &Path, icon_wide: &[u16]) -> Result<bool, String> {
        unsafe {
            let lnk_wide = to_wide(&lnk_path.to_string_lossy());

            let mut shell_link: *mut c_void = std::ptr::null_mut();
            let hr = CoCreateInstance(
                &CLSID_SHELL_LINK as *const GUID,
                std::ptr::null_mut(),
                CLSCTX_INPROC_SERVER,
                &IID_ISHELLLINKW as *const GUID,
                &mut shell_link as *mut *mut c_void,
            );
            if hr < 0 || shell_link.is_null() {
                return Err(format!("CoCreateInstance(CLSID_ShellLink) failed: {hr:#x}"));
            }

            let persist_file = match unknown_query_interface(shell_link, &IID_IPERSISTFILE) {
                Some(p) => p,
                None => {
                    unknown_release(shell_link);
                    return Err("QueryInterface(IID_IPersistFile) failed".to_string());
                }
            };

            let result = (|| -> Result<bool, String> {
                let hr = persistfile_load(persist_file, lnk_wide.as_ptr(), STGM_READWRITE);
                if hr < 0 {
                    return Err(format!("IPersistFile::Load failed: {hr:#x}"));
                }

                // Guard: only touch shortcuts that actually target this app's
                // exe. Matters for the taskbar's "User Pinned\TaskBar" folder,
                // which holds pins for every app, not just FluxDown.
                let mut path_buf = [0u16; MAX_PATH];
                if shelllink_get_path(shell_link, &mut path_buf) >= 0 {
                    let target = wide_to_string(&path_buf);
                    if !target.is_empty() && !paths_match(&PathBuf::from(&target), exe) {
                        return Ok(false);
                    }
                }

                let icon_ptr: PCWSTR = icon_wide.as_ptr();
                let hr = shelllink_set_icon_location(shell_link, icon_ptr);
                if hr < 0 {
                    return Err(format!("SetIconLocation failed: {hr:#x}"));
                }
                let hr = persistfile_save(persist_file, lnk_wide.as_ptr(), true);
                if hr < 0 {
                    return Err(format!("IPersistFile::Save failed: {hr:#x}"));
                }
                Ok(true)
            })();

            unknown_release(persist_file);
            unknown_release(shell_link);
            result
        }
    }

    /// Candidate `.lnk` paths: the fixed Desktop/Start Menu shortcuts this
    /// app's own installer creates, plus every `.lnk` in the taskbar's
    /// pinned-items folder (name not controlled by us — Windows names it
    /// after whatever the user pinned from, so all entries are scanned and
    /// filtered by target inside `update_one`).
    unsafe fn candidate_shortcuts() -> Vec<PathBuf> {
        unsafe {
            let mut targets = Vec::new();
            if let Some(desktop) = known_folder_dir(&FOLDERID_Desktop) {
                targets.push(desktop.join("FluxDown.lnk"));
            }
            if let Some(programs) = known_folder_dir(&FOLDERID_Programs) {
                targets.push(programs.join("FluxDown").join("FluxDown.lnk"));
            }
            if let Some(quick_launch) = known_folder_dir(&FOLDERID_QuickLaunch) {
                let taskbar_dir = quick_launch.join("User Pinned").join("TaskBar");
                if let Ok(entries) = std::fs::read_dir(&taskbar_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        let is_lnk = path
                            .extension()
                            .and_then(|e| e.to_str())
                            .map(|e| e.eq_ignore_ascii_case("lnk"))
                            .unwrap_or(false);
                        if is_lnk {
                            targets.push(path);
                        }
                    }
                }
            }
            targets.retain(|p| p.is_file());
            targets
        }
    }

    fn update_all(icon_path: &str) {
        let exe = match exe_path() {
            Ok(p) => p,
            Err(e) => {
                log_info!("[shortcut_icon] cannot resolve exe path: {e}");
                return;
            }
        };

        // SAFETY: `candidate_shortcuts`/`update_one`/`notify_shell_item` only
        // call well-formed, correctly-typed Win32/COM APIs with valid
        // pointers built just above; COM is initialized on this thread for
        // the duration of the block and torn down before returning.
        unsafe {
            let targets = candidate_shortcuts();
            if targets.is_empty() {
                return;
            }

            let icon_wide = to_wide(icon_path);
            let hr = CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED);
            if hr < 0 {
                log_info!("[shortcut_icon] CoInitializeEx failed: {hr:#x}");
                return;
            }

            for lnk in &targets {
                match update_one(lnk, &exe, &icon_wide) {
                    Ok(true) => {
                        notify_shell_item(lnk);
                        log_info!("[shortcut_icon] updated {}", lnk.display());
                    }
                    Ok(false) => {}
                    Err(e) => {
                        log_info!("[shortcut_icon] failed to update {}: {e}", lnk.display());
                    }
                }
            }

            CoUninitialize();
        }
    }

    /// Listens for `UpdateShortcutIcons` signals from Dart (sent by
    /// `AppIconService._applyIcon`/`useDefault` alongside the runtime
    /// `WM_SETICON` call) and rewrites matching shortcuts on a blocking
    /// worker thread — COM calls and file IO must not run on the
    /// single-threaded async runtime (see `lib.rs`'s runtime-constraint
    /// note).
    pub async fn listen() {
        let recv = UpdateShortcutIcons::get_dart_signal_receiver();
        while let Some(pack) = recv.recv().await {
            let icon_path = pack.message.icon_path;
            let _ = tokio::task::spawn_blocking(move || update_all(&icon_path)).await;
        }
    }
}

#[cfg(not(target_os = "windows"))]
mod inner {
    /// No-op — Linux/macOS handle their shortcut-icon equivalents outside
    /// Rust (see module docs above).
    pub async fn listen() {}
}

pub use inner::listen;
