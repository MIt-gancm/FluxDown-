import { describe, expect, test } from 'bun:test'
import { RESCAN_MIN_INTERVAL_MS, RescanThrottle } from './rescanThrottle'

// 用例与 crates/downloads/src/model/file_rescan.rs 的测试一一对应。
describe('RescanThrottle', () => {
  test('requests inside cooldown collapse into one trailing rescan', () => {
    const start = 1_000
    const throttle = new RescanThrottle()
    expect(throttle.request(start)).toEqual({ kind: 'now' })
    expect(throttle.request(start + 3_000)).toEqual({ kind: 'after', delayMs: RESCAN_MIN_INTERVAL_MS - 3_000 })
    expect(throttle.request(start + 5_000)).toEqual({ kind: 'coalesced' })

    const fired = start + RESCAN_MIN_INTERVAL_MS
    throttle.trailingFired(fired)
    expect(throttle.request(fired + 1_000).kind).toBe('after')
    expect(throttle.request(fired + 1_000)).toEqual({ kind: 'coalesced' })
  })

  test('immediate rescan restarts cooldown and expiry allows rescan', () => {
    const start = 1_000
    const a = new RescanThrottle()
    a.recordImmediate(start)
    expect(a.request(start + 1_000).kind).toBe('after')

    const b = new RescanThrottle()
    expect(b.request(start)).toEqual({ kind: 'now' })
    expect(b.request(start + RESCAN_MIN_INTERVAL_MS)).toEqual({ kind: 'now' })
  })
})
