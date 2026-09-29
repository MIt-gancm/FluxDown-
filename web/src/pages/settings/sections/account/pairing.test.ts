import { describe, expect, test } from 'bun:test'
import type { LinkDiscoveredPeer } from '../../../../lib/rpc'
import { isManualAddressValid, isPairingCodeComplete, normalizePairingCode, peerAddress, unpairedPeers } from './pairing'

const peer = (over: Partial<LinkDiscoveredPeer>): LinkDiscoveredPeer => ({ name: 'p', host: '10.0.0.2', port: 17800, source: 'mdns', ...over })

describe('局域网配对规则', () => {
  test('发现对端地址：IPv6 加方括号，已带括号不重复', () => {
    expect(peerAddress(peer({}))).toBe('10.0.0.2:17800')
    expect(peerAddress(peer({ host: 'fe80::1', port: 9 }))).toBe('[fe80::1]:9')
    expect(peerAddress(peer({ host: '[::1]', port: 9 }))).toBe('[::1]:9')
  })

  test('配对码只留数字并截断 6 位', () => {
    expect(normalizePairingCode('12a3-45 678')).toBe('123456')
    expect(isPairingCodeComplete('12345')).toBe(false)
    expect(isPairingCodeComplete('123456')).toBe(true)
  })

  test('手动地址：https 与带路径的写法放行，空白拒绝', () => {
    expect(isManualAddressValid('https://nas.example.com/fluxdown')).toBe(true)
    expect(isManualAddressValid('192.168.1.5')).toBe(true)
    expect(isManualAddressValid('  ')).toBe(false)
    expect(isManualAddressValid('a b')).toBe(false)
  })

  test('已配对设备不再出现在发现列表；指纹未知保留', () => {
    const list = [peer({ fingerprint: 'a' }), peer({ fingerprint: 'b' }), peer({})]
    expect(unpairedPeers(list, [{ fingerprint: 'a' }]).map((item) => item.fingerprint)).toEqual(['b', undefined])
  })
})
