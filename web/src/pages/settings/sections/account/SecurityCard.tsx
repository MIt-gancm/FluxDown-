// 账号与安全：邮箱行 + 修改邮箱（Flutter `/me/email` 多步验证：原邮箱验证码 → 新邮箱验证码）。

import { useEffect, useRef, useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { AgentSessionDto } from '../../../../lib/rpc'
import { Button, Card, ConfirmFooter, Dialog, FieldError, FieldHint, Form, FormField, Input, toast } from '../../../../ui'
import { accountErrorKey } from './errorText'
import { useCountdown } from './useCountdown'

const EMAIL_PATTERN = /^[^\s@]+@[^\s@]+\.[^\s@]+$/

function EmailChangeDialog({ current, onClose }: { current: string; onClose: () => void }) {
  const t = useT()
  const [oldCode, setOldCode] = useState('')
  const [newEmail, setNewEmail] = useState('')
  const [newCode, setNewCode] = useState('')
  const [step, setStep] = useState<'old' | 'new'>('old')
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const oldCountdown = useCountdown()
  const newCountdown = useCountdown()
  const sent = useRef(false)

  // 打开即向原邮箱发码（GPUI/Flutter 同款）。
  useEffect(() => {
    if (sent.current) return
    sent.current = true
    rpc.agent.profile
      .sendEmailCode()
      .then((result) => oldCountdown.start(result.ttlSeconds))
      .catch((error: unknown) => setErrorKey(accountErrorKey(error)))
  }, [oldCountdown])

  const email = newEmail.trim()
  const emailError = email === '' ? undefined : !EMAIL_PATTERN.test(email) ? t('accountEmailChangeInvalid') : email.toLowerCase() === current.toLowerCase() ? t('accountEmailChangeSame') : undefined

  const guarded = async (action: () => Promise<void>) => {
    if (busy) return
    setBusy(true)
    setErrorKey(null)
    try {
      await action()
    } catch (error) {
      setErrorKey(accountErrorKey(error, 'code'))
    } finally {
      setBusy(false)
    }
  }

  const sendNewCode = () =>
    guarded(async () => {
      const result = await rpc.agent.profile.sendNewEmailCode({ email, code: oldCode.trim() })
      newCountdown.start(result.ttlSeconds)
      setStep('new')
    })

  const confirm = () =>
    guarded(async () => {
      await rpc.agent.profile.changeEmail({ email, oldCode: oldCode.trim(), newCode: newCode.trim() })
      toast.key('accountEmailChangeSuccess', 'success')
      onClose()
    })

  const canNext = oldCode.trim() !== '' && email !== '' && emailError === undefined
  const canConfirm = newCode.trim() !== ''

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !busy && onClose()}
      title={t('accountEmailChangeTitle')}
      modalLocked={busy}
      footer={
        <ConfirmFooter
          okLabel={step === 'old' ? t('accountEmailChangeSendNewCode') : t('confirm')}
          onCancel={onClose}
          onOk={() => void (step === 'old' ? sendNewCode() : confirm())}
          okDisabled={step === 'old' ? !canNext : !canConfirm}
          loading={busy}
        />
      }
    >
      <Form onSubmit={() => void (step === 'old' ? canNext && sendNewCode() : canConfirm && confirm())}>
        {step === 'old' ? (
          <>
            <FieldHint>{t('accountEmailChangeOldSubtitle', { email: current })}</FieldHint>
            <FormField label={t('accountEmailChangeOldCodePlaceholder')} htmlFor="account-email-old-code" hint={t('accountEmailChangeOldCodeHint')}>
              <Input id="account-email-old-code" value={oldCode} onChange={(event) => setOldCode(event.target.value)} inputMode="numeric" autoComplete="one-time-code" autoFocus />
            </FormField>
            <FormField label={t('accountEmailChangeNewPlaceholder')} htmlFor="account-email-new" {...(emailError ? { error: emailError } : {})}>
              <Input id="account-email-new" type="email" value={newEmail} onChange={(event) => setNewEmail(event.target.value)} invalid={emailError !== undefined} autoCapitalize="none" />
            </FormField>
            <div className="text-xs text-muted-foreground tabular">{oldCountdown.remaining > 0 ? t('accountCodeExpireIn', { seconds: oldCountdown.remaining }) : ''}</div>
          </>
        ) : (
          <>
            <FieldHint>{t('accountEmailChangeCodeSubtitle', { email })}</FieldHint>
            <FormField label={t('accountFieldCode')} htmlFor="account-email-new-code">
              <Input id="account-email-new-code" value={newCode} onChange={(event) => setNewCode(event.target.value)} placeholder={t('accountCodePlaceholder')} inputMode="numeric" autoComplete="one-time-code" autoFocus />
            </FormField>
            <div className="text-xs text-muted-foreground tabular">{newCountdown.remaining > 0 ? t('accountCodeExpireIn', { seconds: newCountdown.remaining }) : ''}</div>
          </>
        )}
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}

export function SecurityCard({ session, disabled }: { session: AgentSessionDto; disabled: boolean }) {
  const t = useT()
  const [open, setOpen] = useState(false)
  return (
    <section className="flex flex-col gap-1">
      <div className="flex flex-col gap-0.5">
        <div className="text-sm font-medium text-foreground">{t('accountSecurityGroup')}</div>
        <div className="text-xs text-muted-foreground">{t('accountSecurityGroupDesc')}</div>
      </div>
      <Card className="mt-1 flex w-full flex-wrap items-center justify-between gap-x-3 gap-y-2 p-3">
        <div className="text-sm font-medium text-foreground">{t('accountEmailPlaceholder')}</div>
        <div className="flex min-w-0 items-center gap-2">
          <span className="min-w-0 truncate text-sm text-muted-foreground">{session.user.email}</span>
          <Button disabled={disabled} onClick={() => setOpen(true)}>
            {t('accountEmailChangeTitle')}
          </Button>
        </div>
      </Card>
      {open ? <EmailChangeDialog current={session.user.email} onClose={() => setOpen(false)} /> : null}
    </section>
  )
}

