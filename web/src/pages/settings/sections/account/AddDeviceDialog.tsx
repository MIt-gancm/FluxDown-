// 添加设备（GPUI add-device 对话框）：账号方式（同账号登录即自动出现）/ 局域网直连配对
// （发现、手动地址含 https、配对码、SAS 确认、本机配对码展示）。

import { Laptop, RefreshCw } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc, useAgent } from '../../../../lib/rpc'
import type { LinkDeviceInfo, LinkDiscoveredPeer, LinkPairBeginResponse, LinkPairingCodeDto } from '../../../../lib/rpc'
import { Button, ConfirmFooter, Dialog, FieldError, FieldHint, Form, FormField, Icon, Input, SegmentedTabs, Spinner, toast } from '../../../../ui'
import { accountErrorKey } from './errorText'
import { isManualAddressValid, isPairingCodeComplete, normalizePairingCode, peerAddress, unpairedPeers } from './pairing'
import { useCountdown } from './useCountdown'

const NO_PEERS: readonly LinkDiscoveredPeer[] = []
const NO_LINKED: readonly LinkDeviceInfo[] = []

type Tab = 'account' | 'local'

function AccountTab() {
  const t = useT()
  const email = useAgent((snapshot) => snapshot.session?.user.email ?? null, null)
  return (
    <div className="flex flex-col gap-3">
      <FieldHint>{t('addDeviceHint')}</FieldHint>
      {email ? (
        <div className="rounded-md bg-muted px-3 py-2 text-sm text-foreground">{t('addDeviceAccountSynced', { account: email })}</div>
      ) : (
        <FieldError>{t('addDeviceLoginRequired')}</FieldError>
      )}
      <FieldHint>{t('addDeviceAccountFooter')}</FieldHint>
    </div>
  )
}

