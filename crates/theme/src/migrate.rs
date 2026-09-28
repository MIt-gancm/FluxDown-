//! 主题文件迁移。
//!
//! - v2 起的版本升级用声明式表 [`MIGRATIONS`]：每步把 token 层内的路径改名/移动，
//!   同一张表经 `registry.json` 导出给网站端执行。
//! - v1 内部格式（`FluxThemeDefinition` 的 serde：顶层 `schemaVersion: 1` / `name` /
//!   `author` 与 `light` / `dark` 两份完整 snake_case 快照）由代码迁移为 v2；键改名
//!   部分同样以数据表 [`V1_KEY_RENAMES`] 表达。

use serde_json::{Map, Value};

use crate::document::{get_path, remove_path};
use crate::{Diagnostic, DiagnosticKind, THEME_FORMAT, THEME_SCHEMA_VERSION};

/// 一次 schema 升级：把旧版本文件升到 `to_version`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Migration {
    pub to_version: u32,
    /// token 层内的 `(旧路径, 新路径)`。
    pub renames: &'static [(&'static str, &'static str)],
}

/// v2 之后的升级步骤（按 `to_version` 升序）。v2 是首个公开版本，暂无步骤。
pub static MIGRATIONS: &[Migration] = &[];

/// v1 快照中 snake_case 键 → v2 camelCase 键（token 层内路径）。
pub static V1_KEY_RENAMES: &[(&str, &str)] = &[
    ("colors.surface_foreground", "colors.surfaceForeground"),
    ("colors.primary_foreground", "colors.primaryForeground"),
    ("colors.secondary_foreground", "colors.secondaryForeground"),
    ("colors.muted_foreground", "colors.mutedForeground"),
    ("colors.accent_foreground", "colors.accentForeground"),
    (
        "colors.destructive_foreground",
        "colors.destructiveForeground",
    ),
    ("typography.mono_md", "typography.monoMd"),
    ("typography.xs.line_height", "typography.xs.lineHeight"),
    ("typography.sm.line_height", "typography.sm.lineHeight"),
    ("typography.md.line_height", "typography.md.lineHeight"),
    ("typography.lg.line_height", "typography.lg.lineHeight"),
    ("typography.xl.line_height", "typography.xl.lineHeight"),
    (
        "typography.monoMd.line_height",
        "typography.monoMd.lineHeight",
    ),
];

const V1_GROUPS: [&str; 5] = ["colors", "radius", "spacing", "typography", "shadow"];

/// 是否形如 v1 内部格式：无 `format`，`schemaVersion` 为 1，或缺失但 `light` / `dark`
/// 含 snake_case 键的 Base 分组。
pub(crate) fn is_v1(object: &Map<String, Value>) -> bool {
    if object.contains_key("format") {
        return false;
    }
    let has_v1_layer = ["light", "dark"].iter().any(|key| {
        object
            .get(*key)
            .and_then(Value::as_object)
            .is_some_and(|layer| V1_GROUPS.iter().any(|group| layer.contains_key(*group)))
    });
    if !has_v1_layer {
        return false;
    }
    match object.get("schemaVersion") {
        Some(version) => version.as_u64() == Some(1),
        None => ["light", "dark"].iter().any(|key| {
            object
                .get(*key)
                .and_then(Value::as_object)
                .is_some_and(has_snake_case_key)
        }),
    }
}

fn has_snake_case_key(map: &Map<String, Value>) -> bool {
    map.iter()
        .any(|(key, value)| key.contains('_') || value.as_object().is_some_and(has_snake_case_key))
}

