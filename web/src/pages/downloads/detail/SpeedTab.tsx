import { useT } from '../../../i18n'
import { formatBytes, MAX_ETA_SECS } from '../model/task'
import type { DownloadTaskView } from '../model/task'
import { formatEta } from '../table/text'
import { DetailRow, Note } from './DetailRow'
import { SPEED_HISTORY_CAPACITY as CAPACITY } from './useSpeedHistory'

const W = 400
const H = 120

function Tile({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex min-w-0 flex-1 flex-col gap-0.5 rounded-md border border-hairline bg-chrome px-3 py-2">
      <div className="truncate text-xs text-text-tertiary">{label}</div>
      <div className="tabular truncate text-sm font-medium text-foreground">{value}</div>
    </div>
  )
}

export function SpeedTab({ view, history }: { view: DownloadTaskView; history: readonly number[] }) {
  const t = useT()
  const downloading = view.state === 'downloading'
  const current = downloading ? (view.speed ?? 0) : 0
  const avg = history.length > 0 ? history.reduce((a, v) => a + v, 0) / history.length : 0
  const peak = history.reduce((a, v) => Math.max(a, v), 0)
  const fmt = (v: number) => `${formatBytes(v)}/s`
  // 少于 2 点或全程为 0（未在下载）时不画平线，给出说明。
  const hasChart = history.length >= 2 && peak > 0

  const max = Math.max(peak, 1)
  const step = W / (CAPACITY - 1)
  const x0 = (CAPACITY - history.length) * step
  const pts = history.map((v, i) => `${(x0 + i * step).toFixed(1)},${(H - 2 - (v / max) * (H - 6)).toFixed(1)}`)
  const line = pts.join(' ')
  const area = `${x0.toFixed(1)},${H} ${line} ${W},${H}`

  const rt = view.runtime
  const limit = rt?.parallelismLimit
  return (
    <div className="flex flex-col gap-3">
      <div className="flex gap-2 mobile:flex-col">
        <Tile label={t('infoSpeed')} value={fmt(current)} />
        <Tile label={t('speedAverageRecent')} value={fmt(avg)} />
        <Tile label={t('speedPeakRecent')} value={fmt(peak)} />
      </div>
      <div className="rounded-md border border-hairline bg-chrome p-2">
        {hasChart ? (
          <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="h-28 w-full text-primary" role="img" aria-label={t('detailTabSpeed')}>
            <polygon points={area} fill="currentColor" fillOpacity={0.18} />
            <polyline points={line} fill="none" stroke="currentColor" strokeWidth={1.5} vectorEffect="non-scaling-stroke" />
          </svg>
        ) : (
          <div className="flex h-28 items-center justify-center px-3 text-center">
            <Note>{t('speedChartEmpty')}</Note>
          </div>
        )}
      </div>
      <div className="flex flex-col">
        {downloading && view.etaSeconds !== null && view.etaSeconds <= MAX_ETA_SECS ? (
          <DetailRow label={t('infoRemaining')}>{formatEta(t, view.etaSeconds)}</DetailRow>
        ) : null}
        {downloading && rt && rt.activeTransfers !== null ? (
          <DetailRow label={t('taskActiveTransfers')}>
            {limit != null && limit > 0 ? `${rt.activeTransfers} / ${limit}` : String(rt.activeTransfers)}
          </DetailRow>
        ) : null}
        {view.protocol === 'bt' && rt && rt.connectedPeers !== null ? (
          <DetailRow label={t('taskConnectedPeers')}>{rt.connectedPeers}</DetailRow>
        ) : null}
      </div>
    </div>
  )
}
