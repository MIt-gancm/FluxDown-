// 局域网配对的纯规则：发现结果 → 地址、配对码归一化、已配对过滤。

import type { LinkDeviceInfo, LinkDiscoveredPeer } from '../../../../lib/rpc'

export const PAIRING_CODE_LENGTH = 6

/** 发现的对端 → `agent.link.pairBegin` 接受的 `host:port`（IPv6 加方括号）。 */
export function peerAddress(peer: Pick<LinkDiscoveredPeer, 'host' | 'port'>): string {
  const host = peer.host.includes(':') && !peer.host.startsWith('[') ? `[${peer.host}]` : peer.host
  return `${host}:${peer.port}`
}

/** 配对码只保留数字并截断到 6 位。 */
export function normalizePairingCode(raw: string): string {
  return raw.replace(/\D/g, '').slice(0, PAIRING_CODE_LENGTH)
}

export function isPairingCodeComplete(code: string): boolean {
  return code.length === PAIRING_CODE_LENGTH
}

/** 手动地址只做非空检查：`host`、`host:port`、`http(s)://…` 的解析与端口校验由 agent 负责（缺失端口有默认值）。 */
export function isManualAddressValid(address: string): boolean {
  return address.trim() !== '' && !/\s/.test(address.trim())
}

/** 发现列表里去掉已配对的设备（按指纹）。指纹未知的 mDNS 条目保留。 */
export function unpairedPeers(
  peers: readonly LinkDiscoveredPeer[],
  paired: readonly Pick<LinkDeviceInfo, 'fingerprint'>[],
): LinkDiscoveredPeer[] {
  const known = new Set(paired.map((device) => device.fingerprint))
  return peers.filter((peer) => !peer.fingerprint || !known.has(peer.fingerprint))
}
