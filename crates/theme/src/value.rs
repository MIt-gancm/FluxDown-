//! token 值：wire 语法（颜色 / 数值 / 字体 / 阴影 / 引用）的解析与规范化输出。
//!
//! 颜色在内存中保持 [`Hsla`]，与 Base 组件消费的类型一致；输出统一为小写
//! `#rrggbbaa`（通道 `round(c * 255)`），与 gpui `Rgba` 的序列化相同。

use gpui::{Hsla, Rgba, rgba};
use serde_json::{Number, Value};

use crate::TokenKind;
use crate::json::OrderedJson;

/// 阴影缺省颜色：黑色 46/255 alpha，与内置主题的阴影色相同。
pub(crate) const SHADOW_DEFAULT_COLOR: u32 = 0x0000_002E;

/// 已解析的单个 token 值。
#[derive(Debug, Clone, PartialEq)]
pub enum TokenValue {
    Color(Hsla),
    /// px、字重或比例。
    Number(f32),
    Font(String),
    Shadow(Vec<ShadowValue>),
}

/// 单层阴影（px）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShadowValue {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: Hsla,
    pub inset: bool,
}

impl TokenValue {
    /// 规范化 wire 值（颜色 `#rrggbbaa`、整数去掉小数点）。
    #[must_use]
    pub fn to_json(&self) -> Value {
        self.to_ordered_json().to_value()
    }

    /// 同 [`Self::to_json`]，阴影层键序固定为 `x y blur spread color inset`。
    #[must_use]
    pub fn to_ordered_json(&self) -> OrderedJson {
        match self {
            Self::Color(color) => color_hex(*color).into(),
            Self::Number(number) => number_value(*number).into(),
            Self::Font(font) => font.clone().into(),
            Self::Shadow(layers) => OrderedJson::Array(layers.iter().map(shadow_json).collect()),
        }
    }

    pub(crate) fn as_color(&self) -> Option<Hsla> {
        match self {
            Self::Color(color) => Some(*color),
            _ => None,
        }
    }

    /// 值是否能填入 `kind` 类型的 token（数值类 kind 之间互通）。
    pub(crate) fn fits(&self, kind: TokenKind) -> bool {
        matches!(
            (self, kind),
            (Self::Color(_), TokenKind::Color)
                | (
                    Self::Number(_),
                    TokenKind::Length
                        | TokenKind::Radius
                        | TokenKind::FontWeight
                        | TokenKind::Number
                )
                | (Self::Font(_), TokenKind::FontFamily)
                | (Self::Shadow(_), TokenKind::Shadow)
        )
    }
}

fn shadow_json(layer: &ShadowValue) -> OrderedJson {
    ordered_json!({
        "x": (number_value(layer.x)),
        "y": (number_value(layer.y)),
        "blur": (number_value(layer.blur)),
        "spread": (number_value(layer.spread)),
        "color": (color_hex(layer.color)),
        "inset": (layer.inset),
    })
}

/// `f32` → JSON number：整数输出为整数，其余取 `f32` 的最短十进制表示
/// （`0.38` 而不是 `0.3799999952316284`）。
#[must_use]
pub fn number_value(number: f32) -> Value {
    if !number.is_finite() {
        return Value::Null;
    }
    if number.fract() == 0. && number.abs() < 1e9 {
        return Value::from(number as i64);
    }
    format!("{number}")
        .parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map_or(Value::Null, Value::Number)
}

/// 颜色 → 小写 `#rrggbbaa`。
#[must_use]
pub fn color_hex(color: Hsla) -> String {
    let rgba = Rgba::from(color);
    let channel = |value: f32| (value.clamp(0., 1.) * 255.).round() as u8;
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        channel(rgba.r),
        channel(rgba.g),
        channel(rgba.b),
        channel(rgba.a)
    )
}

/// `#RRGGBB` / `#RRGGBBAA`（大小写不限）→ 颜色。
#[must_use]
pub fn parse_hex_color(text: &str) -> Option<Hsla> {
    let hex = text.strip_prefix('#')?;
    if !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(hex, 16).ok()?;
    match hex.len() {
        6 => Some(Hsla::from(rgba((value << 8) | 0xFF))),
        8 => Some(Hsla::from(rgba(value))),
        _ => None,
    }
}

