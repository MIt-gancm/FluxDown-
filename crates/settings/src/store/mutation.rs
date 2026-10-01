//! 整值 daemon 配置键（如 `webhook.endpoints` 这种 JSON 数组）的读-改-写链。
//!
//! `set_daemon` 写的是「最终值」：修订冲突时原样重发，会覆盖别处（Web SPA / 另一窗口）
//! 同时写入的修改。这里存的是「变更函数」：每次写入前 `daemon.config.get` 取最新值重新
//! 计算，冲突时重取重放（与 Web `patchEndpoints` 同语义）；写入串行，不会彼此制造冲突。
//! 乐观展示 = 最近一次服务端值 + 按序重放未完成的变更，服务端快照到达时同样重放，
//! 所以变更函数必须幂等（「按 id 覆盖 / 删除 / 置开关」天然满足）。

use std::collections::{BTreeMap, VecDeque};
use std::rc::Rc;

use fluxdown_protocol::{
    ApplicationErrorCode, DaemonConfigPatch, DaemonConfigSnapshot, RpcErrorData, method,
};
use gpui::{Context, SharedString};
use serde_json::{Value, json};

use super::{MAX_CONFLICT_RETRIES, SettingsError, SettingsErrorKind, SettingsStore};
use crate::port::SettingsPort;

/// 变更函数：输入当前原始值，返回新值；`None` = 无需写入。
type MutateFn = Rc<dyn Fn(&str) -> Option<String>>;

struct DaemonMutation {
    id: u64,
    key: &'static str,
    apply: MutateFn,
}

/// 未完成的变更队列与乐观展示的服务端基线。
#[derive(Default)]
pub(super) struct DaemonMutations {
    queue: VecDeque<DaemonMutation>,
    /// 有排队变更的键最近一次的服务端值（`None` = 服务端未设置）。
    base: BTreeMap<&'static str, Option<String>>,
    next_id: u64,
    inflight: bool,
}

impl DaemonMutations {
    /// 连接失效：未发送的变更作废（与 `pending_daemon` 同语义）。在途那条完成后自行收尾。
    pub(super) fn discard(&mut self) {
        self.queue.clear();
        self.base.clear();
    }
}

struct MutationOutcome {
    /// 最近一次读到 / 写入后的服务端配置。
    latest: Option<DaemonConfigSnapshot>,
    error: Option<RpcErrorData>,
}

impl SettingsStore {
    /// 以读-改-写方式修改一个 daemon 配置键：`apply` 基于服务端最新值计算新值，
    /// 冲突时重取重放。本地立即乐观展示，失败回到服务端值并给出错误。
    pub fn mutate_daemon(
        &mut self,
        key: &'static str,
        apply: impl Fn(&str) -> Option<String> + 'static,
        cx: &mut Context<Self>,
    ) {
        if self.stale {
            self.set_error(SettingsErrorKind::Disconnected, "", cx);
            return;
        }
        let server = self.daemon.values.get(key).cloned();
        self.mutations.base.entry(key).or_insert(server);
        let id = self.mutations.next_id;
        self.mutations.next_id += 1;
        self.mutations.queue.push_back(DaemonMutation {
            id,
            key,
            apply: Rc::new(apply),
        });
        self.overlay_mutations();
        cx.notify();
        self.pump_mutations(cx);
    }

    /// 服务端配置刚写入 `self.daemon`（尚未叠加本地编辑）：刷新排队键的基线。
    pub(super) fn capture_mutation_bases(&mut self) {
        for mutation in &self.mutations.queue {
            self.mutations
                .base
                .insert(mutation.key, self.daemon.values.get(mutation.key).cloned());
        }
    }

    /// 在服务端基线上按序重放未完成的变更。
    pub(super) fn overlay_mutations(&mut self) {
        for (key, base) in &self.mutations.base {
            let mut value = base
                .clone()
                .unwrap_or_else(|| fluxdown_protocol::daemon_config_default(key).to_owned());
            for mutation in self.mutations.queue.iter().filter(|m| m.key == *key) {
                if let Some(next) = (mutation.apply)(&value) {
                    value = next;
                }
            }
            self.daemon.values.insert((*key).to_owned(), value);
        }
    }

    fn pump_mutations(&mut self, cx: &mut Context<Self>) {
        if self.mutations.inflight || self.stale {
            return;
        }
        let Some(front) = self.mutations.queue.front() else {
            return;
        };
        let (id, key, apply) = (front.id, front.key, front.apply.clone());
        self.mutations.inflight = true;
        let port = self.port.clone();
        cx.spawn(async move |this, cx| {
            let outcome = run_mutation(port.as_ref(), key, apply.as_ref()).await;

            let Ok(()) = this.update(cx, |this, cx| this.finish_mutation(id, outcome, cx)) else {
                // 设置视图或窗口已释放，结束回调，不再更新状态。
                return;
            };
        })
        .detach();
    }

    fn finish_mutation(&mut self, id: u64, outcome: MutationOutcome, cx: &mut Context<Self>) {
        self.mutations.inflight = false;
        self.mutations.queue.retain(|mutation| mutation.id != id);
        let newer = outcome
            .latest
            .filter(|latest| !self.stale && latest.revision >= self.daemon.revision);
        if let Some(latest) = newer {
            // 与 `ConfigChanged` 同路径：基线跟随服务端，再盖回本地编辑与剩余变更。
            self.daemon = latest;
            self.refresh_daemon_baselines();
            self.overlay_local_edits();
        } else {
            self.overlay_mutations();
        }
        let queue = &self.mutations.queue;
        self.mutations
            .base
            .retain(|key, _| queue.iter().any(|mutation| mutation.key == *key));
        if let Some(error) = outcome.error {
            self.last_error = Some(SettingsError {
                kind: SettingsErrorKind::from_rpc(&error),
                detail: SharedString::default(),
            });
        }
        cx.notify();
        self.pump_mutations(cx);
    }
}

