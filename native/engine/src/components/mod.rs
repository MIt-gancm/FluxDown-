//! 可选外部组件管理（v1：ffmpeg + yt-dlp）。
//!
//! 组件是宿主侧受控的外部可执行程序，**不随安装包分发**，运行时按需由用户在
//! 设置「组件」页主动触发下载（合规边界）。它们与插件沙箱（QuickJS）是两套
//! 正交的信任模型，本模块不与插件系统的脚本执行交互——但已安装的组件会被
//! 授权插件经 `flux.ffmpeg` / `flux.ytdlp` 门面调用（见 [`crate::plugin`]）。
//!
//! - [`ffmpeg`]：音视频处理，服务 DASH/轨对任务的 mux（BtbN 归档，解压取单文件）。
//! - [`ytdlp`]：站点媒体提取器，服务插件 resolve 平面的直链提取（单文件二进制）。
//!
//! 两者共用同一套路径来源模型（[`ComponentSource`]，manual→managed→system）、
//! 错误类型（[`ComponentError`]）与安装底座（[`download_to_file`]/[`fetch_github_json`]）。

mod ffmpeg;
mod ytdlp;

pub use ffmpeg::*;
pub use ytdlp::*;

use std::path::PathBuf;

/// 组件下载镜像基址的 config 键（675#1）：GitHub Release API 未认证匿名限流
/// 60 请求/小时，公司网络/部分地区直连 `github.com`/`api.github.com` 也可能
/// 被拦截（403/超时）。用户可在设置里填一个 GitHub 反代地址（如自建
/// `ghproxy`/`gh-proxy` 类反代的基址），非空时替换掉请求 URL 里的
/// `https://api.github.com` 或 `https://github.com` 前缀（含 Release JSON
/// 查询与资产直链下载两条路径）。空值（默认）= 不改写，直连 GitHub。
pub const CONFIG_COMPONENT_MIRROR_BASE: &str = "component_mirror_base";

/// 把 `url` 的 `https://api.github.com` / `https://github.com` 前缀替换为
/// `mirror_base`（trim 首尾空白与末尾 `/`）。`mirror_base` 为空则原样返回。
/// 纯函数，不做网络/IO，便于测试。
#[cfg(any(feature = "components", test))]
pub(crate) fn apply_component_mirror(url: &str, mirror_base: &str) -> String {
    let mirror_base = mirror_base.trim().trim_end_matches('/');
    if mirror_base.is_empty() {
        return url.to_string();
    }
    for prefix in ["https://api.github.com", "https://github.com"] {
        if let Some(rest) = url.strip_prefix(prefix) {
            return format!("{mirror_base}{rest}");
        }
    }
    url.to_string()
}

/// 读取用户配置的组件镜像基址（[`CONFIG_COMPONENT_MIRROR_BASE`]）；未设置
/// 或读取失败均返回空串（= 不改写，直连 GitHub），不让配置读取失败阻断
/// 安装流程。
#[cfg(feature = "components")]
pub(crate) async fn component_mirror_base(db: &crate::db::Db) -> String {
    db.get_config(CONFIG_COMPONENT_MIRROR_BASE)
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
}

/// 组件生效路径的来源。ffmpeg / yt-dlp 共用；`as_str` 为稳定 wire 字符串
/// （跨 hub 信号 / server JSON / Dart 徽章共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentSource {
    /// 用户手动指定路径（config）。
    Manual,
    /// 数据目录 `bin/` 下的托管安装。
    Managed,
    /// 系统 PATH 中找到。
    System,
    /// 未找到任何可用二进制。
    None,
}

impl ComponentSource {
    /// 稳定的 wire 字符串（跨 hub 信号 / server JSON 共用）。
    pub fn as_str(self) -> &'static str {
        match self {
            ComponentSource::Manual => "manual",
            ComponentSource::Managed => "managed",
            ComponentSource::System => "system",
            ComponentSource::None => "none",
        }
    }
}

/// 组件操作错误（ffmpeg / yt-dlp 共用）。
#[derive(thiserror::Error, Debug)]
pub enum ComponentError {
    /// 当前平台无托管构建——请用系统安装或手动指定路径。
    #[error("managed install not supported on this platform")]
    Unsupported,
    #[error("http error: {0}")]
    Http(String),
    #[error("asset not found: {0}")]
    NotFound(String),
    #[error("io error: {0}")]
    Io(String),
    #[error("db error: {0}")]
    Db(String),
    #[error("archive error: {0}")]
    Archive(String),
    /// 安装后可执行探测失败（下载损坏/架构不符）。
    #[error("installed binary failed verification: {0}")]
    Verify(String),
}

