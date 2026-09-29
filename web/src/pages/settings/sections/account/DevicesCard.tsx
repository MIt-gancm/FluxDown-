// 受信任设备（GPUI devices.rs）：列表 + 重试；Web 额外提供重命名 / 删除（agent.device.*）。

import { Pencil, RefreshCw, Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { CloudDevice } from '../../../../lib/rpc'
import { Badge, Button, Card, ConfirmFooter, Dialog, FieldError, Form, FormField, Icon, Input, confirmDialog, toast } from '../../../../ui'
import { accountErrorKey } from './errorText'

const PLATFORM_KEYS: Record<string, string> = {
  windows: 'accountDevicePlatformWindows',
  macos: 'accountDevicePlatformMacos',
  linux: 'accountDevicePlatformLinux',
  android: 'accountDevicePlatformAndroid',
  ios: 'accountDevicePlatformIos',
  web: 'accountDevicePlatformWeb',
}

function RenameDialog({ device, onClose }: { device: CloudDevice; onClose: () => void }) {
  const t = useT()
  const [name, setName] = useState(device.name)
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const trimmed = name.trim()
  const invalid = trimmed.length < 1 || [...trimmed].length > 64

  const submit = async () => {
    if (busy || invalid) return
    setBusy(true)
    setErrorKey(null)
    try {
      await rpc.agent.device.rename({ id: device.id, name: trimmed })
      onClose()
    } catch (error) {
      setErrorKey(accountErrorKey(error))
      setBusy(false)
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !busy && onClose()}
      title={t('accountDeviceRenameTitle')}
      modalLocked={busy}
      footer={<ConfirmFooter okLabel={t('confirm')} onCancel={onClose} onOk={() => void submit()} okDisabled={invalid} loading={busy} />}
    >
      <Form onSubmit={() => void submit()}>
        <FormField label={t('accountDeviceRenameTitle')} htmlFor="account-device-name" {...(invalid && name !== '' ? { error: t('accountDeviceRenameInvalid') } : {})}>
          <Input id="account-device-name" value={name} onChange={(event) => setName(event.target.value)} invalid={invalid && name !== ''} autoFocus />
        </FormField>
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}

function DeviceRow({ device, disabled }: { device: CloudDevice; disabled: boolean }) {
  const t = useT()
  const [renaming, setRenaming] = useState(false)
  const platformKey = device.platform ? PLATFORM_KEYS[device.platform.toLowerCase()] : undefined
  const platform = platformKey ? t(platformKey) : (device.platform ?? '')
  const lastSeen = Number.isNaN(Date.parse(device.lastSeenAt)) ? '' : new Date(device.lastSeenAt).toLocaleString()
  const meta = [platform, device.appVersion, lastSeen].filter((part): part is string => Boolean(part)).join(' · ')

  const remove = async () => {
    const description = device.isCurrent
      ? `${t('accountDeviceDeleteConfirmDesc')}\n${t('accountDeviceDeleteCurrentWarning')}`
      : t('accountDeviceDeleteConfirmDesc')
    const ok = await confirmDialog({
      title: t('accountDeviceDeleteConfirmTitle'),
      description,
      intent: 'destructive',
      okLabel: t('accountDeviceDeleteAction'),
    })
    if (!ok) return
    try {
      await rpc.agent.device.delete({ id: device.id })
    } catch (error) {
      toast.key(accountErrorKey(error), 'error')
    }
  }

  return (
    <div className="flex items-center gap-2 px-3 py-2 coarse:min-h-touch">
      <span className={`size-2 shrink-0 rounded-full ${device.isOnline ? 'bg-success' : 'bg-text-tertiary/50'}`} aria-hidden />
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 truncate text-sm font-medium text-foreground">{device.name}</span>
          {device.isCurrent ? <Badge tone="accent">{t('accountDeviceCurrent')}</Badge> : null}
        </div>
        {meta ? <div className="truncate text-xs text-muted-foreground">{meta}</div> : null}
      </div>
      <div className="flex shrink-0 items-center gap-0.5">
        <Button variant="ghost" iconOnly aria-label={t('accountDeviceRenameTitle')} title={t('accountDeviceRenameTitle')} className="text-muted-foreground hover:text-foreground" disabled={disabled} onClick={() => setRenaming(true)}>
          <Icon icon={Pencil} size="md" />
        </Button>
        <Button
          variant="ghost"
          iconOnly
          aria-label={t('accountDeviceDeleteAction')}
          title={t('accountDeviceDeleteAction')}
          className="text-muted-foreground hover:text-destructive"
          disabled={disabled}
          onClick={() => void remove()}
        >
          <Icon icon={Trash2} size="md" />
        </Button>
      </div>
      {renaming ? <RenameDialog device={device} onClose={() => setRenaming(false)} /> : null}
    </div>
  )
}

export function DevicesCard({ devices, disabled }: { devices: readonly CloudDevice[]; disabled: boolean }) {
  const t = useT()
  const [refreshing, setRefreshing] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)

  const refresh = async () => {
    if (refreshing) return
    setRefreshing(true)
    setErrorKey(null)
    try {
      await rpc.agent.device.list()
      toast.key('accountCloudRefreshDone', 'success')
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    } finally {
      setRefreshing(false)
    }
  }

  return (
    <section className="flex flex-col gap-2">
      <div className="flex items-center justify-between gap-2">
        <div className="flex min-w-0 flex-col gap-0.5">
          <div className="text-sm font-medium text-foreground">{t('accountDevicesTitle')}</div>
          <div className="text-xs text-muted-foreground">{t('accountDevicesDesc')}</div>
        </div>
        <Button icon={RefreshCw} disabled={disabled} loading={refreshing} onClick={() => void refresh()}>
          {t('accountDevicesRetry')}
        </Button>
      </div>
      {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
      <Card className="w-full overflow-hidden [&>*+*]:border-t [&>*+*]:border-hairline">
        {devices.length === 0 ? (
          <div className="flex justify-center p-4 text-xs text-muted-foreground">{t('accountDevicesEmpty')}</div>
        ) : (
          devices.map((device) => <DeviceRow key={device.id} device={device} disabled={disabled} />)
        )}
      </Card>
    </section>
  )
}