async fn run_mutation(
    port: &dyn SettingsPort,
    key: &'static str,
    apply: &dyn Fn(&str) -> Option<String>,
) -> MutationOutcome {
    let mut conflicts = 0;
    loop {
        let snapshot = match port.call(method::DAEMON_CONFIG_GET, json!({})).await {
            Ok(value) => match decode_snapshot(value) {
                Ok(snapshot) => snapshot,
                Err(error) => return failed(None, error),
            },
            Err(error) => return failed(None, error),
        };
        let current = snapshot
            .values
            .get(key)
            .cloned()
            .unwrap_or_else(|| fluxdown_protocol::daemon_config_default(key).to_owned());
        let Some(next) = apply(&current) else {
            return MutationOutcome {
                latest: Some(snapshot),
                error: None,
            };
        };
        let patch = DaemonConfigPatch {
            expected_revision: snapshot.revision,
            values: BTreeMap::from([(key.to_owned(), next)]),
        };
        let params = serde_json::to_value(patch).unwrap_or_else(|_| json!({}));
        match port.call(method::DAEMON_CONFIG_PATCH, params).await {
            Ok(value) => {
                return MutationOutcome {
                    latest: Some(decode_snapshot(value).unwrap_or(snapshot)),
                    error: None,
                };
            }
            Err(error)
                if error.code == ApplicationErrorCode::Conflict
                    && conflicts < MAX_CONFLICT_RETRIES =>
            {
                conflicts += 1;
            }
            Err(error) => return failed(Some(snapshot), error),
        }
    }
}

fn failed(latest: Option<DaemonConfigSnapshot>, error: RpcErrorData) -> MutationOutcome {
    MutationOutcome {
        latest,
        error: Some(error),
    }
}

fn decode_snapshot(value: Value) -> Result<DaemonConfigSnapshot, RpcErrorData> {
    serde_json::from_value(value)
        .map_err(|_| RpcErrorData::new(ApplicationErrorCode::Internal, false))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Mutex;
    use std::task::{Context, Poll, Waker};

    use fluxdown_protocol::{
        ApplicationErrorCode, DaemonConfigPatch, DaemonConfigSnapshot, RpcErrorData, method,
    };
    use serde_json::Value;

    use super::run_mutation;
    use crate::port::{PortFuture, SettingsPort};

    const KEY: &str = "webhook.endpoints";

    /// 内存 daemon：`config.get` 返回当前值；`config.patch` 校验修订。`concurrent`
    /// 模拟别处在本端读完之后、写入之前抢先写入的一次修改。
    struct FakeDaemon {
        state: Mutex<(u64, String)>,
        concurrent: Mutex<Option<String>>,
    }

    impl SettingsPort for FakeDaemon {
        fn call(&self, name: &'static str, params: Value) -> PortFuture<Value> {
            let result = (|| {
                let mut state = self.state.lock().map_err(|_| internal())?;
                if name == method::DAEMON_CONFIG_GET {
                    let snapshot = DaemonConfigSnapshot {
                        revision: state.0,
                        values: BTreeMap::from([(KEY.to_owned(), state.1.clone())]),
                    };
                    return serde_json::to_value(snapshot).map_err(|_| internal());
                }
                if let Some(value) = self.concurrent.lock().map_err(|_| internal())?.take() {
                    *state = (state.0 + 1, value);
                }
                let patch: DaemonConfigPatch =
                    serde_json::from_value(params).map_err(|_| internal())?;
                if patch.expected_revision != state.0 {
                    return Err(RpcErrorData::new(ApplicationErrorCode::Conflict, true));
                }
                let value = patch.values.get(KEY).cloned().unwrap_or_default();
                *state = (state.0 + 1, value);
                let snapshot = DaemonConfigSnapshot {
                    revision: state.0,
                    values: BTreeMap::from([(KEY.to_owned(), state.1.clone())]),
                };
                serde_json::to_value(snapshot).map_err(|_| internal())
            })();
            Box::pin(async move { result })
        }
    }

    fn internal() -> RpcErrorData {
        RpcErrorData::new(ApplicationErrorCode::Internal, false)
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = std::pin::pin!(future);
        let mut cx = Context::from_waker(Waker::noop());
        loop {
            if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
                return output;
            }
        }
    }

    #[test]
    fn conflicting_write_is_recomputed_on_the_latest_value() {
        let daemon = FakeDaemon {
            state: Mutex::new((1, "a".to_owned())),
            concurrent: Mutex::new(Some("a,b".to_owned())),
        };
        let append_c = |raw: &str| Some(format!("{raw},c"));
        let outcome = block_on(run_mutation(&daemon, KEY, &append_c));
        assert!(outcome.error.is_none());
        let state = daemon.state.lock().expect("state");
        assert_eq!(
            state.1, "a,b,c",
            "the concurrent write must survive; the edit is replayed on top of it"
        );
        assert_eq!(outcome.latest.map(|latest| latest.revision), Some(state.0));
    }

    #[test]
    fn no_op_edit_does_not_write() {
        let daemon = FakeDaemon {
            state: Mutex::new((4, "a".to_owned())),
            concurrent: Mutex::new(None),
        };
        let outcome = block_on(run_mutation(&daemon, KEY, &|_: &str| None));
        assert!(outcome.error.is_none());
        assert_eq!(daemon.state.lock().expect("state").0, 4);
    }
}
