// 本机服务协议的通用类型（镜像 native/protocol/src/*.rs 的 serde JSON 形状）。
// 约定：Rust i64/u64/f64/u32 一律 number；BTreeMap/HashMap<String,V> → Record<string,V>；
// Option<T> 无 skip_serializing_if 时为 `T | null`，有 skip 的字段为可选 `?:`。

/** serde_json::Value。 */
export type JsonValue =
  | string
  | number
  | boolean
  | null
  | JsonValue[]
  | { [key: string]: JsonValue };

/** 服务职责（`ServiceRole`，camelCase）。 */
export type ServiceRole = 'daemon' | 'agent';

/** 多数「无返回值」方法的结果 `{"ok":true}`。 */
export interface OkResult {
  ok: boolean;
}

/** 内置主队列的稳定 ID。 */
export const MAIN_QUEUE_ID = 'main';
/** 内置「稍后下载」队列的稳定 ID。 */
export const LATER_QUEUE_ID = 'later';
