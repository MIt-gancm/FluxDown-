//! 构建脚本：确定应用版本号并注入 `FLUXDOWN_APP_VERSION` 编译期环境变量，供
//! downloader.rs 拼出 aria2 风格的默认 UA（`FluxDown/<版本>`）。
//!
//! 取值顺序：构建环境变量 `FLUXDOWN_APP_VERSION`（发布流水线按 tag 导出）→ 仓库根
//! pubspec.yaml 的 `version:` → 回退 "1.0"（独立打包 engine crate 等场景）。

use std::fs;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-env-changed=FLUXDOWN_APP_VERSION");
    let from_env = std::env::var("FLUXDOWN_APP_VERSION")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    if let Some(version) = from_env {
        println!("cargo:rustc-env=FLUXDOWN_APP_VERSION={version}");
        return;
    }

    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
    let pubspec = Path::new(&manifest_dir).join("../../pubspec.yaml");
    println!("cargo:rerun-if-changed={}", pubspec.display());

    // pubspec 版本形如 `version: 0.1.44+1`，取 `+` 前的语义版本。
    let version = fs::read_to_string(&pubspec)
        .ok()
        .and_then(|text| {
            text.lines().find_map(|line| {
                let rest = line.strip_prefix("version:")?;
                let v = rest.trim().split('+').next().unwrap_or("").trim();
                if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                }
            })
        })
        .unwrap_or_else(|| "1.0".to_string());
    println!("cargo:rustc-env=FLUXDOWN_APP_VERSION={version}");
}
