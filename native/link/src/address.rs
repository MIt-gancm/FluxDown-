//! 对端地址模型：`host` / `host:port` / `http(s)://host[:port][/base]`。
//!
//! 此前配对与探测把地址硬拼成 `http://{host}:{port}`，于是 https 站点、443 端口、
//! 带域名的反代（含子路径部署）全部无法配对。这里把用户输入解析成结构化的
//! [`PeerAddress`]，所有出站请求统一由它拼 URL：
//!
//! - 无协议 → `http`；无端口 → `http` 用 [`DEFAULT_HTTP_PORT`]（17800）、`https` 用 443。
//! - `/base` 子路径原样保留（去掉尾部 `/`、查询串与片段），供反代把 FluxDown 挂在
//!   子路径下（`https://example.com/fluxdown`）。
//! - IPv6 字面量必须用方括号写端口（`[fe80::1]:17800`），裸 IPv6（`fe80::1`）按无端口处理。
//! - 已配对设备的旧候选（`ip:port`，Flutter 时代写入）用同一解析器读回，格式向后兼容。

use std::fmt;
use std::net::Ipv6Addr;

use super::error::{LinkError, LinkResult};

/// FluxDown 本地 API 的默认明文端口。
pub const DEFAULT_HTTP_PORT: u16 = 17800;
/// `https` 未写端口时的默认端口。
pub const DEFAULT_HTTPS_PORT: u16 = 443;

/// 地址协议。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }

    fn default_port(self) -> u16 {
        match self {
            Scheme::Http => 80,
            Scheme::Https => DEFAULT_HTTPS_PORT,
        }
    }
}

/// 一个结构化的对端地址。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerAddress {
    scheme: Scheme,
    /// 主机名 / IP，IPv6 不含方括号。
    host: String,
    port: u16,
    /// 反代子路径：空串或以 `/` 开头、不以 `/` 结尾。
    base_path: String,
}

impl PeerAddress {
    /// 由「裸主机 + 端口」构造 http 地址（Flutter 旧接口 / mDNS 发现结果）。
    pub fn from_host_port(host: &str, port: u16) -> LinkResult<Self> {
        validate_host(host)?;
        if port == 0 {
            return Err(LinkError::BadPayload("port must be 1-65535".into()));
        }
        Ok(Self {
            scheme: Scheme::Http,
            host: host.to_string(),
            port,
            base_path: String::new(),
        })
    }

    /// 解析用户输入 / 已存储候选。
    pub fn parse(input: &str) -> LinkResult<Self> {
        let input = input.trim();
        if input.is_empty() {
            return Err(LinkError::BadPayload("address is empty".into()));
        }
        let (scheme, rest) = match input.split_once("://") {
            Some((scheme, rest)) => {
                let scheme = if scheme.eq_ignore_ascii_case("http") {
                    Scheme::Http
                } else if scheme.eq_ignore_ascii_case("https") {
                    Scheme::Https
                } else {
                    return Err(LinkError::BadPayload(format!(
                        "unsupported address scheme: {scheme}"
                    )));
                };
                (scheme, rest)
            }
            None => (Scheme::Http, input),
        };
        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, tail) = rest.split_at(authority_end);
        if authority.contains('@') {
            return Err(LinkError::BadPayload(
                "credentials in address are not supported".into(),
            ));
        }
        let path = tail.split(['?', '#']).next().unwrap_or_default();
        let base_path = path.trim_end_matches('/').to_string();
        if base_path
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(LinkError::BadPayload("invalid address path".into()));
        }

        let (host, explicit_port) = split_authority(authority)?;
        validate_host(&host)?;
        let port = explicit_port.unwrap_or(match scheme {
            Scheme::Http => DEFAULT_HTTP_PORT,
            Scheme::Https => DEFAULT_HTTPS_PORT,
        });
        Ok(Self {
            scheme,
            host,
            port,
            base_path,
        })
    }

    #[must_use]
    pub fn scheme(&self) -> &'static str {
        self.scheme.as_str()
    }

    #[must_use]
    pub fn is_https(&self) -> bool {
        self.scheme == Scheme::Https
    }

    /// 主机名 / IP（IPv6 不含方括号）。
    #[must_use]
    pub fn host(&self) -> &str {
        &self.host
    }

    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[must_use]
    pub fn base_path(&self) -> &str {
        &self.base_path
    }

    /// `host:port`（IPv6 加方括号），用于日志与 `initiatorAddrs` 之外的展示。
    #[must_use]
    pub fn authority(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        if self.port == self.scheme.default_port() {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    /// API 基址（不含尾斜杠），如 `https://example.com/fluxdown`。
    #[must_use]
    pub fn base_url(&self) -> String {
        format!(
            "{}://{}{}",
            self.scheme.as_str(),
            self.authority(),
            self.base_path
        )
    }

    /// 拼接 API 路径（`path` 以 `/` 开头）。
    #[must_use]
    pub fn url(&self, path: &str) -> String {
        format!("{}{path}", self.base_url())
    }

    /// 存入候选表的字符串：普通 http 直连保持旧的 `host:port` 形态（与 Flutter 时代
    /// 写入的记录互相可读），其余（https / 子路径）存完整 URL。
    #[must_use]
    pub fn to_candidate(&self) -> String {
        if self.scheme == Scheme::Http && self.base_path.is_empty() {
            let host = if self.host.contains(':') {
                format!("[{}]", self.host)
            } else {
                self.host.clone()
            };
            format!("{host}:{}", self.port)
        } else {
            self.base_url()
        }
    }
}

impl fmt::Display for PeerAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.base_url())
    }
}