/// v1 → v2：`name` / `author` 移入 `meta`，两份快照键改名、阴影改为扁平字段，
/// 基底取 `builtin:default`（快照是完整值，导出 diff 时自动去掉与默认相同的项）。
pub(crate) fn migrate_v1(
    object: Map<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Map<String, Value> {
    let mut output = Map::new();
    output.insert("format".into(), Value::String(THEME_FORMAT.into()));
    output.insert("schemaVersion".into(), Value::from(THEME_SCHEMA_VERSION));
    let mut meta = Map::new();
    let mut rest = Map::new();
    for (key, value) in object {
        match key.as_str() {
            "schemaVersion" => {}
            "name" | "author" => {
                if !value.is_null() {
                    meta.insert(key, value);
                }
            }
            "light" | "dark" => {
                let layer = match value {
                    Value::Object(layer) => Value::Object(migrate_v1_layer(layer)),
                    other => other,
                };
                rest.insert(key, layer);
            }
            _ => {
                rest.insert(key, value);
            }
        }
    }
    if !meta.is_empty() {
        output.insert("meta".into(), Value::Object(meta));
    }
    output.insert("extends".into(), Value::String("builtin:default".into()));
    output.extend(rest);
    diagnostics.push(Diagnostic::new(
        "schemaVersion",
        DiagnosticKind::Migrated,
        "已从 v1 内部格式迁移到 v2",
    ));
    output
}

fn migrate_v1_layer(mut layer: Map<String, Value>) -> Map<String, Value> {
    for (from, to) in V1_KEY_RENAMES {
        rename_path(&mut layer, from, to);
    }
    if let Some(Value::Object(shadow)) = layer.get_mut("shadow") {
        for (_, levels) in shadow.iter_mut() {
            if let Value::Array(levels) = levels {
                for level in levels.iter_mut() {
                    if let Value::Object(level) = level {
                        *level = migrate_v1_shadow(level);
                    }
                }
            }
        }
    }
    layer
}

/// `{ color, offset: {x, y}, blur_radius, spread_radius, inset }` → `{ x, y, blur, spread, color, inset }`。
fn migrate_v1_shadow(level: &Map<String, Value>) -> Map<String, Value> {
    let mut output = Map::new();
    if let Some(x) = get_path(level, "offset.x") {
        output.insert("x".into(), x.clone());
    }
    if let Some(y) = get_path(level, "offset.y") {
        output.insert("y".into(), y.clone());
    }
    for (from, to) in [
        ("blur_radius", "blur"),
        ("spread_radius", "spread"),
        ("color", "color"),
        ("inset", "inset"),
    ] {
        if let Some(value) = level.get(from) {
            output.insert(to.into(), value.clone());
        }
    }
    output
}

/// 按表把 `from_version` 之后的步骤依次作用到一个 token 层。
pub(crate) fn apply_steps(layer: &mut Map<String, Value>, from_version: u32, steps: &[Migration]) {
    for step in steps.iter().filter(|step| step.to_version > from_version) {
        for (from, to) in step.renames {
            rename_path(layer, from, to);
        }
    }
}

/// 把 `from` 处的值移到 `to`（目标已存在时不覆盖）；移走后清理空父对象。
fn rename_path(layer: &mut Map<String, Value>, from: &str, to: &str) {
    if get_path(layer, to).is_some() {
        return;
    }
    let Some(value) = remove_path(layer, from) else {
        return;
    };
    insert_path(layer, to, value);
}

fn insert_path(map: &mut Map<String, Value>, path: &str, value: Value) {
    match path.split_once('.') {
        None => {
            map.insert(path.to_owned(), value);
        }
        Some((head, rest)) => {
            let child = map
                .entry(head.to_owned())
                .or_insert_with(|| Value::Object(Map::new()));
            if let Value::Object(child) = child {
                insert_path(child, rest, value);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{Migration, apply_steps, is_v1};

    #[test]
    fn declarative_steps_rename_only_newer_versions() {
        static STEPS: &[Migration] = &[
            Migration {
                to_version: 2,
                renames: &[("colors.old", "colors.skipped")],
            },
            Migration {
                to_version: 3,
                renames: &[
                    ("colors.old", "colors.new"),
                    ("density.row", "density.taskRow"),
                ],
            },
        ];
        let Value::Object(mut layer) = json!({
            "colors": { "old": "#112233" },
            "density": { "row": 40, "taskRow": 50 },
        }) else {
            unreachable!();
        };
        apply_steps(&mut layer, 2, STEPS);
        assert_eq!(
            Value::Object(layer),
            json!({
                "colors": { "new": "#112233" },
                "density": { "row": 40, "taskRow": 50 },
            })
        );
    }

    #[test]
    fn v1_detection_requires_v1_shape() {
        let object = |value: Value| value.as_object().cloned().unwrap_or_default();
        assert!(is_v1(&object(
            json!({ "schemaVersion": 1, "light": { "colors": {} } })
        )));
        assert!(is_v1(&object(
            json!({ "dark": { "colors": { "muted_foreground": "#000000ff" } } })
        )));
        assert!(!is_v1(&object(
            json!({ "dark": { "colors": { "mutedForeground": "#000000" } } })
        )));
        assert!(!is_v1(&object(
            json!({ "schemaVersion": 2, "light": { "colors": {} } })
        )));
        assert!(!is_v1(&object(
            json!({ "format": "x", "schemaVersion": 1, "light": { "colors": {} } })
        )));
    }
}