/** 本机配对码：供对端输入；Web 场景的「本机」即服务器。关闭对话框时停止广播。 */
function OwnCodePanel() {
  const t = useT()
  const lanEnabled = useAgent((snapshot) => snapshot.gateway.lanEnabled, false)
  const [own, setOwn] = useState<LinkPairingCodeDto | null>(null)
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const active = useRef(false)
  const expiry = useCountdown()

  useEffect(
    () => () => {
      if (active.current) void rpc.agent.link.stopPairing().catch(() => undefined)
    },
    [],
  )

  const show = async () => {
    setBusy(true)
    setErrorKey(null)
    try {
      const result = await rpc.agent.link.pairingCode()
      active.current = true
      setOwn(result)
      expiry.start(Math.max(0, Math.round((result.expiresAtUnixMs - Date.now()) / 1000)))
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <div className="flex flex-col gap-2 rounded-md border border-hairline p-3">
      <div className="flex items-center justify-between gap-2">
        <div className="text-sm font-medium text-foreground">{t('localPairingMyCodeTitle')}</div>
        <Button loading={busy} onClick={() => void show()}>
          {own ? t('localPairingMyCodeRefresh') : t('localPairingMyCodeShow')}
        </Button>
      </div>
      <FieldHint>{t('localPairingMyCodeHint')}</FieldHint>
      {own ? (
        <>
          <div className="flex items-baseline gap-3">
            <span className="tabular font-mono text-title font-semibold tracking-widest text-foreground">{own.code}</span>
            <span className="tabular text-xs text-muted-foreground">{expiry.remaining > 0 ? t('accountCodeExpireIn', { seconds: expiry.remaining }) : t('localPairingMyCodeExpired')}</span>
          </div>
          {own.addresses.length > 0 ? (
            <div className="flex flex-col gap-0.5 text-xs text-muted-foreground">
              <span>{t('localPairingMyAddresses')}</span>
              {own.addresses.map((address) => (
                <span key={address} className="break-all font-mono text-foreground">
                  {address}
                </span>
              ))}
            </div>
          ) : (
            <FieldError>{lanEnabled ? t('localPairingNoAddress') : t('localPairingLanDisabledHint')}</FieldError>
          )}
        </>
      ) : null}
      {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
    </div>
  )
}

function PeerList({ peers, selected, onSelect }: { peers: readonly LinkDiscoveredPeer[]; selected: string; onSelect: (peer: LinkDiscoveredPeer) => void }) {
  const t = useT()
  if (peers.length === 0) return <FieldHint>{t('localPairingNoDevices')}</FieldHint>
  return (
    <div className="flex flex-col overflow-hidden rounded-md border border-hairline [&>*+*]:border-t [&>*+*]:border-hairline">
      {peers.map((peer) => {
        const address = peerAddress(peer)
        return (
          <button
            key={`${peer.fingerprint ?? ''}@${address}`}
            type="button"
            onClick={() => onSelect(peer)}
            aria-pressed={selected === address}
            className={`flex items-center gap-2 px-3 py-2 text-left text-sm coarse:min-h-touch hover:bg-nav-hover ${selected === address ? 'bg-nav-selected' : ''}`}
          >
            <Icon icon={Laptop} size="md" className="text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate text-foreground">{peer.name}</span>
            <span className="truncate text-xs text-muted-foreground">{address}</span>
          </button>
        )
      })}
    </div>
  )
}

function LocalTab({ onDone }: { onDone: () => void }) {
  const t = useT()
  const discovered = useAgent((snapshot) => snapshot.linkDiscovered ?? NO_PEERS, NO_PEERS)
  const linked = useAgent((snapshot) => snapshot.linkedDevices, NO_LINKED)
  const peers = unpairedPeers(discovered, linked)
  const [address, setAddress] = useState('')
  const [manual, setManual] = useState(false)
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const [pending, setPending] = useState<LinkPairBeginResponse | null>(null)
  const [scanning, setScanning] = useState(false)

  const scan = async () => {
    setScanning(true)
    try {
      await rpc.agent.link.discoverySet({ enabled: false })
      await rpc.agent.link.discoverySet({ enabled: true })
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    } finally {
      setScanning(false)
    }
  }

  // 打开即开始发现；离开（含关闭对话框）即停止。
  useEffect(() => {
    void rpc.agent.link.discoverySet({ enabled: true }).catch((error: unknown) => setErrorKey(accountErrorKey(error)))
    return () => {
      void rpc.agent.link.discoverySet({ enabled: false }).catch(() => undefined)
    }
  }, [])

  // 放弃 SAS 确认（关闭对话框 / 返回）时通知对端，避免悬挂会话。
  const pendingRef = useRef<LinkPairBeginResponse | null>(null)
  pendingRef.current = pending
  useEffect(
    () => () => {
      const token = pendingRef.current?.token
      if (token) void rpc.agent.link.pairFinish({ token, accept: false }).catch(() => undefined)
    },
    [],
  )

  const canBegin = isManualAddressValid(address) && isPairingCodeComplete(code) && !busy

  const begin = async () => {
    if (!canBegin) return
    setBusy(true)
    setErrorKey(null)
    try {
      setPending(await rpc.agent.link.pairBegin({ address: address.trim(), code }))
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    } finally {
      setBusy(false)
    }
  }

  const finish = async (accept: boolean) => {
    if (!pending) return
    const current = pending
    setBusy(true)
    setErrorKey(null)
    try {
      pendingRef.current = null
      const result = await rpc.agent.link.pairFinish({ token: current.token, accept })
      if (accept && result.paired) {
        toast.key('localPairingPaired', 'success', { device: result.device?.name ?? current.peerName })
        onDone()
        return
      }
      setPending(null)
      if (accept) setErrorKey('localPairingFailed')
    } catch (error) {
      setPending(null)
      setErrorKey(accountErrorKey(error))
    } finally {
      setBusy(false)
    }
  }

  if (pending) {
    return (
      <div className="flex flex-col gap-3">
        <div className="text-sm font-medium text-foreground">{t('localPairingSasTitle')}</div>
        <FieldHint>{t('localPairingSasHint')}</FieldHint>
        <div className="flex flex-col items-center gap-1 rounded-md bg-muted py-4">
          <span className="tabular font-mono text-display font-semibold tracking-widest text-foreground">{pending.sas}</span>
          <span className="text-xs text-muted-foreground">{pending.peerName}</span>
        </div>
        {busy ? <FieldHint>{t('localPairingWaitingPeer')}</FieldHint> : null}
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <div className="flex justify-end gap-2">
          <Button variant="outline" disabled={busy} onClick={() => void finish(false)}>
            {t('localPairingReject')}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void finish(true)}>
            {t('localPairingConfirm')}
          </Button>
        </div>
      </div>
    )
  }

  return (
    <Form onSubmit={() => void begin()}>
      <FieldHint>{t('localPairingHint')}</FieldHint>
      <div className="flex items-center justify-between gap-2">
        <span className="flex items-center gap-2 text-xs text-muted-foreground">
          {scanning ? <Spinner /> : null}
          {t('localPairingDiscovering')}
        </span>
        <Button variant="ghost" icon={RefreshCw} disabled={scanning} onClick={() => void scan()}>
          {t('localPairingRetryScan')}
        </Button>
      </div>
      <PeerList
        peers={peers}
        selected={address}
        onSelect={(peer) => {
          setAddress(peerAddress(peer))
          setManual(false)
        }}
      />
      <button type="button" className="self-start text-xs text-accent-text coarse:min-h-touch" onClick={() => setManual((value) => !value)}>
        {t('localPairingManualAddress')}
      </button>
      {manual ? (
        <FormField label={t('localPairingAddressLabel')} htmlFor="pair-address" hint={t('localPairingAddressHint')}>
          <Input id="pair-address" value={address} spellCheck={false} autoCapitalize="none" placeholder="192.168.1.10:17800 / https://host" onChange={(event) => setAddress(event.target.value)} />
        </FormField>
      ) : null}
      <FormField label={t('localPairingCodeLabel')} htmlFor="pair-code" hint={t('localPairingCodeHint')}>
        <Input id="pair-code" value={code} inputMode="numeric" autoComplete="one-time-code" placeholder={t('localPairingCodePlaceholder')} onChange={(event) => setCode(normalizePairingCode(event.target.value))} />
      </FormField>
      {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
      <div className="flex justify-end">
        <Button variant="primary" disabled={!canBegin} loading={busy} onClick={() => void begin()}>
          {t('localPairingConnect')}
        </Button>
      </div>
      <OwnCodePanel />
      <button type="submit" className="hidden" />
    </Form>
  )
}

export function AddDeviceDialog({ onClose }: { onClose: () => void }) {
  const t = useT()
  const [tab, setTab] = useState<Tab>('account')
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t('addDeviceEntry')}
      size="lg"
      footer={<ConfirmFooter okLabel={t('close')} cancelLabel={null} onCancel={onClose} onOk={onClose} />}
    >
      <div className="flex flex-col gap-3 pb-2">
        <SegmentedTabs
          value={tab}
          onValueChange={setTab}
          aria-label={t('addDeviceEntry')}
          items={[
            { value: 'account', label: t('addDeviceTabAccount') },
            { value: 'local', label: t('addDeviceTabLocal') },
          ]}
        />
        {tab === 'account' ? <AccountTab /> : <LocalTab onDone={onClose} />}
      </div>
    </Dialog>
  )
}
