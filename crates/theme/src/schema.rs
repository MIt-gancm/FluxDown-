//! 由注册表生成主题文件的 JSON Schema（draft 2020-12）。
//!
//! Schema 只描述结构与值语法供编辑器补全/校验；未知键允许（加载时原样保留）。
//! `minimum` / `maximum` 是编辑器提示：客户端加载越界值时 clamp 并记诊断，不拒绝文件。

use crate::builtin::BuiltinBase;
use crate::json::OrderedJson;
use crate::registry::{TOKENS, TokenKind, TokenSpec};
use crate::value::number_value;
use crate::{THEME_FORMAT, THEME_SCHEMA_URL, THEME_SCHEMA_VERSION};

const REF_PATTERN: &str = r"^\{[A-Za-z0-9]+(\.[A-Za-z0-9]+)+\}$";
const COLOR_REF_PATTERN: &str = r"^(#([0-9A-Fa-f]{6}|[0-9A-Fa-f]{8})|\{[A-Za-z0-9]+(\.[A-Za-z0-9]+)+\}(/(100|[0-9]{1,2})(\.[0-9]+)?)?)$";

/// 主题文件 JSON Schema。
#[must_use]
pub fn json_schema() -> OrderedJson {
    ordered_json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "$id": THEME_SCHEMA_URL,
        "title": "FluxDown GPUI Theme",
        "type": "object",
        "properties": {
            "$schema": { "type": "string" },
            "format": { "const": THEME_FORMAT },
            "schemaVersion": { "type": "integer", "minimum": 1, "default": THEME_SCHEMA_VERSION },
            "meta": {
                "type": "object",
                "properties": {
                    "id": { "type": "string" },
                    "name": { "type": "string" },
                    "author": { "type": "string" },
                    "version": { "type": "string" },
                    "description": { "type": "string" },
                },
            },
            "extends": {
                "enum": (BuiltinBase::ALL.iter().map(|base| base.extends_value()).collect::<Vec<_>>()),
                "default": (BuiltinBase::Default.extends_value()),
            },
            "tokens": { "$ref": "#/$defs/layer" },
            "dark": { "$ref": "#/$defs/layer" },
            "light": { "$ref": "#/$defs/layer" },
        },
        "$defs": {
            "layer": (layer_schema()),
            "color": { "type": "string", "pattern": COLOR_REF_PATTERN },
            "ref": { "type": "string", "pattern": REF_PATTERN },
            "shadowLayer": {
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" },
                    "blur": { "type": "number" },
                    "spread": { "type": "number" },
                    "color": { "$ref": "#/$defs/color" },
                    "inset": { "type": "boolean" },
                },
            },
        },
    })
}

/// 构建中的嵌套节点：子键按注册表首次出现的顺序排列。
enum SchemaNode {
    Token(OrderedJson),
    Group(Vec<(String, SchemaNode)>),
}

/// 把 `group.key[.prop]` 路径组织成嵌套对象 schema。
fn layer_schema() -> OrderedJson {
    let mut root = Vec::new();
    for spec in TOKENS {
        insert_schema(&mut root, spec.path, token_schema(spec));
    }
    object_schema(root)
}

fn insert_schema(node: &mut Vec<(String, SchemaNode)>, path: &str, schema: OrderedJson) {
    let (head, rest) = match path.split_once('.') {
        Some((head, rest)) => (head, Some(rest)),
        None => (path, None),
    };
    let index = match node.iter().position(|(key, _)| key == head) {
        Some(index) => index,
        None => {
            node.push((head.to_owned(), SchemaNode::Group(Vec::new())));
            node.len() - 1
        }
    };
    match (rest, &mut node[index].1) {
        (None, slot) => *slot = SchemaNode::Token(schema),
        (Some(rest), SchemaNode::Group(child)) => insert_schema(child, rest, schema),
        (Some(_), SchemaNode::Token(_)) => {}
    }
}

/// 中间节点 → `{ type: object, properties }`。
fn object_schema(node: Vec<(String, SchemaNode)>) -> OrderedJson {
    let properties = node
        .into_iter()
        .map(|(key, value)| {
            let value = match value {
                SchemaNode::Token(schema) => schema,
                SchemaNode::Group(child) => object_schema(child),
            };
            (key, value)
        })
        .collect();
    ordered_json!({ "type": "object", "properties": (OrderedJson::Object(properties)) })
}

fn token_schema(spec: &TokenSpec) -> OrderedJson {
    let mut schema = match spec.kind {
        TokenKind::Color => ordered_json!({ "$ref": "#/$defs/color" }),
        TokenKind::FontFamily => ordered_json!({ "type": "string", "minLength": 1 }),
        TokenKind::Shadow => ordered_json!({
            "anyOf": [
                { "type": "array", "items": { "$ref": "#/$defs/shadowLayer" } },
                { "$ref": "#/$defs/ref" },
            ],
        }),
        TokenKind::Length | TokenKind::Radius | TokenKind::FontWeight | TokenKind::Number => {
            let mut number = vec![("type".to_owned(), OrderedJson::from("number"))];
            if let Some((min, max)) = spec.range {
                number.push(("minimum".into(), number_value(min).into()));
                number.push(("maximum".into(), number_value(max).into()));
            }
            ordered_json!({ "anyOf": [(OrderedJson::Object(number)), { "$ref": "#/$defs/ref" }] })
        }
    };
    if let OrderedJson::Object(entries) = &mut schema {
        entries.push(("x-fluxdown-token".into(), spec.path.into()));
        entries.push(("x-fluxdown-kind".into(), spec.kind.wire_name().into()));
        if spec.kit_bound {
            entries.push(("readOnly".into(), true.into()));
        }
    }
    schema
}
