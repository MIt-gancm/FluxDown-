//! agent 私有状态文件里的设备互联存储：本机身份种子 + 已配对设备名册。
//!
//! 名册沿用 daemon 迁移时写入 `AgentState::linked_devices` 的 JSON 形状（`fingerprint` /
//! `identityPubB64` / `name` / `platform` / `linkSecretB64` / `candidatesJson` / `pairedAt` /
//! `lastSeenAt`），并向后兼容地追加两个可选字段 `defaultSaveDir` / `pathStyle`（对端经已认证
//! 链路自报，旧记录没有）。链路密钥只存在这里与内存里，永远不进快照 / 事件。

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as B64;
use fluxdown_link::{LinkError, LinkResult, LinkStorage, PeerCandidate, PeerInfo, PeerRecord};
use fluxdown_protocol::{LinkDeviceInfo, PathStyle};
use serde_json::{Value, json};
use tokio::sync::Mutex;

use crate::state::{AgentState, StateStore};

/// 基于 agent 状态文件的 [`LinkStorage`]。
pub struct AgentLinkStorage {
    state: Arc<Mutex<AgentState>>,
    store: Arc<StateStore>,
}

impl AgentLinkStorage {
    #[must_use]
    pub fn new(state: Arc<Mutex<AgentState>>, store: Arc<StateStore>) -> Self {
        Self { state, store }
    }

    async fn persist(&self, state: &AgentState) -> LinkResult<()> {
        self.store
            .save(state)
            .await
            .map_err(|error| LinkError::Store(format!("{error:#}")))
    }
}

/// 身份种子可能是 daemon 迁移来的 base64 字符串，也可能是早期 agent 自行生成的
/// `{ "secretB64": … }` 对象；两种都认。
fn seed_from_identity(value: &Value) -> Option<[u8; 32]> {
    let text = match value {
        Value::String(text) => text.as_str(),
        Value::Object(map) => map.get("secretB64")?.as_str()?,
        _ => return None,
    };
    let bytes = B64.decode(text.trim()).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

fn path_style_text(style: PathStyle) -> Option<&'static str> {
    match style {
        PathStyle::Windows => Some("windows"),
        PathStyle::Posix => Some("posix"),
        PathStyle::Unknown => None,
    }
}

fn path_style_from_text(text: &str) -> Option<PathStyle> {
    match text {
        "windows" => Some(PathStyle::Windows),
        "posix" => Some(PathStyle::Posix),
        _ => None,
    }
}

/// 把名册里的一条 JSON 解析为强类型记录；缺关键字段（指纹 / 公钥 / 密钥）的坏条目返回
/// `None`（不让一条坏数据毒死整份名册）。
pub(super) fn record_from_value(value: &Value) -> Option<PeerRecord> {
    let text = |key: &str| value.get(key).and_then(Value::as_str);
    let bytes = |key: &str| text(key).and_then(|s| B64.decode(s).ok());
    let fingerprint = text("fingerprint")?.to_owned();
    let identity_pub = bytes("identityPubB64")?;
    let link_secret = bytes("linkSecretB64").filter(|secret| !secret.is_empty())?;
    let candidates: Vec<PeerCandidate> = text("candidatesJson")
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default();
    Some(PeerRecord {
        fingerprint,
        identity_pub,
        name: text("name").unwrap_or_default().to_owned(),
        platform: text("platform")
            .filter(|platform| !platform.is_empty())
            .map(str::to_owned),
        link_secret,
        candidates,
        paired_at: value.get("pairedAt").and_then(Value::as_i64).unwrap_or(0),
        last_seen_at: value.get("lastSeenAt").and_then(Value::as_i64).unwrap_or(0),
        info: PeerInfo {
            default_save_dir: text("defaultSaveDir")
                .filter(|dir| !dir.is_empty())
                .map(str::to_owned),
            path_style: text("pathStyle")
                .filter(|style| path_style_from_text(style).is_some())
                .map(str::to_owned),
        },
    })
}

pub(super) fn value_from_record(record: &PeerRecord) -> Value {
    let mut value = json!({
        "fingerprint": record.fingerprint,
        "identityPubB64": B64.encode(&record.identity_pub),
        "name": record.name,
        "platform": record.platform.clone().unwrap_or_default(),
        "linkSecretB64": B64.encode(&record.link_secret),
        "candidatesJson": serde_json::to_string(&record.candidates).unwrap_or_else(|_| "[]".to_owned()),
        "pairedAt": record.paired_at,
        "lastSeenAt": record.last_seen_at,
    });
    if let Some(dir) = &record.info.default_save_dir {
        value["defaultSaveDir"] = Value::String(dir.clone());
    }
    if let Some(style) = &record.info.path_style {
        value["pathStyle"] = Value::String(style.clone());
    }
    value
}

