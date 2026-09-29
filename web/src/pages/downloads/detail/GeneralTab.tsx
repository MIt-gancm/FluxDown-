import { Copy, Download } from 'lucide-react'
import { useT } from '../../../i18n'
import { Button, Icon, Tooltip } from '../../../ui'
import { copyText } from '../../../lib/copy'
import { useDownloads, stateLabel } from '../state'
import { copyUrls, downloadViewsFiles, isDownloadable } from '../model/actions'
import { MAX_ETA_SECS, PROTOCOL_LABEL, formatBytes, formatDateTime, sourceSite } from '../model/task'
import type { DownloadTaskView, TaskState } from '../model/task'
import { openGroupDetail } from '../dialogs'
import { DetailRow } from './DetailRow'

const STATE_TEXT: Record<TaskState, string> = {
  pending: 'text-status-queued',
  downloading: 'text-status-downloading',
  paused: 'text-status-paused',
  completed: 'text-status-completed',
  failed: 'text-status-failed',
}

/** 剩余时间：与 GPUI `format_eta` 一致（<60s 秒 / <1h 分 / 否则小时 1 位小数）。 */
function formatEta(t: ReturnType<typeof useT>, seconds: number): string {
  if (seconds < 60) return t('etaSeconds', { n: String(seconds) })
  if (seconds < 3600) return t('etaMinutes', { n: String(Math.floor(seconds / 60)) })
  return t('etaHours', { n: (seconds / 3600).toFixed(1) })
}

export function GeneralTab({ view }: { view: DownloadTaskView }) {
  const t = useT()
  const { queueName } = useDownloads()
  const dto = view.dto
  const notSet = t('detailNotSet')
  const checksum = dto && dto.checksum !== '' ? dto.checksum : notSet
  const proxy = dto && dto.proxyUrl !== '' ? dto.proxyUrl : t('detailFollowGlobal')
  const queueLabel = view.queueId === '' ? '' : (queueName(view.queueId) ?? view.queueId)
  const completed = view.state === 'completed'
  const downloading = view.state === 'downloading'

  return (
    <div className="flex flex-col">
      <div className="flex flex-col gap-0.5 pb-2">
        <div className="min-w-0 truncate text-sm font-medium">{view.name}</div>
        <div className="text-xs text-muted-foreground">
          {PROTOCOL_LABEL[view.protocol]} · {sourceSite(view)}
        </div>
        {view.boosted ? <div className="text-xs text-warning">{t('detailBoostActive')}</div> : null}
      </div>
      <DetailRow label={t('infoStatus')}>
        <span className={STATE_TEXT[view.state]}>{stateLabel(t, view.state)}</span>
      </DetailRow>
      {view.sizeBytes > 0 ? <DetailRow label={t('infoSize')}>{formatBytes(view.sizeBytes)}</DetailRow> : null}
      {!completed ? <DetailRow label={t('infoDownloaded')}>{formatBytes(view.downloadedBytes)}</DetailRow> : null}
      {downloading && view.speed !== null ? <DetailRow label={t('infoSpeed')}>{formatBytes(view.speed)}/s</DetailRow> : null}
      {downloading && view.etaSeconds !== null && view.etaSeconds <= MAX_ETA_SECS ? (
        <DetailRow label={t('infoRemaining')}>{formatEta(t, view.etaSeconds)}</DetailRow>
      ) : null}
      {view.createdAtSecs > 0 ? <DetailRow label={t('infoStartedAt')}>{formatDateTime(view.createdAtSecs)}</DetailRow> : null}
      {view.completedAtSecs > 0 ? <DetailRow label={t('infoCompletedAt')}>{formatDateTime(view.completedAtSecs)}</DetailRow> : null}
      <DetailRow label={t('infoPath')}>
        <div className="flex items-center gap-1">
          <span className="min-w-0 flex-1 break-all text-accent-text">{view.saveDir}</span>
          <Tooltip content={t('webCopy')}>
            <Button
              variant="ghost"
              iconOnly
              aria-label={t('webCopy')}
              onClick={() => copyText(view.saveDir)}
            >
              <Icon icon={Copy} />
            </Button>
          </Tooltip>
        </div>
      </DetailRow>
      <DetailRow label={t('taskQueueLabel')}>{queueLabel}</DetailRow>
      <DetailRow label={t('taskChecksum')}>{checksum}</DetailRow>
      <DetailRow label={t('taskProxy')}>{proxy}</DetailRow>
      {dto?.ignoreTlsErrors ? <DetailRow label={t('taskIgnoreTlsErrors')}>✓</DetailRow> : null}
      {view.errorMessage !== '' ? (
        <DetailRow label={t('infoError')}>
          <span className="text-destructive">{view.errorMessage}</span>
        </DetailRow>
      ) : null}
      {view.groupId !== '' ? (
        <DetailRow label={t('groupMemberOfLabel')}>
          <Button variant="ghost" onClick={() => openGroupDetail(view.groupId)}>
            {t('openGroupInWindowAction')}
          </Button>
        </DetailRow>
      ) : null}
      <div className="flex flex-wrap gap-2 pt-3">
        {isDownloadable(view) ? (
          <Button variant="primary" icon={Download} disabled={view.fileMissing} onClick={() => downloadViewsFiles([view])}>
            {t('webDownloadFile')}
          </Button>
        ) : null}
        <Button icon={Copy} onClick={() => copyUrls([view])}>
          {t('copyUrl')}
        </Button>
      </div>
    </div>
  )
}
