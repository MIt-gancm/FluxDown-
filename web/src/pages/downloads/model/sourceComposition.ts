// 来源构成计算（纯函数）。镜像 crates/downloads/src/model/source_composition.rs，两端算法必须保持一致。

import type { TaskProtocol } from './task'

export type SourceKind = 'origin' | 'cdn' | 'proxy' | 'nic' | 'p2p'

export interface SourceSlice {
  kind: SourceKind
  bytes: number
  /** 0..1（downloaded == 0 时整体为 empty，不会产出切片）。 */
  fraction: number
}

export interface SourceComposition {
  empty: boolean
  downloaded: number
  slices: SourceSlice[]
  /** 加速路径字节占比 0..1。 */
  acceleratedShare: number
  p2p: boolean
}

export interface SourceBytesInput {
  cdnBytes: number
  proxyBytes: number
  nicBytes: number
}

const clamp = (n: number): number => (Number.isFinite(n) && n > 0 ? Math.floor(n) : 0)

export function composeSources(protocol: TaskProtocol, downloadedRaw: number, bytes: SourceBytesInput | null | undefined): SourceComposition {
  const downloaded = clamp(downloadedRaw)
  if (downloaded === 0) return { empty: true, downloaded: 0, slices: [], acceleratedShare: 0, p2p: protocol === 'bt' || protocol === 'ed2k' }
  if (protocol === 'bt' || protocol === 'ed2k') {
    return { empty: false, downloaded, slices: [{ kind: 'p2p', bytes: downloaded, fraction: 1 }], acceleratedShare: 0, p2p: true }
  }
  let cdn = clamp(bytes?.cdnBytes ?? 0)
  let proxy = clamp(bytes?.proxyBytes ?? 0)
  let nic = clamp(bytes?.nicBytes ?? 0)
  const accel = cdn + proxy + nic
  if (accel > downloaded) {
    cdn = Math.floor((cdn * downloaded) / accel)
    proxy = Math.floor((proxy * downloaded) / accel)
    nic = Math.floor((nic * downloaded) / accel)
  }
  const scaledAccel = cdn + proxy + nic
  const origin = downloaded - scaledAccel
  const http = protocol === 'http'
  const slices: SourceSlice[] = [{ kind: 'origin', bytes: origin, fraction: origin / downloaded }]
  const push = (kind: SourceKind, b: number) => {
    if (http || b > 0) slices.push({ kind, bytes: b, fraction: b / downloaded })
  }
  push('cdn', cdn)
  push('proxy', proxy)
  push('nic', nic)
  return { empty: false, downloaded, slices, acceleratedShare: scaledAccel / downloaded, p2p: false }
}

/** 一位小数百分比；恰为 0 → "0%"，(0, 0.001) → "<0.1%"。 */
export function formatPercent(fraction: number): string {
  if (!(fraction > 0)) return '0%'
  if (fraction < 0.001) return '<0.1%'
  return `${(fraction * 100).toFixed(1)}%`
}