fn records(state: &AgentState) -> Vec<PeerRecord> {
    let mut records: Vec<PeerRecord> = state
        .linked_devices
        .iter()
        .filter_map(record_from_value)
        .collect();
    records.sort_by_key(|record| std::cmp::Reverse(record.last_seen_at));
    records
}

fn position(state: &AgentState, fingerprint: &str) -> Option<usize> {
    state
        .linked_devices
        .iter()
        .position(|value| value.get("fingerprint").and_then(Value::as_str) == Some(fingerprint))
}

/// 名册记录 → 对外视图（严禁透出链路密钥 / 公钥）。
#[must_use]
pub fn device_info(record: &PeerRecord, online: bool) -> LinkDeviceInfo {
    LinkDeviceInfo {
        fingerprint: record.fingerprint.clone(),
        name: record.name.clone(),
        platform: record.platform.clone(),
        online,
        paired_at: record.paired_at,
        last_seen_at: record.last_seen_at,
        default_save_dir: record.info.default_save_dir.clone(),
        path_style: record
            .info
            .path_style
            .as_deref()
            .and_then(path_style_from_text),
    }
}

/// 运行期名册视图（启动初始快照用，在线状态未知一律 `false`；之后由 `LinkService`
/// 探测后推送带真实 `online` 的 `LinkedDevicesChanged`）。
#[must_use]
pub fn public_devices(state: &AgentState) -> Vec<LinkDeviceInfo> {
    records(state)
        .iter()
        .map(|record| device_info(record, false))
        .collect()
}

/// 名册视图（带在线状态表）。
#[must_use]
pub(super) fn devices_with_online(
    state: &AgentState,
    online: &HashMap<String, bool>,
) -> Vec<LinkDeviceInfo> {
    records(state)
        .iter()
        .map(|record| {
            device_info(
                record,
                online.get(&record.fingerprint).copied().unwrap_or(false),
            )
        })
        .collect()
}

/// 本机路径风格的名册文本形式（对端信息交换用）。
#[must_use]
pub(super) fn local_path_style_text() -> &'static str {
    path_style_text(PathStyle::current()).unwrap_or("posix")
}

#[async_trait]
impl LinkStorage for AgentLinkStorage {
    async fn load_identity_seed(&self) -> LinkResult<Option<[u8; 32]>> {
        let state = self.state.lock().await;
        Ok(state.link_identity.as_ref().and_then(seed_from_identity))
    }

    async fn save_identity_seed(&self, seed: &[u8; 32]) -> LinkResult<()> {
        let mut state = self.state.lock().await;
        state.link_identity = Some(Value::String(B64.encode(seed)));
        self.persist(&state).await
    }

    async fn upsert(&self, record: &PeerRecord) -> LinkResult<()> {
        let mut state = self.state.lock().await;
        let mut fresh = record.clone();
        let existing = position(&state, &record.fingerprint);
        // 重新配对不带对端信息：沿用旧记录里已交换到的信息，直到下一次交换刷新。
        if fresh.info == PeerInfo::default()
            && let Some(old) = existing
                .and_then(|index| state.linked_devices.get(index))
                .and_then(record_from_value)
        {
            fresh.info = old.info;
        }
        let value = value_from_record(&fresh);
        match existing {
            Some(index) => state.linked_devices[index] = value,
            None => state.linked_devices.push(value),
        }
        self.persist(&state).await
    }

    async fn list(&self) -> LinkResult<Vec<PeerRecord>> {
        Ok(records(&*self.state.lock().await))
    }

    async fn get(&self, fingerprint: &str) -> LinkResult<Option<PeerRecord>> {
        let state = self.state.lock().await;
        Ok(position(&state, fingerprint)
            .and_then(|index| state.linked_devices.get(index))
            .and_then(record_from_value))
    }

    async fn remove(&self, fingerprint: &str) -> LinkResult<bool> {
        let mut state = self.state.lock().await;
        let Some(index) = position(&state, fingerprint) else {
            return Ok(false);
        };
        state.linked_devices.remove(index);
        self.persist(&state).await?;
        Ok(true)
    }

    async fn touch(&self, fingerprint: &str, at: i64) -> LinkResult<()> {
        // 最近活跃时间只更新内存：它随下一次名册写盘落地，不值得为每次探活 fsync 整份状态。
        let mut state = self.state.lock().await;
        if let Some(index) = position(&state, fingerprint) {
            state.linked_devices[index]["lastSeenAt"] = json!(at);
        }
        Ok(())
    }

