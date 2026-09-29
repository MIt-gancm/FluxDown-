import { Check } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useState } from 'react'
import { useT } from '../../../i18n'
import { cn } from '../../../lib/cn'
import { RpcError, rpc, rpcStore, useConfigValue } from '../../../lib/rpc'
import { Button, Icon, Input, Popover, toast, Tooltip } from '../../../ui'
import { formatBytes } from '../model/task'

/** 限速预设（MB/s），同 GPUI `SPEED_PRESETS_MB`。 */
const SPEED_PRESETS_MB = [1, 5, 10, 50]
const mbToBytes = (mb: number) => mb * 1_048_576
const kbToBytes = (kb: number) => Math.max(0, kb) * 1024

const readRevision = (): number => rpcStore.peek().snapshot?.daemon.config.revision ?? 0

/** 写限速配置（字节/秒）；revision 冲突时用新 revision 重试一次。 */
async function patchLimit(key: string, bytes: number): Promise<void> {
  const values = { [key]: String(bytes) }
  let revision = readRevision()
  for (let attempt = 0; attempt < 2; attempt++) {
    try {
      await rpc.daemon.config.patch({ expectedRevision: revision, values })
      return
    } catch (err) {
      if (err instanceof RpcError && err.is('conflict')) {
        if (attempt === 0) {
          revision = err.revision ?? readRevision()
          continue
        }
        toast.key('localServiceConflict', 'error')
        return
      }
      toast.error(err)
      return
    }
  }
}

export function SpeedLimitControl({
  icon,
  titleKey,
  configKey,
  disabled,
}: {
  icon: LucideIcon
  titleKey: 'speedLimitTitle' | 'uploadLimit'
  configKey: 'speed_limit_bytes' | 'upload_limit_bytes'
  disabled: boolean
}) {
  const t = useT()
  const raw = useConfigValue(configKey)
  const parsed = Number.parseInt(raw ?? '', 10)
  const value = Number.isFinite(parsed) ? parsed : 0
  const [custom, setCustom] = useState('')
  const title = t(titleKey)
  const off = t('statusSpeedLimitOff')
  const label = value <= 0 ? off : `${formatBytes(value)}/s`
  const presetHit = value <= 0 || SPEED_PRESETS_MB.some((mb) => mbToBytes(mb) === value)
  const desc = t(configKey === 'upload_limit_bytes' ? 'uploadLimitDesc' : 'speedLimitDesc')

  const row = (key: string, text: string, checked: boolean, bytes: number, close: () => void) => (
    <button
      key={key}
      type="button"
      disabled={disabled}
      onClick={() => {
        close()
        void patchLimit(configKey, bytes)
      }}
      className="flex h-control w-full items-center gap-2 rounded-sm px-2 text-left text-sm hover:bg-nav-hover disabled:opacity-50 coarse:min-h-touch"
    >
      <span className="inline-flex w-3.5 justify-center">{checked ? <Icon icon={Check} /> : null}</span>
      {text}
    </button>
  )

  const apply = (close: () => void) => {
    const kb = Number.parseInt(custom, 10)
    if (!Number.isFinite(kb)) return
    close()
    void patchLimit(configKey, kbToBytes(kb))
  }

  return (
    <Popover
      title={title}
      align="end"
      trigger={
        <Tooltip content={title} side="top">
          <button
            type="button"
            disabled={disabled}
            aria-label={`${title} ${label}`}
            className={cn(
              'inline-flex h-status-control shrink-0 items-center justify-center gap-1 whitespace-nowrap rounded-sm px-1.5 text-caption tabular-nums hover:bg-nav-hover disabled:opacity-50 coarse:min-h-touch coarse:min-w-touch',
              value > 0 ? 'text-accent-text' : 'text-muted-foreground',
            )}
          >
            <Icon icon={icon} />
            {/* 窄屏且未限速时只留图标，避免状态栏横向溢出。 */}
            <span className={cn(value <= 0 && 'narrow:hidden')}>{label}</span>
          </button>
        </Tooltip>
      }
    >
      {(close) => (
        <div className="flex min-w-56 flex-col p-1">
          {row('off', off, value <= 0, 0, close)}
          {SPEED_PRESETS_MB.map((mb) => row(String(mb), `${mb} MB/s`, mbToBytes(mb) === value, mbToBytes(mb), close))}
          <div className="my-1 h-px bg-hairline" />
          <div className="flex flex-col gap-1.5 px-2 py-1">
            <span className="flex items-center gap-2 text-sm">
              <span className="inline-flex w-3.5 justify-center">{!presetHit ? <Icon icon={Check} /> : null}</span>
              {t('speedLimitCustom')}
            </span>
            <div className="flex items-center gap-2">
              <Input
                type="number"
                inputMode="numeric"
                min={0}
                value={custom}
                placeholder={!presetHit ? String(Math.round(value / 1024)) : '0'}
                onChange={(e) => setCustom(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === 'Enter') apply(close)
                }}
                disabled={disabled}
                className="flex-1"
              />
              <span className="text-xs text-muted-foreground">{t('statusSpeedLimitKbs')}</span>
              <Button variant="primary" disabled={disabled || custom.trim() === ''} onClick={() => apply(close)}>
                {t('confirm')}
              </Button>
            </div>
            <span className="text-caption text-text-tertiary">{desc}</span>
          </div>
        </div>
      )}
    </Popover>
  )
}
