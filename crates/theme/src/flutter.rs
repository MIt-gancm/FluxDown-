//! Flutter 客户端主题 JSON（`FluxThemeTokens.toJson`，`schemaVersion: 2`）→ v2 主题文件。
//!
//! Flutter 文件只描述一个外观（`appearance: dark | light`）：颜色写入对应模式层，
//! 基底取 `builtin:default`，另一模式保持默认。映射与内置预设相同（Layer0 → Base）：
//! 只转换文件中实际出现的字段；Flutter 专有颜色（element / input / dialog / switch /
//! segmentPalette 等）与 `metrics` 没有 GPUI 对应项，不转换。Flutter 颜色串为
//! `AARRGGBB`（可带 `#`，6 位时 alpha 为 FF），转换为 `#rrggbbaa`。

use serde_json::{Map, Value};

use crate::builtin::BuiltinBase;
use crate::document::get_path;
use crate::json::OrderedJson;
use crate::{Diagnostic, DiagnosticKind, THEME_FORMAT, THEME_SCHEMA_VERSION};

/// Flutter 颜色路径 → v2 token 路径（一个 Flutter 字段可映射到多个 token）。
pub static FLUTTER_COLOR_MAP: &[(&str, &[&str])] = &[
    ("surface.background", &["colors.background"]),
    ("surface.surface1", &["colors.surface"]),
    ("surface.surface2", &["colors.secondary", "colors.muted"]),
    (
        "text.primary",
        &[
            "colors.foreground",
            "colors.surfaceForeground",
            "colors.secondaryForeground",
        ],
    ),
    ("text.secondary", &["colors.mutedForeground"]),
    (
        "accent.color",
        &["colors.primary", "colors.accentForeground", "colors.ring"],
    ),
    ("accent.foreground", &["colors.primaryForeground"]),
    ("accent.background", &["colors.accent"]),
    ("border.default", &["colors.border", "colors.input"]),
    ("status.error", &["colors.destructive"]),
    ("status.success", &["colors.success"]),
    ("status.warning", &["colors.warning"]),
];

/// 原样复制到 `meta` 的 Flutter 顶层字符串字段。
const FLUTTER_META_KEYS: [&str; 2] = ["name", "author"];
/// Flutter 文件可声明的外观，同时是颜色写入的模式层名。
const FLUTTER_APPEARANCES: [&str; 2] = ["dark", "light"];

/// `registry.json` 的 `flutterAdapter`：网站端据此执行与 [`convert`] 相同的适配。
pub(crate) fn adapter_json() -> OrderedJson {
    let color_map = FLUTTER_COLOR_MAP
        .iter()
        .map(|(from, to)| ordered_json!({ "from": (*from), "to": (*to) }))
        .collect::<Vec<_>>();
    ordered_json!({
        "appearances": (FLUTTER_APPEARANCES.as_slice()),
        "extends": (BuiltinBase::Default.extends_value()),
        "metaKeys": (FLUTTER_META_KEYS.as_slice()),
        "colorInput": "AARRGGBB or RRGGBB (optional leading #, surrounding whitespace trimmed, case-insensitive); RRGGBB means alpha ff",
        "colorOutput": "#rrggbbaa lowercase",
        "colorMap": color_map,
    })
}

/// 顶层 `appearance` 为 `dark` / `light` 且 `colors` 为对象。
pub(crate) fn is_flutter_theme(object: &Map<String, Value>) -> bool {
    object
        .get("appearance")
        .and_then(Value::as_str)
        .is_some_and(|appearance| FLUTTER_APPEARANCES.contains(&appearance))
        && object.get("colors").is_some_and(Value::is_object)
}

pub(crate) fn convert(
    object: &Map<String, Value>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Map<String, Value> {
    let mode = match object.get("appearance").and_then(Value::as_str) {
        Some("dark") => "dark",
        _ => "light",
    };
    let colors = object
        .get("colors")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut group = Map::new();
    for (source, targets) in FLUTTER_COLOR_MAP {
        let Some(raw) = get_path(&colors, source) else {
            continue;
        };
        let Some(hex) = raw.as_str().and_then(flutter_hex) else {
            diagnostics.push(Diagnostic::new(
                format!("colors.{source}"),
                DiagnosticKind::InvalidValue,
                format!("Flutter 颜色非法：{raw}，已忽略"),
            ));
            continue;
        };
        for target in *targets {
            if let Some(key) = target.strip_prefix("colors.") {
                group.insert(key.into(), Value::String(hex.clone()));
            }
        }
    }

    let mut meta = Map::new();
    for key in FLUTTER_META_KEYS {
        if let Some(Value::String(value)) = object.get(key) {
            meta.insert(key.into(), Value::String(value.clone()));
        }
    }

    let mut output = Map::new();
    output.insert("format".into(), Value::String(THEME_FORMAT.into()));
    output.insert("schemaVersion".into(), Value::from(THEME_SCHEMA_VERSION));
    if !meta.is_empty() {
        output.insert("meta".into(), Value::Object(meta));
    }
    output.insert(
        "extends".into(),
        Value::String(BuiltinBase::Default.extends_value().into()),
    );
    if !group.is_empty() {
        let mut layer = Map::new();
        layer.insert("colors".into(), Value::Object(group));
        output.insert(mode.into(), Value::Object(layer));
    }
    diagnostics.push(Diagnostic::new(
        "format",
        DiagnosticKind::Migrated,
        format!("已从 Flutter FluxThemeJson（{mode}）转换；Flutter 专有颜色与 metrics 未转换"),
    ));
    output
}

/// `AARRGGBB` / `RRGGBB`（可带 `#`）→ `#rrggbbaa`。
fn flutter_hex(text: &str) -> Option<String> {
    let hex = text.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let argb = match hex.len() {
        6 => format!("ff{hex}"),
        8 => hex.to_owned(),
        _ => return None,
    }
    .to_ascii_lowercase();
    Some(format!("#{}{}", &argb[2..], &argb[..2]))
}

#[cfg(test)]
mod tests {
    use super::flutter_hex;

    #[test]
    fn argb_moves_alpha_to_end() {
        assert_eq!(flutter_hex("1a3b82f6").as_deref(), Some("#3b82f61a"));
        assert_eq!(flutter_hex("#3B82F6").as_deref(), Some("#3b82f6ff"));
        assert_eq!(flutter_hex("fff"), None);
    }
}
