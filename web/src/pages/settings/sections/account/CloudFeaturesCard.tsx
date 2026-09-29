// 云功能：配置同步（agent.sync.enable/disable/now，状态来自快照）+ 多设备协同在线数。

import { Network, RefreshCw } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { CloudDevice, SyncStatusDto } from '../../../../lib/rpc'
import { Badge, Button, Card, Icon, Switch, Tooltip, toast } from '../../../../ui'
import { accountErrorKey } from './errorText'

function RowIcon({ icon }: { icon: LucideIcon }) {
  return (
    <div className="flex size-control shrink-0 items-center justify-center rounded-md bg-muted text-muted-foreground coarse:size-11">
      <Icon icon={icon} />
    </div>
  )
}

export function CloudFeaturesCard({
  loggedIn,
  sync,
  devices,
  disabled,
}: {
  loggedIn: boolean
  sync: SyncStatusDto
  devices: readonly CloudDevice[]
  disabled: boolean
}) {
  const t = useT()
  const active = loggedIn && sync.enabled
  const subtitle = !active
    ? t('cloudSyncDesc')
    : sync.lastError
      ? t('cloudSyncStatusError', { reason: sync.lastError })
      : sync.dirtyKeys.length > 0
        ? t('cloudSyncStatusSyncing')
        : t('cloudSyncStatusSynced')
  const online = devices.filter((device) => device.isOnline).length

  const run = async (action: () => Promise<unknown>) => {
    try {
      await action()
    } catch (error) {
      toast.key(accountErrorKey(error), 'error')
    }
  }

  const toggle = (checked: boolean) => void run(() => (checked ? rpc.agent.sync.enable() : rpc.agent.sync.disable()))

  return (
    <section className="flex flex-col gap-2">
      <div className="flex flex-col gap-0.5">
        <div className="text-sm font-medium text-foreground">{t('accountGroupCloudFeatures')}</div>
        <div className="text-xs text-muted-foreground">{t('accountCloudFeaturesDesc')}</div>
      </div>
      <Card className="flex w-full flex-col [&>*+*]:border-t [&>*+*]:border-hairline">
        <div className="flex items-center gap-3 px-3 py-2">
          <RowIcon icon={RefreshCw} />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="text-sm font-medium text-foreground">{t('cloudSyncTitle')}</div>
            <div className="text-xs text-muted-foreground">{subtitle}</div>
          </div>
          {active ? (
            <Button disabled={disabled} onClick={() => void run(() => rpc.agent.sync.now())}>
              {t('cloudSyncNow')}
            </Button>
          ) : null}
          <Tooltip content={loggedIn ? null : t('cloudSyncLoginRequired')}>
            <span>
              <Switch checked={sync.enabled} disabled={disabled || !loggedIn} onCheckedChange={toggle} aria-label={t('cloudSyncTitle')} />
            </span>
          </Tooltip>
        </div>
        <div className="flex items-center gap-3 px-3 py-2">
          <RowIcon icon={Network} />
          <div className="flex min-w-0 flex-1 flex-col gap-0.5">
            <div className="text-sm font-medium text-foreground">{t('multiDeviceTitle')}</div>
            <div className="text-xs text-muted-foreground">{t('multiDeviceDesc')}</div>
          </div>
          {loggedIn ? <Badge>{t('devicesOnlineCount', { count: online })}</Badge> : null}
        </div>
      </Card>
    </section>
  )
}
