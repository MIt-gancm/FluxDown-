//! 设置能力主题库端口的文件系统实现：`<data_dir>/themes/<id>.json`，原文原样保存。

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use fluxdown_ui_settings::{StoredTheme, ThemeInfo, ThemeLibrary};

const EXTENSION: &str = "json";
/// 清洗后 id 的最大长度（字符均为 ASCII）。
const MAX_ID_LEN: usize = 64;

/// 每个主题一个 `<id>.json`；id 只含 `a-z0-9-_`，因此总是合法且不会越出目录的文件名。
pub struct FsThemeLibrary {
    dir: PathBuf,
}

impl FsThemeLibrary {
    #[must_use]
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path(&self, id: &str) -> io::Result<PathBuf> {
        if sanitize_id(id).as_deref() == Some(id) {
            Ok(self.dir.join(format!("{id}.{EXTENSION}")))
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("invalid theme id: {id}"),
            ))
        }
    }
}

impl ThemeLibrary for FsThemeLibrary {
    fn list(&self) -> io::Result<Vec<ThemeInfo>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut themes = Vec::new();
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some(EXTENSION) {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            // 手工放入的非规范文件名不被引用（偏好只会写规范 id）。
            if sanitize_id(id).as_deref() != Some(id) {
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_file() {
                themes.push(ThemeInfo {
                    id: id.to_owned(),
                    modified: metadata.modified().ok(),
                });
            }
        }
        themes.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(themes)
    }

    fn load(&self, id: &str) -> io::Result<StoredTheme> {
        let path = self.path(id)?;
        let text = fs::read_to_string(&path)?;
        let modified = fs::metadata(&path).and_then(|meta| meta.modified()).ok();
        Ok(StoredTheme {
            info: ThemeInfo {
                id: id.to_owned(),
                modified,
            },
            text,
        })
    }

    fn save(&self, text: &str, preferred_id: Option<&str>) -> io::Result<String> {
        fs::create_dir_all(&self.dir)?;
        let base = preferred_id
            .and_then(sanitize_id)
            .unwrap_or_else(timestamp_id);
        let mut suffix = 1u32;
        loop {
            let id = if suffix == 1 {
                base.clone()
            } else {
                suffixed_id(&base, suffix)
            };
            match write_new(&self.dir.join(format!("{id}.{EXTENSION}")), text) {
                Ok(()) => return Ok(id),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => suffix += 1,
                Err(error) => return Err(error),
            }
        }
    }

    fn delete(&self, id: &str) -> io::Result<()> {
        match fs::remove_file(self.path(id)?) {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        }
    }
}

/// 只在目标不存在时创建并写入：并发导入也不会覆盖已有主题。写失败时删除半成品。
fn write_new(path: &Path, text: &str) -> io::Result<()> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    if written.is_err() {
        let _ = fs::remove_file(path);
    }
    written
}

/// `meta.id` → 库 id：小写，`[a-z0-9_-]` 之外的字符折叠为单个 `-`，去掉首尾 `-`，
/// 截断到 [`MAX_ID_LEN`]；清洗后为空则 `None`。
fn sanitize_id(raw: &str) -> Option<String> {
    let mut id = String::with_capacity(raw.len().min(MAX_ID_LEN));
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if id.len() >= MAX_ID_LEN {
            break;
        }
        if ch.is_ascii_alphanumeric() || ch == '_' {
            id.push(ch);
        } else if !id.is_empty() && !id.ends_with('-') {
            id.push('-');
        }
    }
    let id = id.trim_end_matches('-');
    (!id.is_empty()).then(|| id.to_owned())
}

/// `<base>-<suffix>`：先截断 `base` 给后缀留出空间（去掉截断后尾部的 `-`），
/// 结果仍满足 `sanitize_id(id) == id`。
fn suffixed_id(base: &str, suffix: u32) -> String {
    let suffix = format!("-{suffix}");
    // base 已清洗，全为 ASCII，按字节截断不会切到字符中间。
    let keep = base.len().min(MAX_ID_LEN - suffix.len());
    let head = base[..keep].trim_end_matches('-');
    format!("{head}{suffix}")
}

fn timestamp_id() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    format!("theme-{millis}")
}

#[cfg(test)]
mod tests {
    use fluxdown_ui_theme::{ExportMode, ThemeDocument};

    use super::*;

    /// 每个测试独占的临时目录，drop 时清理。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos());
            Self(std::env::temp_dir().join(format!(
                "fluxdown-themes-{label}-{}-{nanos}",
                std::process::id()
            )))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn save_load_delete_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new("crud");
        let library = FsThemeLibrary::new(temp.0.join("themes"));
        assert!(library.list()?.is_empty(), "missing dir lists empty");

        let text = "{ \"meta\": { \"id\": \"Nord Square\" } }\n";
        let id = library.save(text, Some("Nord Square"))?;
        assert_eq!(id, "nord-square");
        assert!(temp.0.join("themes/nord-square.json").is_file());

