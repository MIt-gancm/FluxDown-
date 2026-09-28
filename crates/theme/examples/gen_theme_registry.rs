//! 生成主题注册表镜像与 fixture 期望值：
//!
//! - `website-v2/src/lib/gpui-theme/registry.json`
//! - `website-v2/public/schemas/gpui-theme.v2.json`
//! - `crates/theme/tests/fixtures/<name>.resolved.json`（每个 `<name>.json`）
//!
//! 运行：`cargo run -p fluxdown_ui_theme --example gen_theme_registry`。
//! 漂移守卫测试（`tests/generated.rs`）比较同一生成结果与已提交文件。

use std::path::{Path, PathBuf};
use std::{fs, io};

use fluxdown_ui_theme::{json_schema, registry_json, resolved_snapshot, to_pretty_json};

fn main() -> io::Result<()> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest.join("../..");
    write(
        &root.join("website-v2/src/lib/gpui-theme/registry.json"),
        &to_pretty_json(&registry_json()),
    )?;
    write(
        &root.join("website-v2/public/schemas/gpui-theme.v2.json"),
        &to_pretty_json(&json_schema()),
    )?;
    for source in fixture_sources(&manifest.join("tests/fixtures"))? {
        let text = fs::read_to_string(&source)?;
        write(
            &source.with_extension("resolved.json"),
            &to_pretty_json(&resolved_snapshot(&text)),
        )?;
    }
    Ok(())
}

/// `*.json`，不含 `*.resolved.json`，按文件名排序。
fn fixture_sources(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut sources: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".json") && !name.ends_with(".resolved.json"))
        })
        .collect();
    sources.sort();
    Ok(sources)
}

fn write(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::read_to_string(path).is_ok_and(|existing| existing == contents) {
        return Ok(());
    }
    fs::write(path, contents)?;
    println!("wrote {}", path.display());
    Ok(())
}