    async fn update_candidates(
        &self,
        fingerprint: &str,
        candidates: &[PeerCandidate],
    ) -> LinkResult<bool> {
        let mut state = self.state.lock().await;
        let Some(index) = position(&state, fingerprint) else {
            return Ok(false);
        };
        state.linked_devices[index]["candidatesJson"] =
            Value::String(serde_json::to_string(candidates).unwrap_or_else(|_| "[]".to_owned()));
        self.persist(&state).await?;
        Ok(true)
    }

    async fn set_peer_info(&self, fingerprint: &str, info: &PeerInfo) -> LinkResult<bool> {
        let mut state = self.state.lock().await;
        let Some(index) = position(&state, fingerprint) else {
            return Ok(false);
        };
        let current = state
            .linked_devices
            .get(index)
            .and_then(record_from_value)
            .map(|record| record.info);
        if current.as_ref() == Some(info) {
            return Ok(true);
        }
        let entry = &mut state.linked_devices[index];
        match &info.default_save_dir {
            Some(dir) => entry["defaultSaveDir"] = Value::String(dir.clone()),
            None => {
                if let Some(map) = entry.as_object_mut() {
                    map.remove("defaultSaveDir");
                }
            }
        }
        match &info.path_style {
            Some(style) => entry["pathStyle"] = Value::String(style.clone()),
            None => {
                if let Some(map) = entry.as_object_mut() {
                    map.remove("pathStyle");
                }
            }
        }
        self.persist(&state).await?;
        Ok(true)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use fluxdown_link::TransportKind;

    use super::*;

    fn record() -> PeerRecord {
        PeerRecord {
            fingerprint: "fp-1".into(),
            identity_pub: vec![1; 32],
            name: "Laptop".into(),
            platform: Some("macos".into()),
            link_secret: vec![2; 32],
            candidates: vec![PeerCandidate {
                kind: TransportKind::Direct,
                address: "192.168.1.5:17800".into(),
            }],
            paired_at: 100,
            last_seen_at: 200,
            info: PeerInfo::default(),
        }
    }

    #[test]
    fn migrated_daemon_roster_shape_is_readable() {
        // daemon `link_migration_export` 写入的形状（没有 defaultSaveDir / pathStyle）。
        let value = json!({
            "fingerprint": "fp-1",
            "identityPubB64": B64.encode([1u8; 32]),
            "name": "Phone",
            "platform": "",
            "linkSecretB64": B64.encode([2u8; 32]),
            "candidatesJson": r#"[{"kind":"Direct","address":"10.0.0.2:17800"}]"#,
            "pairedAt": 5,
            "lastSeenAt": 6,
        });
        let record = record_from_value(&value).unwrap();
        assert_eq!(record.platform, None);
        assert_eq!(record.candidates.len(), 1);
        assert_eq!(record.info, PeerInfo::default());
        assert!(device_info(&record, false).default_save_dir.is_none());
    }

    #[test]
    fn broken_roster_entries_are_skipped_not_fatal() {
        assert!(record_from_value(&json!({"fingerprint": "x"})).is_none());
        let empty_secret = json!({
            "fingerprint": "x",
            "identityPubB64": B64.encode([1u8; 32]),
            "linkSecretB64": "",
        });
        assert!(record_from_value(&empty_secret).is_none());
    }

    #[test]
    fn peer_info_survives_a_roster_roundtrip_and_maps_to_dto() {
        let mut original = record();
        original.info = PeerInfo {
            default_save_dir: Some("/mnt/downloads".into()),
            path_style: Some("posix".into()),
        };
        let back = record_from_value(&value_from_record(&original)).unwrap();
        assert_eq!(back.info, original.info);
        let dto = device_info(&back, true);
        assert!(dto.online);
        assert_eq!(dto.default_save_dir.as_deref(), Some("/mnt/downloads"));
        assert_eq!(dto.path_style, Some(PathStyle::Posix));
    }

    #[test]
    fn identity_seed_accepts_daemon_string_and_legacy_object() {
        let seed = [9u8; 32];
        let text = B64.encode(seed);
        assert_eq!(seed_from_identity(&Value::String(text.clone())), Some(seed));
        assert_eq!(
            seed_from_identity(&json!({"secretB64": text, "publicB64": "ignored"})),
            Some(seed)
        );
        assert_eq!(seed_from_identity(&Value::Null), None);
        assert_eq!(
            seed_from_identity(&Value::String("not base64!".into())),
            None
        );
    }
}
