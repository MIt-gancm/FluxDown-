// 投递记录分组：最近投递（实时，来自 `webhooksChanged` 事件）+ 模拟一次完成事件 + 清空。

import { useMemo, useState } from 'react'
import { useT } from '../../i18n'
import { cn } from '../../lib/cn'
import { errorMessage, rpc, useDaemon } from '../../lib/rpc'
import { Button } from '../../ui'
import { NO_DELIVERIES, selectDeliveries } from './endpoints'
import { WebhookGroup } from './WebhookGroup'
import type { RunWrite } from './write'

/** 页面只渲染最近这么多条（GPUI 同）。 */
const VISIBLE_DELIVERIES = 50

const pad = (value: number) => String(value).padStart(2, '0')

/** Unix 毫秒 → `YYYY-MM-DD HH:mm`（本地时区，与 GPUI `format_unix` 一致）。 */
function formatTimestamp(ms: number): string {
  const date = new Date(ms)
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`
}

export function DeliveryLogGroup({ disabled, runWrite }: { disabled: boolean; runWrite: RunWrite }) {
  const t = useT()
  const deliveries = useDaemon(selectDeliveries, NO_DELIVERIES)
  const [simulating, setSimulating] = useState(false)
  const [simulateText, setSimulateText] = useState<string | null>(null)

  // 引擎按新→旧下发；这里显式按时间倒序，不依赖下发顺序。
  const visible = useMemo(
    () => [...deliveries].sort((a, b) => b.timestampMs - a.timestampMs).slice(0, VISIBLE_DELIVERIES),
    [deliveries],
  )

  const simulate = async () => {
    if (simulating) return
    setSimulating(true)
    setSimulateText(t('webhookLogPending'))
    try {
      const response = await rpc.daemon.webhook.simulate()
      setSimulateText(
        response.dispatched === 0 ? t('webhookSimulateNoTarget') : t('webhookSimulateDispatched', { n: response.dispatched }),
      )
    } catch (error) {
      setSimulateText(t('webhookTestFail', { error: errorMessage(error) }))
    } finally {
      setSimulating(false)
    }
  }

  const clear = () =>
    runWrite(async () => {
      await rpc.daemon.webhook.clearDeliveries()
    })

  return (
    <WebhookGroup title={t('webhookDeliveryLog')} subtitle={t('webhookLogSubtitle')}>
      <div className="flex w-full flex-col gap-1">
        {simulateText ? <div className="px-2 text-xs text-muted-foreground">{simulateText}</div> : null}
        {visible.length === 0 ? <div className="px-2 py-1 text-xs text-muted-foreground">{t('webhookLogEmpty')}</div> : null}
        {visible.map((delivery) => {
          const status = delivery.success
            ? `${delivery.statusCode} · ${delivery.latencyMs}ms`
            : delivery.error === ''
              ? `HTTP ${delivery.statusCode}`
              : delivery.error
          return (
            <div
              key={delivery.deliveryId}
              className="flex w-full min-w-0 items-center justify-between gap-3 border-b border-hairline px-2 py-1 narrow:flex-col narrow:items-stretch narrow:gap-0.5"
            >
              <div className="flex min-w-0 flex-col gap-0.5">
                <div className="truncate text-sm text-foreground">
                  {delivery.endpointName} · {delivery.event}
                </div>
                <div className="truncate text-xs text-muted-foreground" title={status}>
                  {status} · {t('webhookAttempts', { n: delivery.attempts })}
                </div>
              </div>
              <div className={cn('shrink-0 text-xs tabular', delivery.success ? 'text-muted-foreground' : 'text-destructive')}>
                {formatTimestamp(delivery.timestampMs)}
              </div>
            </div>
          )
        })}
        <div className="flex w-full flex-wrap justify-end gap-2 pt-2 narrow:[&>*]:flex-1">
          <Button loading={simulating} disabled={disabled || simulating} onClick={() => void simulate()}>
            {simulating ? t('webhookLogPending') : t('webhookLogSimulate')}
          </Button>
          <Button disabled={disabled || deliveries.length === 0} onClick={() => void clear()}>
            {t('webhookLogClear')}
          </Button>
        </div>
      </div>
    </WebhookGroup>
  )
}
