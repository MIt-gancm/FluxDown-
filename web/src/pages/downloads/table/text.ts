// 任务表 / 卡片共用的文案与映射（状态文案、并发详情、类型图标）。

import { AppWindow, Disc3, File, FileArchive, FileImage, FileMusic, FilePlay, FileText, Smartphone } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { activeTransfers, formatBytes, MAX_ETA_SECS } from '../model/task'
import type { DownloadTaskView, TaskKind, TaskState } from '../model/task'
import { stateLabel } from '../state'

export type Translate = (key: string, params?: Record<string, string | number>) => string

export const KIND_ICON: Record<TaskKind, LucideIcon> = {
  video: FilePlay,
  audio: FileMusic,
  document: FileText,
  image: FileImage,
  archive: FileArchive,
  diskImage: Disc3,
  application: AppWindow,
  mobile: Smartphone,
  other: File,
}

const KIND_LABEL_KEY: Record<TaskKind, string> = {
  video: 'categoryVideo',
  audio: 'categoryAudio',
  document: 'categoryDocument',
  image: 'categoryImage',
  archive: 'categoryArchive',
  diskImage: 'categoryArchive',
  application: 'categoryProgram',
  mobile: 'categoryProgram',
  other: 'categoryOther',
}

export const kindLabel = (t: Translate, kind: TaskKind): string => t(KIND_LABEL_KEY[kind])

export const STATUS_TEXT: Record<TaskState, string> = {
  downloading: 'text-status-downloading',
  failed: 'text-status-failed',
  paused: 'text-status-paused',
  pending: 'text-status-queued',
  completed: 'text-status-completed',
}

export function formatEta(t: Translate, seconds: number): string {
  if (seconds < 60) return t('etaSeconds', { n: seconds })
  if (seconds < 3600) return t('etaMinutes', { n: Math.floor(seconds / 60) })
  return t('etaHours', { n: (seconds / 3600).toFixed(1) })
}

/** 状态列主文案：下载中显示「速度 · 剩余时间」（只显示已知部分），其余为状态名。 */
export function statusLabel(t: Translate, view: DownloadTaskView): string {
  if (view.state !== 'downloading') return stateLabel(t, view.state)
  const speed = view.speed !== null && view.speed > 0 ? `${formatBytes(view.speed)}/s` : null
  const eta = view.etaSeconds !== null && view.etaSeconds <= MAX_ETA_SECS ? formatEta(t, view.etaSeconds) : null
  if (speed && eta) return `${speed} · ${eta}`
  return speed ?? eta ?? t('statusDownloading')
}

function bytesProgress(view: DownloadTaskView): string {
  return view.sizeBytes > 0
    ? `${formatBytes(view.downloadedBytes)} / ${formatBytes(view.sizeBytes)}`
    : formatBytes(view.downloadedBytes)
}

/** 实时并发：本地分段连接数与 BT 已连节点数。 */
function transferDetail(t: Translate, view: DownloadTaskView): string | null {
  const active = activeTransfers(view)
  const peers = view.runtimeConnected && view.protocol === 'bt' ? (view.runtime?.connectedPeers ?? null) : null
  if (active !== null && peers !== null) {
    return `${active} ${t('taskActiveTransfers')} · ${peers} ${t('taskConnectedPeers')}`
  }
  if (active !== null) return `${active} ${t('taskActiveTransfers')}`
  if (peers !== null) return `${peers} ${t('taskConnectedPeers')}`
  return null
}

/** 状态列第二行 / 紧凑密度悬停提示：已下 / 总量、并发连接、失败原因首行。 */
export function statusDetail(t: Translate, view: DownloadTaskView): string | null {
  switch (view.state) {
    case 'downloading': {
      const bytes = bytesProgress(view)
      const transfers = transferDetail(t, view)
      return transfers ? `${bytes} · ${transfers}` : bytes
    }
    case 'paused':
      return bytesProgress(view)
    case 'failed': {
      const line = view.errorMessage.split('\n', 1)[0]?.trim() ?? ''
      return line === '' ? null : line
    }
    default:
      return null
  }
}

