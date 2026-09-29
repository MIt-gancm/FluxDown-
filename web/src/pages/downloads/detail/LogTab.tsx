import { useT } from '../../../i18n'
import { Button } from '../../../ui'
import type { TaskActivityDto } from '../../../lib/rpc'
import { stateLabel } from '../state'
import type { TaskState } from '../model/task'
import { Note } from './DetailRow'
import { useActivity } from './useActivity'

const KIND_KEY: Record<string, string> = {
  status: 'detailActivityKindStatus',
  error: 'detailActivityKindError',
  split: 'detailActivityKindSplit',
  cdn_pool: 'detailActivityKindCdnPool',
  cdn_kick: 'detailActivityKindCdnKick',
  cdn_breaker: 'detailActivityKindCdnBreaker',
  cdn_fallback: 'detailActivityKindCdnFallback',
  cdn_summary: 'detailActivityKindCdnSummary',
  nic_links: 'detailActivityKindNicLinks',
  nic_off: 'detailActivityKindNicOff',
  retry: 'detailActivityKindRetry',
  journal_overflow: 'detailActivityKindJournalOverflow',
}

function activityState(status: number): TaskState {
  switch (status) {
    case 0:
    case 5:
      return 'pending'
    case 1:
      return 'downloading'
    case 2:
      return 'paused'
    case 3:
      return 'completed'
    default:
      return 'failed'
  }
}

const pad = (n: number, w = 2) => String(n).padStart(w, '0')

/** `YYYY-MM-DD HH:MM:SS.mmm`（本地时区）。 */
function formatTimestamp(ms: number): string {
  const d = new Date(ms)
  if (Number.isNaN(d.getTime())) return '—'
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`
}

export function LogTab({ taskId }: { taskId: string }) {
  const t = useT()
  const { feed, loadOlder, retry } = useActivity(taskId)
  const { oldest, newest, truncated } = feed.retainedRange()
  const entries = feed.entries().reverse()

  const describe = (entry: TaskActivityDto): string => {
    const kindKey = KIND_KEY[entry.kind] ?? 'detailActivityKindUnknown'
    const label = t(kindKey)
    if (entry.kind === 'status' && entry.status !== null) return `${label}: ${stateLabel(t, activityState(entry.status))}`
    if (kindKey === 'detailActivityKindUnknown') {
      return entry.message === '' ? `${label} (${entry.kind})` : `${label} (${entry.kind}): ${entry.message}`
    }
    return entry.message === '' ? label : `${label}: ${entry.message}`
  }

  return (
    <div className="flex flex-col gap-1">
      <Note>{t('detailLogHint')}</Note>
      {truncated ? <Note tone="warning">{t('detailActivityTruncated')}</Note> : null}
      {feed.hasJournalGap() ? <Note tone="warning">{t('detailActivityJournalGap')}</Note> : null}
      {oldest !== null && newest !== null ? (
        <Note className="tabular">{t('detailActivityRetainedRange', { oldest: `#${oldest}`, newest: `#${newest}` })}</Note>
      ) : null}
      {feed.failed() ? (
        <div className="flex flex-wrap items-center gap-2">
          <Note tone="destructive">{t('detailActivityQueryFailed')}</Note>
          <Button onClick={retry}>{t('detailActivityRetry')}</Button>
        </div>
      ) : null}
      {feed.isLoading() ? <Note>{t('detailActivityLoading')}</Note> : null}
      {feed.loaded() && entries.length === 0 ? <Note>{t('detailLogEmpty')}</Note> : null}
      {entries.map((entry) => (
        <div key={entry.id} className="flex items-start gap-2 text-xs mobile:flex-col mobile:gap-0">
          <div className="tabular w-[168px] shrink-0 text-text-tertiary">{formatTimestamp(entry.timestampMs)}</div>
          <div className={entry.kind === 'journal_overflow' ? 'min-w-0 flex-1 break-words text-warning' : 'min-w-0 flex-1 break-words text-foreground'}>
            {describe(entry)}
          </div>
        </div>
      ))}
      {feed.hasOlder() && !feed.isLoading() && !feed.failed() ? (
        <div className="pt-1">
          <Button variant="ghost" onClick={loadOlder}>
            {t('detailActivityLoadMore')}
          </Button>
        </div>
      ) : null}
    </div>
  )
}
