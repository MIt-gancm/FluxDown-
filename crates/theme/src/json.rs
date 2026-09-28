//! 显式保序的 JSON 树：导出物（主题文件、`registry.json`、Schema、`*.resolved.json`）的
//! 规范键序由构造顺序决定，不依赖 `serde_json::Map` 的底层实现（`preserve_order` 是否被
//! 构建图中其他 crate 打开）。

use serde_json::Value;

/// 对象键按构造顺序输出的 JSON；[`OrderedJson::Value`] 叶子里嵌套的对象按
/// `serde_json::Map` 的迭代顺序输出（用于原样保留的未知键内容）。
#[derive(Debug, Clone, PartialEq)]
pub enum OrderedJson {
    Value(Value),
    Array(Vec<OrderedJson>),
    Object(Vec<(String, OrderedJson)>),
}

/// `{ "key": value, ... }` / `[ ... ]` 字面量 → [`OrderedJson`]，键序即书写顺序。
/// 非单个 token 的值表达式需用括号包起来。
macro_rules! ordered_json {
    ({ $($key:literal : $value:tt),* $(,)? }) => {
        $crate::json::OrderedJson::Object(vec![$(($key.to_owned(), ordered_json!($value))),*])
    };
    ([ $($value:tt),* $(,)? ]) => {
        $crate::json::OrderedJson::Array(vec![$(ordered_json!($value)),*])
    };
    ($value:expr) => {
        $crate::json::OrderedJson::from($value)
    };
}

impl OrderedJson {
    /// 对象中 `key` 对应的值。
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(entries) => entries
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// 对象的键（按输出顺序）；非对象为空。
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        let entries = match self {
            Self::Object(entries) => entries.as_slice(),
            _ => &[],
        };
        entries.iter().map(|(key, _)| key.as_str())
    }

    /// 转为 `serde_json::Value`（键序交给 `Map` 的实现）。
    #[must_use]
    pub fn to_value(&self) -> Value {
        match self {
            Self::Value(value) => value.clone(),
            Self::Array(items) => Value::Array(items.iter().map(Self::to_value).collect()),
            Self::Object(entries) => Value::Object(
                entries
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_value()))
                    .collect(),
            ),
        }
    }

    /// 两空格缩进（与 `serde_json::to_string_pretty` 同格式）+ 末尾换行。
    #[must_use]
    pub fn to_pretty(&self) -> String {
        let mut out = Vec::new();
        if write_node(&mut out, Node::Ordered(self), 0).is_err() {
            return String::new();
        }
        out.push(b'\n');
        String::from_utf8(out).unwrap_or_default()
    }
}

impl From<Value> for OrderedJson {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}

impl From<&str> for OrderedJson {
    fn from(value: &str) -> Self {
        Self::Value(Value::String(value.to_owned()))
    }
}

impl From<String> for OrderedJson {
    fn from(value: String) -> Self {
        Self::Value(Value::String(value))
    }
}

impl From<bool> for OrderedJson {
    fn from(value: bool) -> Self {
        Self::Value(Value::Bool(value))
    }
}

impl From<i32> for OrderedJson {
    fn from(value: i32) -> Self {
        Self::Value(Value::from(value))
    }
}

impl From<u32> for OrderedJson {
    fn from(value: u32) -> Self {
        Self::Value(Value::from(value))
    }
}

impl<T: Into<OrderedJson>> From<Vec<T>> for OrderedJson {
    fn from(items: Vec<T>) -> Self {
        Self::Array(items.into_iter().map(Into::into).collect())
    }
}

impl<T: Into<OrderedJson> + Copy> From<&[T]> for OrderedJson {
    fn from(items: &[T]) -> Self {
        Self::Array(items.iter().map(|item| (*item).into()).collect())
    }
}

#[derive(Clone, Copy)]
enum Node<'a> {
    Ordered(&'a OrderedJson),
    Plain(&'a Value),
}

const INDENT: &[u8] = b"  ";

fn write_node(out: &mut Vec<u8>, node: Node<'_>, depth: usize) -> serde_json::Result<()> {
    let value = match node {
        Node::Ordered(OrderedJson::Array(items)) => {
            return write_seq(
                out,
                b"[]",
                items.iter().map(|item| (None, Node::Ordered(item))),
                depth,
            );
        }
        Node::Ordered(OrderedJson::Object(entries)) => {
            return write_seq(
                out,
                b"{}",
                entries
                    .iter()
                    .map(|(key, value)| (Some(key.as_str()), Node::Ordered(value))),
                depth,
            );
        }
        Node::Ordered(OrderedJson::Value(value)) | Node::Plain(value) => value,
    };
    match value {
        Value::Array(items) => write_seq(
            out,
            b"[]",
            items.iter().map(|item| (None, Node::Plain(item))),
            depth,
        ),
        Value::Object(map) => write_seq(
            out,
            b"{}",
            map.iter()
                .map(|(key, value)| (Some(key.as_str()), Node::Plain(value))),
            depth,
        ),
        scalar => serde_json::to_writer(&mut *out, scalar),
    }
}

fn write_seq<'a>(
    out: &mut Vec<u8>,
    brackets: &[u8; 2],
    items: impl Iterator<Item = (Option<&'a str>, Node<'a>)>,
    depth: usize,
) -> serde_json::Result<()> {
    out.push(brackets[0]);
    let mut empty = true;
    for (key, node) in items {
        out.extend_from_slice(if empty { b"\n" } else { b",\n" });
        empty = false;
        indent(out, depth + 1);
        if let Some(key) = key {
            serde_json::to_writer(&mut *out, key)?;
            out.extend_from_slice(b": ");
        }
        write_node(out, node, depth + 1)?;
    }
    if !empty {
        out.push(b'\n');
        indent(out, depth);
    }
    out.push(brackets[1]);
    Ok(())
}

fn indent(out: &mut Vec<u8>, depth: usize) {
    for _ in 0..depth {
        out.extend_from_slice(INDENT);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::OrderedJson;

    #[test]
    fn pretty_output_matches_serde_json_format_and_keeps_construction_order() {
        let tree = ordered_json!({
            "z": 1,
            "a": [],
            "m": { "y": "\"q\"", "b": [true, (json!(null))] },
            "empty": {},
            "plain": (json!({ "k": [1.5, { "n": -2 }] })),
        });
        assert_eq!(
            tree.to_pretty(),
            concat!(
                "{\n",
                "  \"z\": 1,\n",
                "  \"a\": [],\n",
                "  \"m\": {\n",
                "    \"y\": \"\\\"q\\\"\",\n",
                "    \"b\": [\n",
                "      true,\n",
                "      null\n",
                "    ]\n",
                "  },\n",
                "  \"empty\": {},\n",
                "  \"plain\": {\n",
                "    \"k\": [\n",
                "      1.5,\n",
                "      {\n",
                "        \"n\": -2\n",
                "      }\n",
                "    ]\n",
                "  }\n",
                "}\n",
            )
        );
        assert_eq!(
            tree.keys().collect::<Vec<_>>(),
            ["z", "a", "m", "empty", "plain"]
        );
        assert_eq!(
            tree.to_value(),
            json!({ "z": 1, "a": [], "m": { "y": "\"q\"", "b": [true, null] }, "empty": {}, "plain": { "k": [1.5, { "n": -2 }] } })
        );
        assert_eq!(OrderedJson::from(json!([])).to_pretty(), "[]\n");
    }
}