/// macOS/Linux 上 Homebrew（及部分发行版自装软件）常见的固定安装目录。
/// GUI 应用进程继承的 PATH 往往不含这些目录（Finder/启动台启动时 shell
/// 登录脚本未执行），仅在 PATH 扫描失败后作为兜底，不改变 PATH 内的优先级。
#[cfg(target_os = "macos")]
const FALLBACK_DIRS: &[&str] = &["/opt/homebrew/bin", "/usr/local/bin"];
#[cfg(all(unix, not(target_os = "macos")))]
const FALLBACK_DIRS: &[&str] = &["/usr/local/bin"];
#[cfg(not(unix))]
const FALLBACK_DIRS: &[&str] = &[];

/// 扫描系统 PATH 寻找指定可执行文件（含 Windows 的 `.exe` 后缀由调用方带入）。
///
/// PATH 扫描失败时，在类 Unix 平台回退探测 [`FALLBACK_DIRS`]（Homebrew 等
/// 固定安装目录），不改变 PATH 内目录的优先级。
pub fn find_in_path(binary_name: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH");
    let path_dirs = path_var
        .as_deref()
        .map(std::env::split_paths)
        .into_iter()
        .flatten();
    let fallback = FALLBACK_DIRS.iter().map(PathBuf::from);
    find_in_dirs(path_dirs.chain(fallback), binary_name)
}

/// 按给定顺序在目录列表中查找第一个存在的 `dir/binary_name` 常规文件；
/// 空目录项跳过。纯函数，不读环境变量，便于测试。
fn find_in_dirs(dirs: impl IntoIterator<Item = PathBuf>, binary_name: &str) -> Option<PathBuf> {
    dirs.into_iter()
        .filter(|dir| !dir.as_os_str().is_empty())
        .map(|dir| dir.join(binary_name))
        .find(|candidate| candidate.is_file())
}

/// GitHub Release API JSON 拉取（带 `User-Agent`/`Accept` 头）。ffmpeg / yt-dlp
/// 安装流程共用。
#[cfg(feature = "components")]
pub(crate) async fn fetch_github_json(
    client: &reqwest::Client,
    url: &str,
) -> Result<serde_json::Value, ComponentError> {
    let resp = client
        .get(url)
        .header("User-Agent", "FluxDown")
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| ComponentError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ComponentError::Http(format!(
            "GitHub API returned {}",
            resp.status()
        )));
    }
    resp.json::<serde_json::Value>()
        .await
        .map_err(|e| ComponentError::Http(e.to_string()))
}

/// [`fetch_github_json`]，但先按用户配置的 [`CONFIG_COMPONENT_MIRROR_BASE`]
/// 改写 `url`（675#1）：改写后请求失败（或未配置，改写后与原 URL 相同）
/// 都回退直连原始 `url`，不让一次镜像故障拖垮整个安装流程。
#[cfg(feature = "components")]
pub(crate) async fn fetch_github_json_with_mirror(
    client: &reqwest::Client,
    url: &str,
    mirror_base: &str,
) -> Result<serde_json::Value, ComponentError> {
    let mirrored = apply_component_mirror(url, mirror_base);
    if mirrored != url
        && let Ok(v) = fetch_github_json(client, &mirrored).await
    {
        return Ok(v);
    }
    fetch_github_json(client, url).await
}

/// 官网组件版本镜像的基地址。版本列表拉取优先经此转发（服务端持 token +
/// 24h 缓存，规避 GitHub 匿名 API 每 IP 60/h 限流与直连 api.github.com 的
/// 网络问题），失败依次回退用户配置的镜像（若有）与直连 GitHub。
#[cfg(feature = "components")]
const MIRROR_BASE: &str = "https://fluxdown.zerx.dev/api/components";

/// 版本列表专用：优先经官网镜像 `MIRROR_BASE/<component>` 拉取（返回原样
/// GitHub JSON）；失败则经 [`fetch_github_json_with_mirror`] 依次尝试用户
/// 配置的镜像与直连 `github_url`。二进制下载不走此路径。
#[cfg(feature = "components")]
pub(crate) async fn fetch_versions_json(
    client: &reqwest::Client,
    component: &str,
    github_url: &str,
    mirror_base: &str,
) -> Result<serde_json::Value, ComponentError> {
    let official_mirror = format!("{MIRROR_BASE}/{component}");
    if let Ok(v) = fetch_github_json(client, &official_mirror).await {
        return Ok(v);
    }
    fetch_github_json_with_mirror(client, github_url, mirror_base).await
}

