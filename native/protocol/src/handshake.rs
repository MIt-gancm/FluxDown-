//! daemon ↔ agent 专属的双向挑战应答握手。
//!
//! `daemon.token` 是 agent 与 fluxdownd 共享的长期密钥：任何占到 daemon 端口的本机进程
//! 都能假装成 daemon，因此 token（以及任何可复用的派生凭据）在对端**证明**持有 token 之前
//! 不得离开 agent。握手在 WebSocket 内、`system.hello` 之前完成：
//!
//! 1. agent → daemon `system.auth.challenge { clientNonce }`；
//! 2. daemon → agent `{ serverNonce, serverProof }`，`serverProof` 绑定两个随机数；
//! 3. agent 校验 `serverProof` 通过后才发送 `system.auth.prove { clientProof }`；
//! 4. daemon 校验 `clientProof` 后连接进入已认证状态，两端各自用同一函数派生出本会话的
//!    HTTP 凭据（[`http_credential`]），用于 `/blobs`、`/files`、`/exports` 等专用 HTTP 端点。
//!
//! 原始 token 从不上线；三种 MAC 用不同的角色标签做域分离，服务端证明无法被反射成客户端
//! 证明，HTTP 凭据也无法由任一证明推出。随机数由调用方从系统 CSPRNG 生成（本 crate 不
//! 依赖随机数库），每次连接各用一组新值，所以旧会话的证明与凭据重放无效。

use serde::{Deserialize, Serialize};

use crate::digest::{constant_time_eq, hmac_sha256, to_hex};

/// 握手第一步：客户端发出随机数挑战。
pub const SYSTEM_AUTH_CHALLENGE: &str = "system.auth.challenge";
/// 握手第二步：客户端在校验完服务端证明后提交自己的证明。
pub const SYSTEM_AUTH_PROVE: &str = "system.auth.prove";

const NONCE_MIN_LEN: usize = 32;
const NONCE_MAX_LEN: usize = 128;
const DOMAIN: &str = "fluxdown-daemon-auth/v1";

const ROLE_SERVER_PROOF: &str = "daemon-proof";
const ROLE_CLIENT_PROOF: &str = "agent-proof";
const ROLE_HTTP_CREDENTIAL: &str = "http-credential";

/// `system.auth.challenge` 参数。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthChallengeParams {
    pub client_nonce: String,
}

/// `system.auth.challenge` 结果：服务端随机数与它对 token 持有权的证明。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthChallengeResult {
    pub server_nonce: String,
    pub server_proof: String,
}

/// `system.auth.prove` 参数。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthProveParams {
    pub client_proof: String,
}

/// `system.auth.prove` 结果；校验失败走 `Unauthorized` 错误而不是 `authenticated=false`。
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthProveResult {
    pub authenticated: bool,
}

