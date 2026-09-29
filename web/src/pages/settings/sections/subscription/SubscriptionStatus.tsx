// 订阅状态行：已订阅条数、更新时间/失败提示、立即更新（GPUI `subscription::status_item`）。

import { useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import { Button } from '../../../../ui'
import { SettingsCustomRow, useDaemonNumber, useDaemonValue } from '../../kit'
import { formatUnix, listEntries } from './listFormat'
import type { ListFormat } from './listFormat'

export type SubscriptionKind = 'btTrackers' | 'ed2kServers'

const SPEC = {
  btTrackers: { cacheKey: 'bt_tracker_sub_cache', updatedKey: 'bt_tracker_sub_updated_at', format: 'lines', prefix: 'btTrackerSub' },
  ed2kServers: { cacheKey: 'ed2k_server_sub_cache', updatedKey: 'ed2k_server_sub_updated_at', format: 'comma', prefix: 'ed2kServerSub' },
} as const satisfies Record<SubscriptionKind, { cacheKey: string; updatedKey: string; format: ListFormat; prefix: string }>

async function refresh(kind: SubscriptionKind): Promise<{ success: boolean; updatedAt: number }> {
  const result = kind === 'btTrackers' ? await rpc.daemon.bt.trackerSubscription.refresh() : await rpc.daemon.ed2k.serverSubscription.refresh()
  return { success: result.success, updatedAt: result.updatedAt }
}

export function SubscriptionStatusRow({ kind }: { kind: SubscriptionKind }) {
  const t = useT()
  const spec = SPEC[kind]
  const cache = useDaemonValue(spec.cacheKey)
  const storedUpdatedAt = useDaemonNumber(spec.updatedKey)
  const [busy, setBusy] = useState(false)
  const [failed, setFailed] = useState(false)
  const [freshAt, setFreshAt] = useState(0)
  const count = listEntries(spec.format, cache).length
  const updatedAt = Math.max(storedUpdatedAt, freshAt)

  const run = async () => {
    setBusy(true)
    try {
      const result = await refresh(kind)
      setFailed(!result.success)
      if (result.success) setFreshAt(result.updatedAt)
    } catch {
      setFailed(true)
    } finally {
      setBusy(false)
    }
  }

  const timeText = updatedAt > 0 ? t(`${spec.prefix}UpdatedAt`, { time: formatUnix(updatedAt) }) : t(`${spec.prefix}NeverUpdated`)
  return (
    <SettingsCustomRow className="flex items-center justify-between gap-3">
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="text-sm text-foreground">{t(`${spec.prefix}Status`, { n: count })}</span>
        <span role={failed ? 'alert' : undefined} className={failed ? 'text-xs text-destructive' : 'text-xs text-muted-foreground'}>
          {failed ? t(`${spec.prefix}UpdateFailed`) : timeText}
        </span>
      </div>
      <Button variant="outline" loading={busy} disabled={busy} onClick={() => void run()} className="shrink-0">
        {busy ? t(`${spec.prefix}Updating`) : t(`${spec.prefix}UpdateNow`)}
      </Button>
    </SettingsCustomRow>
  )
}