/// `0xRRGGBBAA` → 颜色。
pub(crate) fn rgba_color(value: u32) -> Hsla {
    Hsla::from(rgba(value))
}

/// 引用：`{group.key}` 或 `{group.key}/NN`（NN 为 0~100 的不透明度百分比，乘到源 alpha 上）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Reference {
    pub path: String,
    /// 0 ~ 1。
    pub opacity: Option<f32>,
}

/// 颜色来源：字面量或引用。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColorSource {
    Literal(Hsla),
    Ref(Reference),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WireShadow {
    pub x: f32,
    pub y: f32,
    pub blur: f32,
    pub spread: f32,
    pub color: ColorSource,
    pub inset: bool,
}

/// 文件中的一个 token 值（尚未解析引用）。
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WireValue {
    Literal(TokenValue),
    Ref(Reference),
    Shadow(Vec<WireShadow>),
}

/// 字符串以 `{` 开头即按引用解析；返回 `None` 表示不是引用语法。
pub(crate) fn parse_reference(text: &str) -> Option<Result<Reference, String>> {
    let rest = text.strip_prefix('{')?;
    let Some((path, tail)) = rest.split_once('}') else {
        return Some(Err(format!("引用缺少右花括号：{text}")));
    };
    let valid_path = !path.is_empty()
        && path.split('.').all(|segment| {
            !segment.is_empty() && segment.bytes().all(|b| b.is_ascii_alphanumeric())
        });
    if !valid_path {
        return Some(Err(format!("引用路径非法：{text}")));
    }
    let opacity = if tail.is_empty() {
        None
    } else {
        let Some(percent) = tail.strip_prefix('/').and_then(|p| p.parse::<f32>().ok()) else {
            return Some(Err(format!("引用透明度后缀非法：{text}")));
        };
        if !(0. ..=100.).contains(&percent) {
            return Some(Err(format!("引用透明度须在 0~100：{text}")));
        }
        Some(percent / 100.)
    };
    Some(Ok(Reference {
        path: path.to_owned(),
        opacity,
    }))
}

/// 按 token 类型解析 wire 值；错误信息用于诊断。
pub(crate) fn parse_wire(kind: TokenKind, value: &Value) -> Result<WireValue, String> {
    if let Value::String(text) = value
        && let Some(reference) = parse_reference(text)
    {
        let reference = reference?;
        if reference.opacity.is_some() && kind != TokenKind::Color {
            return Err(format!("只有颜色引用可带透明度：{text}"));
        }
        return Ok(WireValue::Ref(reference));
    }
    match kind {
        TokenKind::Color => value
            .as_str()
            .and_then(parse_hex_color)
            .map(|color| WireValue::Literal(TokenValue::Color(color)))
            .ok_or_else(|| format!("颜色须为 #RRGGBB / #RRGGBBAA 或引用：{value}")),
        TokenKind::Length | TokenKind::Radius | TokenKind::FontWeight | TokenKind::Number => value
            .as_f64()
            .map(|number| number as f32)
            .filter(|number| number.is_finite())
            .map(|number| WireValue::Literal(TokenValue::Number(number)))
            .ok_or_else(|| format!("须为数字或引用：{value}")),
        TokenKind::FontFamily => value
            .as_str()
            .filter(|font| !font.trim().is_empty())
            .map(|font| WireValue::Literal(TokenValue::Font(font.to_owned())))
            .ok_or_else(|| format!("字体须为非空字符串：{value}")),
        TokenKind::Shadow => parse_shadow(value).map(WireValue::Shadow),
    }
}