/// 拆出 `(host, 显式端口)`；IPv6 字面量剥掉方括号。
fn split_authority(authority: &str) -> LinkResult<(String, Option<u16>)> {
    if let Some(rest) = authority.strip_prefix('[') {
        let (host, after) = rest
            .split_once(']')
            .ok_or_else(|| LinkError::BadPayload("unterminated IPv6 literal".into()))?;
        let port = match after {
            "" => None,
            after => Some(parse_port(after.strip_prefix(':').ok_or_else(|| {
                LinkError::BadPayload("unexpected text after IPv6 literal".into())
            })?)?),
        };
        return Ok((host.to_string(), port));
    }
    // 裸 IPv6（多个冒号）整体当主机名，无端口。
    if authority.matches(':').count() > 1 {
        return Ok((authority.to_string(), None));
    }
    match authority.split_once(':') {
        Some((host, port)) => Ok((host.to_string(), Some(parse_port(port)?))),
        None => Ok((authority.to_string(), None)),
    }
}

fn parse_port(text: &str) -> LinkResult<u16> {
    text.parse::<u16>()
        .ok()
        .filter(|port| *port != 0)
        .ok_or_else(|| LinkError::BadPayload(format!("invalid port: {text}")))
}

fn validate_host(host: &str) -> LinkResult<()> {
    if host.is_empty() {
        return Err(LinkError::BadPayload("address host is empty".into()));
    }
    if host.contains(':') {
        return host
            .parse::<Ipv6Addr>()
            .map(|_| ())
            .map_err(|_| LinkError::BadPayload(format!("invalid IPv6 host: {host}")));
    }
    let valid = host
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'));
    if valid {
        Ok(())
    } else {
        Err(LinkError::BadPayload(format!("invalid host: {host}")))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn parse(input: &str) -> PeerAddress {
        PeerAddress::parse(input).unwrap()
    }

    #[test]
    fn bare_host_uses_http_and_default_port() {
        let a = parse("192.168.1.20");
        assert_eq!(a.base_url(), "http://192.168.1.20:17800");
        assert!(!a.is_https());
    }

    #[test]
    fn host_port_keeps_explicit_port() {
        let a = parse("nas.local:9000");
        assert_eq!(a.base_url(), "http://nas.local:9000");
        assert_eq!(a.to_candidate(), "nas.local:9000");
    }

    #[test]
    fn https_without_port_defaults_to_443_and_omits_it() {
        let a = parse("https://dl.example.com");
        assert_eq!(a.port(), 443);
        assert_eq!(a.base_url(), "https://dl.example.com");
        assert_eq!(a.url("/ping"), "https://dl.example.com/ping");
    }

    #[test]
    fn https_with_port_and_base_path() {
        let a = parse("HTTPS://example.com:8443/fluxdown/?x=1#frag");
        assert_eq!(a.base_url(), "https://example.com:8443/fluxdown");
        assert_eq!(
            a.url("/api/v1/link/pair/hello"),
            "https://example.com:8443/fluxdown/api/v1/link/pair/hello"
        );
        // https / 子路径存完整 URL，重新解析后等价。
        assert_eq!(parse(&a.to_candidate()), a);
    }

    #[test]
    fn explicit_default_http_port_80_is_not_mistaken_for_17800() {
        let a = parse("http://example.com:80");
        assert_eq!(a.port(), 80);
        assert_eq!(a.base_url(), "http://example.com");
    }

    #[test]
    fn ipv6_literals() {
        assert_eq!(
            parse("[fe80::1]:17800").base_url(),
            "http://[fe80::1]:17800"
        );
        assert_eq!(parse("[::1]").base_url(), "http://[::1]:17800");
        assert_eq!(parse("fe80::1").base_url(), "http://[fe80::1]:17800");
        assert_eq!(
            parse("https://[::1]:8443/x").base_url(),
            "https://[::1]:8443/x"
        );
    }

    #[test]
    fn legacy_candidate_roundtrips() {
        let a = PeerAddress::from_host_port("10.0.0.5", 17800).unwrap();
        assert_eq!(a.to_candidate(), "10.0.0.5:17800");
        assert_eq!(parse(&a.to_candidate()), a);
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "   ",
            "ftp://host",
            "http://",
            "host:0",
            "host:70000",
            "host:abc",
            "user@host",
            "http://[::1",
            "bad host",
            "http://host/with space",
        ] {
            assert!(
                PeerAddress::parse(bad).is_err(),
                "{bad:?} should be rejected"
            );
        }
    }
}