/// 随机数必须是 32..=128 位十六进制：定长字符集保证 MAC 输入无歧义，长度下限保证熵。
#[must_use]
pub fn is_valid_nonce(nonce: &str) -> bool {
    (NONCE_MIN_LEN..=NONCE_MAX_LEN).contains(&nonce.len())
        && nonce.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// daemon 对 token 持有权的证明；随机数非法或 token 为空返回 `None`。
#[must_use]
pub fn server_proof(token: &str, client_nonce: &str, server_nonce: &str) -> Option<String> {
    mac(ROLE_SERVER_PROOF, token, client_nonce, server_nonce)
}

/// agent 对 token 持有权的证明；随机数非法或 token 为空返回 `None`。
#[must_use]
pub fn client_proof(token: &str, client_nonce: &str, server_nonce: &str) -> Option<String> {
    mac(ROLE_CLIENT_PROOF, token, client_nonce, server_nonce)
}

/// 本会话的 HTTP 凭据：两端各自派生、从不经 WebSocket 传输；只有持有 token 且参与了本次
/// 握手的一方能算出，daemon 在会话结束时撤销。
#[must_use]
pub fn http_credential(token: &str, client_nonce: &str, server_nonce: &str) -> Option<String> {
    mac(ROLE_HTTP_CREDENTIAL, token, client_nonce, server_nonce)
}

/// 常量时间校验 [`server_proof`]。
#[must_use]
pub fn verify_server_proof(
    token: &str,
    client_nonce: &str,
    server_nonce: &str,
    presented: &str,
) -> bool {
    server_proof(token, client_nonce, server_nonce)
        .is_some_and(|expected| constant_time_eq(expected.as_bytes(), presented.as_bytes()))
}

/// 常量时间校验 [`client_proof`]。
#[must_use]
pub fn verify_client_proof(
    token: &str,
    client_nonce: &str,
    server_nonce: &str,
    presented: &str,
) -> bool {
    client_proof(token, client_nonce, server_nonce)
        .is_some_and(|expected| constant_time_eq(expected.as_bytes(), presented.as_bytes()))
}

/// 常量时间比较两个凭据字符串（供 HTTP 端点校验会话凭据）。
#[must_use]
pub fn credentials_match(expected: &str, presented: &str) -> bool {
    constant_time_eq(expected.as_bytes(), presented.as_bytes())
}

fn mac(role: &str, token: &str, client_nonce: &str, server_nonce: &str) -> Option<String> {
    if token.is_empty() || !is_valid_nonce(client_nonce) || !is_valid_nonce(server_nonce) {
        return None;
    }
    let digest = hmac_sha256(
        token.as_bytes(),
        &[
            DOMAIN.as_bytes(),
            b"\n",
            role.as_bytes(),
            b"\n",
            client_nonce.as_bytes(),
            b"\n",
            server_nonce.as_bytes(),
        ],
    );
    Some(to_hex(&digest))
}

#[cfg(test)]
mod tests {
    use super::{
        client_proof, credentials_match, http_credential, is_valid_nonce, server_proof,
        verify_client_proof, verify_server_proof,
    };

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";
    const CLIENT_NONCE: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const SERVER_NONCE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    #[test]
    fn proofs_verify_only_for_the_same_token_and_nonce_pair() {
        let proof = server_proof(TOKEN, CLIENT_NONCE, SERVER_NONCE).expect("proof");
        assert!(verify_server_proof(
            TOKEN,
            CLIENT_NONCE,
            SERVER_NONCE,
            &proof
        ));
        assert!(!verify_server_proof(
            "another-token",
            CLIENT_NONCE,
            SERVER_NONCE,
            &proof
        ));
        // 重放：换一次连接（任一随机数不同）旧证明即失效。
        let next_server_nonce = "cccccccccccccccccccccccccccccccc";
        assert!(!verify_server_proof(
            TOKEN,
            CLIENT_NONCE,
            next_server_nonce,
            &proof
        ));
        let next_client_nonce = "dddddddddddddddddddddddddddddddd";
        assert!(!verify_server_proof(
            TOKEN,
            next_client_nonce,
            SERVER_NONCE,
            &proof
        ));
        assert!(!verify_server_proof(TOKEN, CLIENT_NONCE, SERVER_NONCE, ""));
        assert!(!verify_server_proof(
            TOKEN,
            CLIENT_NONCE,
            SERVER_NONCE,
            &proof[..proof.len() - 1]
        ));
    }

    #[test]
    fn roles_are_domain_separated() {
        let server = server_proof(TOKEN, CLIENT_NONCE, SERVER_NONCE).expect("server proof");
        let client = client_proof(TOKEN, CLIENT_NONCE, SERVER_NONCE).expect("client proof");
        let http = http_credential(TOKEN, CLIENT_NONCE, SERVER_NONCE).expect("credential");
        assert_ne!(server, client);
        assert_ne!(server, http);
        assert_ne!(client, http);
        // 服务端证明不能被反射成客户端证明，反之亦然。
        assert!(!verify_client_proof(
            TOKEN,
            CLIENT_NONCE,
            SERVER_NONCE,
            &server
        ));
        assert!(!verify_server_proof(
            TOKEN,
            CLIENT_NONCE,
            SERVER_NONCE,
            &client
        ));
        assert!(verify_client_proof(
            TOKEN,
            CLIENT_NONCE,
            SERVER_NONCE,
            &client
        ));
    }

    #[test]
    fn empty_token_and_malformed_nonces_never_yield_a_proof() {
        assert!(server_proof("", CLIENT_NONCE, SERVER_NONCE).is_none());
        assert!(client_proof("", CLIENT_NONCE, SERVER_NONCE).is_none());
        assert!(http_credential("", CLIENT_NONCE, SERVER_NONCE).is_none());
        assert!(!verify_server_proof("", CLIENT_NONCE, SERVER_NONCE, ""));
        for bad in ["", "abc", "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz", "a\nb"] {
            assert!(!is_valid_nonce(bad), "{bad:?}");
            assert!(server_proof(TOKEN, bad, SERVER_NONCE).is_none());
            assert!(client_proof(TOKEN, CLIENT_NONCE, bad).is_none());
        }
        assert!(!is_valid_nonce(&"a".repeat(129)));
        assert!(is_valid_nonce(&"A".repeat(64)));
    }

    #[test]
    fn credentials_match_requires_exact_value() {
        assert!(credentials_match("abc", "abc"));
        assert!(!credentials_match("abc", "abd"));
        assert!(!credentials_match("abc", ""));
    }
}
