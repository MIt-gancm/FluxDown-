import { describe, expect, test } from 'bun:test'
import { DAEMON_SYNC_KEYS, resolveDaemonOverlay, SYNCED_PREF_KEYS } from './writeStore'

// tsconfig 未引入 Bun 类型，仅声明测试用到的最小接口。
declare const Bun: { file(path: URL): { text(): Promise<string> } }

// 镜像契约：native/protocol/src/settings.rs 的 SYNC_SETTING_SPECS。
async function readSyncSpecs(): Promise<{ prefs: Set<string>; daemon: Map<string, string> }> {
  const source = await Bun.file(new URL('../../../../../native/protocol/src/settings.rs', import.meta.url)).text()
  const start = source.indexOf('pub const SYNC_SETTING_SPECS')
  const block = source.slice(start, source.indexOf('\n];', start))
  const prefs = new Set<string>()
  const daemon = new Map<string, string>()
  for (const match of block.matchAll(/spec!\(\s*"([^"]+)"\s*,\s*(\w+)\s*(?:,\s*"([^"]+)"\s*)?,?\s*\)/g)) {
    const [, key, owner, storage] = match
    if (key === undefined) continue
    if (owner === 'Daemon' && storage !== undefined) daemon.set(storage, key)
    else prefs.add(key)
  }
  return { prefs, daemon }
}

describe('同步目录镜像', () => {
  test('SYNCED_PREF_KEYS 与 Rust 偏好/agent 所有的同步项一致（含 custom_categories）', async () => {
    const specs = await readSyncSpecs()
    expect([...SYNCED_PREF_KEYS].sort()).toEqual([...specs.prefs].sort())
  })

  test('DAEMON_SYNC_KEYS 与 Rust daemon 所有的同步项一致', async () => {
    const specs = await readSyncSpecs()
    expect(Object.fromEntries(specs.daemon)).toEqual({ ...DAEMON_SYNC_KEYS })
  })
})

describe('resolveDaemonOverlay', () => {
  test('无覆盖值返回 undefined', () => {
    expect(resolveDaemonOverlay(undefined, '1', '2')).toBeUndefined()
  })

  test('快照未动或已追上覆盖值时保留覆盖', () => {
    expect(resolveDaemonOverlay('5', '1', '1')).toBe('5')
    expect(resolveDaemonOverlay('5', '1', '5')).toBe('5')
    expect(resolveDaemonOverlay('5', '1', undefined)).toBe('5')
  })

  test('快照被外部改成第三个值时覆盖作废', () => {
    expect(resolveDaemonOverlay('5', '1', '9')).toBeUndefined()
  })
})