        let stored = library.load(&id)?;
        assert_eq!(stored.text, text, "text stored verbatim");
        assert_eq!(stored.info.id, id);
        assert!(stored.info.modified.is_some());
        let ids: Vec<String> = library.list()?.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["nord-square"]);

        library.delete(&id)?;
        assert!(library.list()?.is_empty());
        assert_eq!(
            library.load(&id).err().ok_or("expected an error")?.kind(),
            io::ErrorKind::NotFound
        );
        library.delete(&id)?;
        Ok(())
    }

    #[test]
    fn conflicting_ids_get_suffixes_and_never_overwrite() -> Result<(), Box<dyn std::error::Error>>
    {
        let temp = TempDir::new("conflict");
        let library = FsThemeLibrary::new(temp.0.clone());
        assert_eq!(library.save("a", Some("ocean"))?, "ocean");
        assert_eq!(library.save("b", Some("Ocean"))?, "ocean-2");
        assert_eq!(library.save("c", Some("ocean"))?, "ocean-3");
        assert_eq!(library.load("ocean")?.text, "a");
        assert_eq!(library.load("ocean-2")?.text, "b");
        let ids: Vec<String> = library.list()?.into_iter().map(|t| t.id).collect();
        assert_eq!(ids, ["ocean", "ocean-2", "ocean-3"]);
        Ok(())
    }

    #[test]
    fn max_length_ids_stay_valid_after_suffixing() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new("maxlen");
        let library = FsThemeLibrary::new(temp.0.clone());
        // 第 62 位是 `-`：给 `-2` 截断到 62 字符后必须去掉尾部 `-`，避免 `--2`。
        let preferred = format!("{}-bb", "a".repeat(61));
        assert_eq!(preferred.len(), MAX_ID_LEN);
        let ids = (0..3)
            .map(|index| library.save(&index.to_string(), Some(&preferred)))
            .collect::<io::Result<Vec<_>>>()?;
        assert_eq!(ids[0], preferred);
        assert_eq!(ids[1], format!("{}-2", "a".repeat(61)));
        assert_eq!(ids[2], format!("{}-3", "a".repeat(61)));
        for (index, id) in ids.iter().enumerate() {
            assert!(id.len() <= MAX_ID_LEN, "{id}");
            assert_eq!(sanitize_id(id).as_deref(), Some(id.as_str()));
            assert_eq!(library.load(id)?.text, index.to_string());
        }
        let mut listed: Vec<String> = library.list()?.into_iter().map(|t| t.id).collect();
        let mut expected = ids.clone();
        listed.sort();
        expected.sort();
        assert_eq!(listed, expected);
        for id in &ids {
            library.delete(id)?;
        }
        assert!(library.list()?.is_empty());
        Ok(())
    }

    #[test]
    fn missing_or_unusable_meta_id_falls_back_to_timestamp()
    -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new("timestamp");
        let library = FsThemeLibrary::new(temp.0.clone());
        for preferred in [None, Some("  "), Some("../../")] {
            let id = library.save("{}", preferred)?;
            assert!(id.starts_with("theme-"), "{preferred:?} → {id}");
        }
        assert_eq!(library.list()?.len(), 3);
        Ok(())
    }

    #[test]
    fn ids_cannot_escape_the_library_directory() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new("escape");
        let library = FsThemeLibrary::new(temp.0.join("themes"));
        fs::create_dir_all(&temp.0)?;
        fs::write(temp.0.join("secret.json"), "{}")?;
        for id in ["../secret", "a/b", "", "Upper"] {
            assert_eq!(
                library.load(id).err().ok_or("expected an error")?.kind(),
                io::ErrorKind::InvalidInput,
                "{id}"
            );
            assert_eq!(
                library.delete(id).err().ok_or("expected an error")?.kind(),
                io::ErrorKind::InvalidInput
            );
        }
        assert!(temp.0.join("secret.json").exists());
        assert_eq!(
            sanitize_id("../../etc/passwd").as_deref(),
            Some("etc-passwd")
        );
        assert_eq!(sanitize_id("Nord  Square!").as_deref(), Some("nord-square"));
        assert_eq!(
            sanitize_id(&"x".repeat(100)).map(|id| id.len()),
            Some(MAX_ID_LEN)
        );
        Ok(())
    }

    #[test]
    fn import_save_reload_parse_keeps_unknown_keys() -> Result<(), Box<dyn std::error::Error>> {
        let temp = TempDir::new("roundtrip");
        let library = FsThemeLibrary::new(temp.0.clone());
        let text = r##"{
  "format": "fluxdown.gpui-theme",
  "schemaVersion": 2,
  "meta": { "id": "ocean", "name": "Ocean", "x-origin": "gallery" },
  "x-top": { "keep": [1, 2] },
  "extends": "builtin:nord",
  "dark": { "colors": { "primary": "#0ea5e9", "x-glow": "#ffffff" } }
}"##;
        let (document, _) = ThemeDocument::parse(text)?;
        let preferred = document.meta.as_ref().and_then(|meta| meta.id.as_deref());
        let id = library.save(text, preferred)?;

        let reloaded = FsThemeLibrary::new(temp.0.clone());
        let listed: Vec<String> = reloaded.list()?.into_iter().map(|t| t.id).collect();
        assert_eq!(listed, std::slice::from_ref(&id));
        let stored = reloaded.load(&id)?;
        assert_eq!(stored.text, text);
        let (reparsed, _) = ThemeDocument::parse(&stored.text)?;
        assert_eq!(reparsed, document);
        let exported: serde_json::Value =
            serde_json::from_str(&reparsed.to_json_pretty(ExportMode::Diff))?;
        assert_eq!(exported["x-top"]["keep"], serde_json::json!([1, 2]));
        assert_eq!(exported["meta"]["x-origin"], "gallery");
        assert_eq!(exported["dark"]["colors"]["x-glow"], "#ffffff");
        Ok(())
    }
}
