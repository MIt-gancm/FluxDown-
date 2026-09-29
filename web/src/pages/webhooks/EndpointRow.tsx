// 端点行：启用开关 | 名称 + URL·事件 + 健康状态 | 操作。
// 桌面直接展示「编辑 / 测试 / 删除」；移动端收进 `ActionMenu`（底部面板，无悬停依赖）。

import { Ellipsis } from 'lucide-react'
import { useT } from '../../i18n'
import type { WebhookDeliveryDto } from '../../lib/rpc'
import { ActionMenu, Button, Icon, Switch, useIsMobile } from '../../ui'
import type { MenuEntry } from '../../ui'
import { latestDelivery } from './endpoints'
import type { EndpointSpec } from './endpoints'

export function EndpointRow({
  endpoint,
  deliveries,
  disabled,
  testing,
  testBusy,
  onToggle,
  onEdit,
  onTest,
  onDelete,
}: {
  endpoint: EndpointSpec
  deliveries: readonly WebhookDeliveryDto[]
  /** 未连接 daemon：整行只读。 */
  disabled: boolean
  /** 本行正在测试。 */
  testing: boolean
  /** 任意行正在测试（同一时间只跑一个）。 */
  testBusy: boolean
  onToggle: (enabled: boolean) => void
  onEdit: () => void
  onTest: () => void
  onDelete: () => void
}) {
  const t = useT()
  const mobile = useIsMobile()

  const latest = latestDelivery(deliveries, endpoint.id)
  let health: string
  if (!endpoint.enabled) health = t('webhookHealthDisabled')
  else if (!latest) health = t('webhookHealthNone')
  else if (latest.success) health = t('webhookHealthOk', { time: `${latest.latencyMs}ms` })
  else health = t('webhookHealthFail', { detail: latest.error === '' ? `HTTP ${latest.statusCode}` : latest.error })

  const entries: MenuEntry[] = [
    { type: 'item', key: 'edit', label: t('webhookRowEdit'), onSelect: onEdit, disabled },
    { type: 'item', key: 'test', label: t('webhookRowTest'), onSelect: onTest, disabled: disabled || testBusy },
    { type: 'separator', key: 'sep' },
    { type: 'item', key: 'delete', label: t('webhookRowDelete'), onSelect: onDelete, destructive: true, disabled },
  ]

  return (
    <div className="flex w-full min-w-0 items-center gap-2 rounded-md px-2 py-1 hover:bg-row-hover coarse:py-2">
      <Switch checked={endpoint.enabled} disabled={disabled} aria-label={endpoint.name} onCheckedChange={onToggle} />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="truncate text-sm text-foreground">{endpoint.name}</div>
        <div className="truncate text-xs text-muted-foreground">
          {endpoint.url} · {endpoint.events.join(', ')}
        </div>
        <div className="truncate text-xs text-text-tertiary">{health}</div>
      </div>
      {mobile ? (
        <ActionMenu
          title={endpoint.name}
          entries={entries}
          trigger={
            <Button variant="ghost" iconOnly aria-label={t('moreActions')} title={t('moreActions')} loading={testing} className="text-muted-foreground hover:text-foreground">
              <Icon icon={Ellipsis} />
            </Button>
          }
        />
      ) : (
        <div className="flex shrink-0 items-center gap-2">
          <Button disabled={disabled} onClick={onEdit}>
            {t('webhookRowEdit')}
          </Button>
          <Button loading={testing} disabled={disabled || testBusy} onClick={onTest}>
            {t('webhookRowTest')}
          </Button>
          <Button variant="outline" className="text-destructive" disabled={disabled} onClick={onDelete}>
            {t('webhookRowDelete')}
          </Button>
        </div>
      )}
    </div>
  )
}
