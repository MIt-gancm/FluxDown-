//! `thunder://` 链接解析 —— 纯字符串处理，零 I/O、零网络。
//!
//! 迅雷专有的 base64 封装协议：真实下载地址被 `AA` 前缀、`ZZ` 后缀包裹后
//! 整体做标准 base64 编码。只处理这一种最常见形态；flashget（`flashget://`）
//! 与 qqdl（`qqdl://`）等其它厂商私有协议不在范围内。

use base64::Engine as _;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

/// 标准字母表、解码时对 `=` 填充宽容：野外的 thunder 链接常由各种工具/网页
/// 手工拼出，去掉尾部 `=` 的形态并不少见。
const B64: GeneralPurpose = GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

/// `thunder://` 前缀长度（`"thunder://".len()`），解码时按字节切片跳过。
const THUNDER_PREFIX_LEN: usize = 10;

/// URL 是否为 `thunder://` 链接（前缀大小写不敏感）。
///
/// 镜像 `ed2k::link::is_ed2k_url` 的写法，仅判前缀，不做完整解析。
///
/// # Examples
///
/// ```
/// use fluxdown_engine::thunder::is_thunder_url;
/// assert!(is_thunder_url("thunder://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg=="));
/// assert!(is_thunder_url("THUNDER://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg=="));
/// assert!(!is_thunder_url("http://example.com/file.zip"));
/// ```
#[must_use]
pub fn is_thunder_url(url: &str) -> bool {
    url.get(..THUNDER_PREFIX_LEN)
        .map(|prefix| prefix.eq_ignore_ascii_case("thunder://"))
        .unwrap_or(false)
}

/// 解析 `thunder://<base64>` 链接，返回其封装的真实下载地址。
///
/// 载荷做标准 base64 解码后应形如 `AA<真实地址>ZZ`；解出的内容必须以
/// `http://`/`https://`/`ftp://`/`ed2k://`/`magnet:` 之一开头（thunder 链接
/// 内嵌 ed2k/磁力地址是常见形态），否则视为不支持或损坏的链接。
///
/// # Errors
///
/// 前缀不符 / base64 解码失败 / 解码结果非 UTF-8 / 缺少 `AA`…`ZZ` 包裹 /
/// 包裹内地址协议不受支持时返回描述性错误信息。
///
/// # Examples
///
/// ```
/// use fluxdown_engine::thunder::decode_thunder_url;
/// let real = decode_thunder_url("thunder://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg==").unwrap();
/// assert_eq!(real, "http://example.com/file.zip");
/// ```
pub fn decode_thunder_url(url: &str) -> Result<String, String> {
    if !is_thunder_url(url) {
        return Err("not a thunder:// link".to_string());
    }
    let payload = url[THUNDER_PREFIX_LEN..].trim();
    let decoded_bytes = B64
        .decode(payload)
        .map_err(|e| format!("thunder link base64 decode failed: {e}"))?;
    let decoded = String::from_utf8(decoded_bytes)
        .map_err(|e| format!("thunder link payload is not valid utf-8: {e}"))?;
    let inner = decoded
        .strip_prefix("AA")
        .and_then(|s| s.strip_suffix("ZZ"))
        .ok_or_else(|| "thunder link missing AA/ZZ wrapper".to_string())?
        .trim();
    let supported = inner.starts_with("http://")
        || inner.starts_with("https://")
        || inner.starts_with("ftp://")
        || inner.starts_with("ed2k://")
        || inner.starts_with("magnet:");
    if !supported {
        return Err(format!("unsupported protocol inside thunder link: {inner}"));
    }
    Ok(inner.to_string())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn decodes_known_sample() {
        let real =
            decode_thunder_url("thunder://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg==").unwrap();
        assert_eq!(real, "http://example.com/file.zip");
    }

    #[test]
    fn decodes_unpadded_payload() {
        // 野外常见：尾部 `=` 被去掉的 thunder 链接也要能解。
        let real =
            decode_thunder_url("thunder://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg").unwrap();
        assert_eq!(real, "http://example.com/file.zip");
    }

    #[test]
    fn prefix_check_is_case_insensitive() {
        assert!(is_thunder_url(
            "THUNDER://QUFodHRwOi8vZXhhbXBsZS5jb20vZmlsZS56aXBaWg=="
        ));
        assert!(!is_thunder_url("http://example.com/file.zip"));
    }

    #[test]
    fn invalid_base64_errors() {
        let err = decode_thunder_url("thunder://not-valid-base64!!!").unwrap_err();
        assert!(err.contains("base64"));
    }

    #[test]
    fn decoded_non_url_errors() {
        let bad = format!("thunder://{}", B64.encode(b"AAnothing-usefulZZ"));
        let err = decode_thunder_url(&bad).unwrap_err();
        assert!(err.contains("unsupported protocol"));
    }

    #[test]
    fn missing_wrapper_errors() {
        // 缺少 AA/ZZ 包裹：解码出的是裸 URL，不满足 strip_prefix("AA") 语义。
        let bad = format!("thunder://{}", B64.encode(b"http://example.com/file.zip"));
        let err = decode_thunder_url(&bad).unwrap_err();
        assert!(err.contains("AA/ZZ"));
    }
}