fn parse_shadow(value: &Value) -> Result<Vec<WireShadow>, String> {
    let Value::Array(layers) = value else {
        return Err(format!("阴影须为数组或引用：{value}"));
    };
    layers
        .iter()
        .enumerate()
        .map(|(index, layer)| {
            let Value::Object(object) = layer else {
                return Err(format!("阴影第 {index} 层须为对象"));
            };
            let number = |key: &str| -> Result<f32, String> {
                match object.get(key) {
                    None => Ok(0.),
                    Some(value) => value
                        .as_f64()
                        .map(|number| number as f32)
                        .filter(|number| number.is_finite())
                        .ok_or_else(|| format!("阴影第 {index} 层 {key} 须为数字")),
                }
            };
            let color = match object.get("color") {
                None => ColorSource::Literal(rgba_color(SHADOW_DEFAULT_COLOR)),
                Some(Value::String(text)) => match parse_reference(text) {
                    Some(reference) => ColorSource::Ref(reference?),
                    None => ColorSource::Literal(
                        parse_hex_color(text)
                            .ok_or_else(|| format!("阴影第 {index} 层 color 非法：{text}"))?,
                    ),
                },
                Some(other) => return Err(format!("阴影第 {index} 层 color 非法：{other}")),
            };
            let inset = match object.get("inset") {
                None => false,
                Some(Value::Bool(inset)) => *inset,
                Some(other) => return Err(format!("阴影第 {index} 层 inset 须为布尔：{other}")),
            };
            Ok(WireShadow {
                x: number("x")?,
                y: number("y")?,
                blur: number("blur")?,
                spread: number("spread")?,
                color,
                inset,
            })
        })
        .collect()
}

/// 合法字面量（含颜色全为字面量的阴影）的规范 JSON；引用、含引用的阴影与非法值返回 `None`。
pub(crate) fn normalized_literal(kind: TokenKind, value: &Value) -> Option<Value> {
    match parse_wire(kind, value).ok()? {
        WireValue::Literal(literal) => Some(literal.to_json()),
        WireValue::Ref(_) => None,
        WireValue::Shadow(layers) => layers
            .iter()
            .map(|layer| match layer.color {
                ColorSource::Literal(color) => Some(
                    shadow_json(&ShadowValue {
                        x: layer.x,
                        y: layer.y,
                        blur: layer.blur,
                        spread: layer.spread,
                        color,
                        inset: layer.inset,
                    })
                    .to_value(),
                ),
                ColorSource::Ref(_) => None,
            })
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
    }
}

/// 阴影层对象的规范键序；层内其余键按 `Map` 迭代顺序排在后面。
const SHADOW_KEYS: [&str; 6] = ["x", "y", "blur", "spread", "color", "inset"];

/// 文件中阴影 token 的原始值 → 规范键序（非数组 / 非对象层原样）。
pub(crate) fn ordered_shadow_wire(value: &Value) -> OrderedJson {
    let Value::Array(layers) = value else {
        return value.clone().into();
    };
    OrderedJson::Array(
        layers
            .iter()
            .map(|layer| {
                let Value::Object(object) = layer else {
                    return layer.clone().into();
                };
                let known = SHADOW_KEYS.iter().filter_map(|key| {
                    object
                        .get(*key)
                        .map(|value| ((*key).to_owned(), value.clone().into()))
                });
                let rest = object
                    .iter()
                    .filter(|(key, _)| !SHADOW_KEYS.contains(&key.as_str()))
                    .map(|(key, value)| (key.clone(), value.clone().into()));
                OrderedJson::Object(known.chain(rest).collect())
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use gpui::{Hsla, rgb};
    use serde_json::json;

    use super::{color_hex, number_value, parse_hex_color, parse_reference};

    #[test]
    fn hex_round_trips_and_matches_rgb_constructor() {
        assert_eq!(parse_hex_color("#3B82F6"), Some(Hsla::from(rgb(0x3B82F6))));
        assert_eq!(
            parse_hex_color("#3b82f61a").map(color_hex).as_deref(),
            Some("#3b82f61a")
        );
        assert_eq!(parse_hex_color("#fff"), None);
        assert_eq!(parse_hex_color("3B82F6"), None);
        assert_eq!(parse_hex_color("#+b82f6"), None);
    }

    #[test]
    fn numbers_use_shortest_repr() {
        assert_eq!(number_value(28.), json!(28));
        assert_eq!(number_value(0.38), json!(0.38));
        assert_eq!(number_value(16.5), json!(16.5));
    }

    #[test]
    fn references_parse_opacity_percent() {
        let reference = parse_reference("{colors.primary}/20").and_then(Result::ok);
        assert_eq!(
            reference.map(|r| (r.path, r.opacity)),
            Some(("colors.primary".to_owned(), Some(0.2)))
        );
        assert!(matches!(
            parse_reference("{colors.primary}/120"),
            Some(Err(_))
        ));
        assert!(matches!(parse_reference("{colors..primary}"), Some(Err(_))));
        assert!(parse_reference("#ffffff").is_none());
    }
}
