// 全局账号 / 互联守卫：入站局域网配对请求确认 + 会话被撤销的一次性提示。挂在 AppShell，任何页面都生效。

import { useCallback, useEffect, useState } from 'react'
import { useT } from '../i18n'
import { rpc, useAgent, useServiceEvents } from '../lib/rpc'
import type { EventFrame, LinkPairingRequestDto } from '../lib/rpc'
import { accountErrorKey } from '../pages/settings/sections/account/errorText'
import { sessionRevokedKey } from '../pages/settings/sections/account/sessionNotice'
import { Button, Dialog, DialogFooter, FieldHint, toast } from '../ui'

const NO_REQUESTS: readonly LinkPairingRequestDto[] = []

function useSecondsLeft(expiresAtUnixMs: number): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    setNow(Date.now())
    const id = setInterval(() => setNow(Date.now()), 500)
    return () => clearInterval(id)
  }, [expiresAtUnixMs])
  return Math.max(0, Math.ceil((expiresAtUnixMs - now) / 1000))
}

function RequestDialog({ request }: { request: LinkPairingRequestDto }) {
  const t = useT()
  const [busy, setBusy] = useState(false)
  const seconds = useSecondsLeft(request.expiresAtUnixMs)

  const decide = async (accept: boolean) => {
    if (busy) return
    setBusy(true)
    try {
      await rpc.agent.link.approve({ sessionId: request.sessionId, accept })
      if (accept) toast.key('localPairingPaired', 'success', { device: request.peerName })
    } catch (error) {
      toast.key(accountErrorKey(error), 'error')
    } finally {
      setBusy(false)
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && void decide(false)}
      modalLocked={busy}
      title={t('localPairingIncomingTitle')}
      description={t('localPairingIncomingDesc', { device: request.peerName })}
      footer={
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={() => void decide(false)}>
            {t('localPairingReject')}
          </Button>
          <Button variant="primary" loading={busy} onClick={() => void decide(true)}>
            {t('localPairingConfirm')}
          </Button>
        </DialogFooter>
      }
    >
      <div className="flex flex-col gap-3 pb-2">
        <FieldHint>{t('localPairingSasHint')}</FieldHint>
        <div className="flex flex-col items-center gap-1 rounded-md bg-muted py-4">
          <span className="tabular font-mono text-display font-semibold tracking-widest text-foreground">{request.sas}</span>
          <span className="text-xs text-muted-foreground">{[request.peerName, request.peerPlatform].filter(Boolean).join(' · ')}</span>
        </div>
        <FieldHint>{t('accountCodeExpireIn', { seconds })}</FieldHint>
      </div>
    </Dialog>
  )
}

/** 入站配对请求：一次只处理队首，处理后自动轮到下一条。 */
function PairingRequestHost() {
  const requests = useAgent((snapshot) => snapshot.linkPairingRequests ?? NO_REQUESTS, NO_REQUESTS)
  const first = requests[0]
  return first ? <RequestDialog key={first.sessionId} request={first} /> : null
}

/** agent 推送的会话撤销通知（主动登出 / 删本设备不会发）：按 reason 一次性提示。 */
function SessionRevokedNotice() {
  useServiceEvents(
    useCallback((frame: EventFrame) => {
      const { event } = frame
      if (event.service === 'agent' && event.event.type === 'sessionRevoked') {
        toast.key(sessionRevokedKey(event.event.data), 'warning')
      }
    }, []),
  )
  return null
}

export function AccountGuards() {
  return (
    <>
      <PairingRequestHost />
      <SessionRevokedNotice />
    </>
  )
}
