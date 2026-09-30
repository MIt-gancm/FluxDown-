import { describe, expect, test } from 'bun:test'
import { composeSources, formatPercent } from './sourceComposition'

const b = (cdnBytes = 0, proxyBytes = 0, nicBytes = 0) => ({ cdnBytes, proxyBytes, nicBytes })
const bytesOf = (c: ReturnType<typeof composeSources>) => Object.fromEntries(c.slices.map((s) => [s.kind, s.bytes]))

describe('composeSources', () => {
  test('http 无加速 → 全部源站', () => {
    const c = composeSources('http', 1000, b())
    expect(bytesOf(c)).toEqual({ origin: 1000, cdn: 0, proxy: 0, nic: 0 })
  })
  test('http 部分加速', () => {
    const c = composeSources('http', 1000, b(250, 125))
    expect(bytesOf(c)).toEqual({ origin: 625, cdn: 250, proxy: 125, nic: 0 })
    expect(c.acceleratedShare).toBe(0.375)
  })
  test('加速字节超过已下载 → 按比例缩放', () => {
    const c = composeSources('http', 1000, b(900, 300))
    expect(bytesOf(c)).toEqual({ origin: 0, cdn: 750, proxy: 250, nic: 0 })
  })
  test('bt → p2p', () => {
    const c = composeSources('bt', 500, b())
    expect(c.slices).toEqual([{ kind: 'p2p', bytes: 500, fraction: 1 }])
  })
  test('ftp 无加速只有源站行', () => {
    expect(composeSources('ftp', 100, b()).slices.map((s) => s.kind)).toEqual(['origin'])
  })
  test('hls 网卡加速', () => {
    const c = composeSources('hls', 100, b(0, 0, 10))
    expect(bytesOf(c)).toEqual({ origin: 90, nic: 10 })
  })
  test('已下载为 0 → empty', () => {
    expect(composeSources('http', 0, b()).empty).toBe(true)
  })
  test('百分比格式', () => {
    expect(formatPercent(0)).toBe('0%')
    expect(formatPercent(0.0004)).toBe('<0.1%')
    expect(formatPercent(0.875)).toBe('87.5%')
  })
})
