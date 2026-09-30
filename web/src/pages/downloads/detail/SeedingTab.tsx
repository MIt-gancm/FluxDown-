import { useState } from 'react'
import { useT } from '../../../i18n'
import { SEED_LIMIT_INHERIT, rpc } from '../../../lib/rpc'
import { Button, FormField, Input, toast } from '../../../ui'
import { toastRpcError } from '../../../lib/rpcToast'
import { formatBytes } from '../model/task'
import type { DownloadTaskView } from '../model/task'
import { DetailRow } from './DetailRow'
import { uploadLimitText, uploadLimitToSend } from './seedUpload'

const STATUS_KEY: Record<number, string> = {
  1: 'seedingStatusSeeding',
  2: 'seedingStatusRatioReached',
  3: 'seedingStatusTimeReached',
  4: 'seedingStatusUserStopped',
  5: 'seedingStatusDeleted',
  6: 'seedingStatusSessionReleased',
  7: 'seedingStatusInactiveReached',
  8: 'seedingStatusQueued',
}

const isValid = (text: string) => text.trim() === '' || /^[+-]?\d+$/.test(text.trim())

/** 空 / 非法 = 跟随全局（`parse_seed_limit`）。 */
function parseSeedLimit(text: string): number {
  const trimmed = text.trim()
  if (trimmed === '') return SEED_LIMIT_INHERIT
  return /^[+-]?\d+$/.test(trimmed) ? Number.parseInt(trimmed, 10) : SEED_LIMIT_INHERIT
}

/** 任务现值 → 输入框文本：跟随全局（-2）与缺省显示为空，其余原样回显。 */
function seedLimitText(value: number | undefined): string {
  return value === undefined || value === SEED_LIMIT_INHERIT ? '' : String(value)
}

function formatDuration(t: ReturnType<typeof useT>, totalSeconds: number): string {
  const minutes = Math.floor(Math.max(0, totalSeconds) / 60)
  if (minutes < 60) return `${minutes} ${t('timeUnitMinutes')}`
  return `${Math.floor(minutes / 60)} ${t('timeUnitHours')} ${minutes % 60} ${t('timeUnitMinutes')}`
}

function SeedField({ label, hint, value, onChange }: { label: string; hint?: string; value: string; onChange: (v: string) => void }) {
  return (
    <FormField label={label} hint={hint}>
      <Input inputMode="numeric" value={value} invalid={!isValid(value)} onChange={(e) => onChange(e.target.value)} />
    </FormField>
  )
}

export function SeedingTab({ view }: { view: DownloadTaskView }) {
  const t = useT()
  const [ratio, setRatio] = useState(() => seedLimitText(view.dto?.seedRatioLimitMilli))
  const [postRatio, setPostRatio] = useState(() => seedLimitText(view.dto?.seedPostRatioLimitMilli))
  const [seedTime, setSeedTime] = useState(() => seedLimitText(view.dto?.seedTimeLimitMinutes))
  const [inactive, setInactive] = useState(() => seedLimitText(view.dto?.seedInactiveTimeLimitMinutes))
  const [initialUpload] = useState(() => uploadLimitText(view.dto?.seedUploadLimitBps))
  const [upload, setUpload] = useState(initialUpload)
  const [saving, setSaving] = useState(false)

  const uploaded = Math.max(0, view.uploadedBytes)
  const ratioLabel = view.sizeBytes > 0 ? (uploaded / view.sizeBytes).toFixed(2) : '—'
  const statusKey = STATUS_KEY[view.seedingStatus] ?? 'seedingStatusNone'

  const save = () => {
    setSaving(true)
    rpc.daemon.task
      .setSeedLimits({
        taskId: view.taskId,
        ratioLimitMilli: parseSeedLimit(ratio),
        postRatioLimitMilli: parseSeedLimit(postRatio),
        seedTimeLimitMinutes: parseSeedLimit(seedTime),
        inactiveTimeLimitMinutes: parseSeedLimit(inactive),
        uploadLimitBps: uploadLimitToSend(upload, initialUpload, view.dto?.seedUploadLimitBps),
      })
      .then(() => toast.key('btSeedLimitsSaved', 'success'))
      .catch(toastRpcError)
      .finally(() => setSaving(false))
  }

  return (
    <div className="flex flex-col">
      <DetailRow label={t('seedingStatus')}>{t(statusKey)}</DetailRow>
      <DetailRow label={t('uploadedTotal')}>{formatBytes(uploaded)}</DetailRow>
      <DetailRow label={t('seedRatio')}>{ratioLabel}</DetailRow>
      <DetailRow label={t('seedTime')}>{formatDuration(t, view.dto?.seedingTimeSecs ?? 0)}</DetailRow>
      <div className="my-3 h-px bg-hairline" />
      <div className="pb-3 text-caption font-medium text-text-tertiary">{t('btSeedLimitsTitle')}</div>
      <div className="flex flex-col gap-3">
        <div className="grid grid-cols-2 gap-3 mobile:grid-cols-1">
          <SeedField label={t('btSeedRatioLimit')} value={ratio} onChange={setRatio} />
          <SeedField label={t('btSeedPostRatioLimit')} value={postRatio} onChange={setPostRatio} />
        </div>
        <div className="grid grid-cols-2 gap-3 mobile:grid-cols-1">
          <SeedField label={t('btSeedTimeLimit')} value={seedTime} onChange={setSeedTime} />
          <SeedField label={t('btSeedInactiveTimeLimit')} value={inactive} onChange={setInactive} />
        </div>
        <SeedField label={t('btSeedUploadLimit')} hint={t('btSeedUploadLimitHint')} value={upload} onChange={setUpload} />
        <div className="flex justify-end">
          <Button variant="primary" loading={saving} onClick={save}>
            {t('confirm')}
          </Button>
        </div>
      </div>
    </div>
  )
}