/// 流式下载 `url` 到 `dest`，`progress(downloaded, total)` 上报进度（total=0 未知）。
/// ffmpeg 归档 / yt-dlp 单二进制安装共用；每 256KB 上报一次避免信号风暴。
/// 先按用户配置的 [`CONFIG_COMPONENT_MIRROR_BASE`] 改写 `url`（675#1）：改写后
/// 下载失败（或未配置）都回退直连原始 `url`。
#[cfg(feature = "components")]
pub(crate) async fn download_to_file(
    client: &reqwest::Client,
    url: &str,
    mirror_base: &str,
    dest: &std::path::Path,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(), ComponentError> {
    let mirrored = apply_component_mirror(url, mirror_base);
    if mirrored != url
        && download_to_file_from(client, &mirrored, dest, progress)
            .await
            .is_ok()
    {
        return Ok(());
    }
    download_to_file_from(client, url, dest, progress).await
}

#[cfg(feature = "components")]
async fn download_to_file_from(
    client: &reqwest::Client,
    url: &str,
    dest: &std::path::Path,
    progress: &(dyn Fn(u64, u64) + Send + Sync),
) -> Result<(), ComponentError> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let resp = client
        .get(url)
        .header("User-Agent", "FluxDown")
        .send()
        .await
        .map_err(|e| ComponentError::Http(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(ComponentError::Http(format!(
            "download returned {}",
            resp.status()
        )));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(dest)
        .await
        .map_err(|e| ComponentError::Io(e.to_string()))?;
    let mut stream = resp.bytes_stream();
    let mut downloaded: u64 = 0;
    let mut last_report: u64 = 0;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| ComponentError::Http(e.to_string()))?;
        file.write_all(&chunk)
            .await
            .map_err(|e| ComponentError::Io(e.to_string()))?;
        downloaded += chunk.len() as u64;
        if downloaded - last_report >= 256 * 1024 || downloaded == total {
            progress(downloaded, total);
            last_report = downloaded;
        }
    }
    file.flush()
        .await
        .map_err(|e| ComponentError::Io(e.to_string()))?;
    progress(downloaded, total);
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use std::path::PathBuf;

    use super::{ComponentSource, apply_component_mirror};

    #[test]
    fn source_wire_strings() {
        assert_eq!(ComponentSource::Manual.as_str(), "manual");
        assert_eq!(ComponentSource::Managed.as_str(), "managed");
        assert_eq!(ComponentSource::System.as_str(), "system");
        assert_eq!(ComponentSource::None.as_str(), "none");
    }

    /// 目录顺序即优先级：前面的目录命中即返回，空目录项被跳过，缺失目录
    /// 不影响后续（模拟 PATH 未命中后回退到 Homebrew 目录）。
    #[test]
    fn find_in_dirs_respects_order_and_skips_empty_or_missing() {
        let root = std::env::temp_dir().join(format!(
            "fluxdown-find-in-dirs-{}-{}",
            std::process::id(),
            line!()
        ));
        let first = root.join("first");
        let second = root.join("second");
        std::fs::create_dir_all(&first).unwrap();
        std::fs::create_dir_all(&second).unwrap();
        let name = "fluxdown-test-binary";
        std::fs::write(second.join(name), b"").unwrap();
        // `first` 里只有同名目录，不是文件，不应命中。
        std::fs::create_dir_all(first.join(name)).unwrap();

        let dirs = vec![
            PathBuf::new(),
            root.join("missing"),
            first.clone(),
            second.clone(),
        ];
        let found = super::find_in_dirs(dirs, name);
        let none = super::find_in_dirs([first, second], "fluxdown-definitely-not-installed");
        let _ = std::fs::remove_dir_all(&root);

        assert_eq!(found, Some(root.join("second").join(name)));
        assert_eq!(none, None);
    }

    // 675#1：未配置镜像时原样返回。
    #[test]
    fn apply_component_mirror_noop_when_unset() {
        let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
        assert_eq!(apply_component_mirror(url, ""), url);
        assert_eq!(apply_component_mirror(url, "   "), url);
    }

    // 配置后改写 api.github.com 前缀（Release JSON 端点）。
    #[test]
    fn apply_component_mirror_rewrites_api_host() {
        let url = "https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest";
        assert_eq!(
            apply_component_mirror(url, "https://ghproxy.example.com/gh"),
            "https://ghproxy.example.com/gh/repos/yt-dlp/yt-dlp/releases/latest"
        );
    }

    // 配置后改写 github.com 前缀（资产直链下载）；末尾 `/` 与首尾空白容错。
    #[test]
    fn apply_component_mirror_rewrites_asset_download_host() {
        let url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg.zip";
        assert_eq!(
            apply_component_mirror(url, "  https://ghproxy.example.com/gh/  "),
            "https://ghproxy.example.com/gh/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg.zip"
        );
    }

    // 不匹配已知前缀的 URL 原样透传（不误伤第三方镜像/官网 URL）。
    #[test]
    fn apply_component_mirror_leaves_unrelated_urls_untouched() {
        let url = "https://fluxdown.zerx.dev/api/components/ffmpeg";
        assert_eq!(
            apply_component_mirror(url, "https://ghproxy.example.com/gh"),
            url
        );
    }
}
