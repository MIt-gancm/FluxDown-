// 文件跟踪重扫节流：逐条镜像 `crates/downloads/src/model/file_rescan.rs::RescanThrottle`。
// 页面可见 / 重连等请求在最小间隔内折叠成一次尾沿补发，而不是丢弃——否则
// 「可见 → 切走删文件 → 再可见」会被吞掉（daemon 空闲时不再定时扫描）。

/** 两次重扫的最小间隔（毫秒）。 */
export const RESCAN_MIN_INTERVAL_MS = 10_000

/** 一次重扫请求的处理方式。 */
export type RescanDecision =
  | { kind: 'now' }
  /** 冷却中：在 `delayMs` 后补发一次尾沿重扫。 */
  | { kind: 'after'; delayMs: number }
  /** 冷却中且已有尾沿补发排队，本次折叠进去。 */
  | { kind: 'coalesced' }

export class RescanThrottle {
  private last: number | null = null
  private trailingPending = false

  /** 可合并的重扫请求。返回 `now` 时已记为最近一次重扫。 */
  request(now: number): RescanDecision {
    const elapsed = this.last === null ? null : Math.max(0, now - this.last)
    if (elapsed !== null && elapsed < RESCAN_MIN_INTERVAL_MS) {
      if (this.trailingPending) return { kind: 'coalesced' }
      this.trailingPending = true
      return { kind: 'after', delayMs: RESCAN_MIN_INTERVAL_MS - elapsed }
    }
    this.last = now
    return { kind: 'now' }
  }

  /** 尾沿补发执行时调用。 */
  trailingFired(now: number): void {
    this.trailingPending = false
    this.last = now
  }

  /** 不经节流的即时重扫也计入间隔。 */
  recordImmediate(now: number): void {
    this.last = now
  }
}
