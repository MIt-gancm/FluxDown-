// 登录 / 注册对话框（GPUI `crates/account/src/dialogs/{login,register}.rs`）：
// 第一步账号+密码；服务端返回 `deviceVerificationRequired` 后换成验证码步骤再提交。

import { useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { AgentLoginResult } from '../../../../lib/rpc'
import { ConfirmFooter, Dialog, FieldError, FieldHint, Form, FormField, Input } from '../../../../ui'
import { accountErrorKey } from './errorText'
import type { AccountErrorContext } from './errorText'
import { PasswordInput } from './PasswordInput'
import { useCountdown } from './useCountdown'

function VerifyStep({
  title,
  subtitle,
  notice,
  code,
  onCode,
  remaining,
  onResend,
  busy,
}: {
  title: string
  subtitle: string
  notice?: string
  code: string
  onCode: (value: string) => void
  remaining: number
  onResend?: () => void
  busy: boolean
}) {
  const t = useT()
  return (
    <>
      <div className="flex flex-col gap-0.5">
        <div className="text-sm font-medium text-foreground">{title}</div>
        <FieldHint>{subtitle}</FieldHint>
        {notice ? <FieldHint className="text-warning">{notice}</FieldHint> : null}
      </div>
      <FormField label={t('accountFieldCode')} htmlFor="account-code">
        <Input
          id="account-code"
          value={code}
          onChange={(event) => onCode(event.target.value)}
          placeholder={t('accountCodePlaceholder')}
          inputMode="numeric"
          autoComplete="one-time-code"
          autoFocus
        />
      </FormField>
      <div className="flex items-center justify-between gap-2 text-xs text-muted-foreground">
        <span className="tabular">{remaining > 0 ? t('accountCodeExpireIn', { seconds: remaining }) : ''}</span>
        {onResend ? (
          <button type="button" className="text-accent-text disabled:opacity-50 coarse:min-h-touch" disabled={busy} onClick={onResend}>
            {t('accountResendCode')}
          </button>
        ) : null}
      </div>
    </>
  )
}

/** 提交并解释 AgentLoginResult；返回 `'ok' | 'verify' | 'error'`。 */
async function run(
  action: () => Promise<AgentLoginResult>,
  context: AccountErrorContext,
  setError: (key: string | null) => void,
  setBusy: (busy: boolean) => void,
): Promise<{ kind: 'ok' } | { kind: 'verify'; ttlSeconds: number; willReplaceDevices: boolean } | { kind: 'error' }> {
  setBusy(true)
  setError(null)
  try {
    const result = await action()
    if (result.status === 'ok') return { kind: 'ok' }
    return { kind: 'verify', ttlSeconds: result.ttlSeconds, willReplaceDevices: result.willReplaceDevices }
  } catch (error) {
    setError(accountErrorKey(error, context))
    return { kind: 'error' }
  } finally {
    setBusy(false)
  }
}

export function LoginDialog({ onClose }: { onClose: () => void }) {
  const t = useT()
  const [account, setAccount] = useState('')
  const [password, setPassword] = useState('')
  const [code, setCode] = useState('')
  const [verify, setVerify] = useState(false)
  const [replace, setReplace] = useState(false)
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const countdown = useCountdown()

  const canSubmit = account.trim() !== '' && password !== '' && (!verify || code.trim() !== '')

  const submit = async () => {
    if (busy || !canSubmit) return
    const trimmed = account.trim()
    const outcome = await run(
      () => (verify ? rpc.agent.auth.loginVerify({ account: trimmed, password, code: code.trim() }) : rpc.agent.auth.login({ account: trimmed, password })),
      verify ? 'code' : 'login',
      setErrorKey,
      setBusy,
    )
    if (outcome.kind === 'ok') onClose()
    else if (outcome.kind === 'verify') {
      setVerify(true)
      setReplace(outcome.willReplaceDevices)
      countdown.start(outcome.ttlSeconds)
    }
  }

  const resend = async () => {
    const outcome = await run(() => rpc.agent.auth.login({ account: account.trim(), password }), 'login', setErrorKey, setBusy)
    if (outcome.kind === 'verify') countdown.start(outcome.ttlSeconds)
    else if (outcome.kind === 'ok') onClose()
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !busy && onClose()}
      title={t('accountLoginDialogTitle')}
      modalLocked={busy}
      footer={<ConfirmFooter okLabel={verify ? t('confirm') : t('accountLogin')} onCancel={onClose} onOk={() => void submit()} okDisabled={!canSubmit} loading={busy} />}
    >
      <Form onSubmit={() => void submit()}>
        {verify ? (
          <VerifyStep
            title={t('accountDeviceVerifyTitle')}
            subtitle={t('accountDeviceVerifySubtitleGeneric')}
            {...(replace ? { notice: t('accountDeviceVerifyReplacementNotice') } : {})}
            code={code}
            onCode={setCode}
            remaining={countdown.remaining}
            onResend={() => void resend()}
            busy={busy}
          />
        ) : (
          <>
            <FormField label={t('accountFieldAccount')} htmlFor="account-login-account">
              <Input
                id="account-login-account"
                value={account}
                onChange={(event) => setAccount(event.target.value)}
                placeholder={t('accountLoginAccountPlaceholder')}
                autoComplete="username"
                autoCapitalize="none"
                autoFocus
              />
            </FormField>
            <FormField label={t('accountPasswordPlaceholder')} htmlFor="account-login-password">
              <PasswordInput
                id="account-login-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                placeholder={t('accountPasswordPlaceholder')}
                autoComplete="current-password"
              />
            </FormField>
          </>
        )}
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}

export function RegisterDialog({ onClose }: { onClose: () => void }) {
  const t = useT()
  const [email, setEmail] = useState('')
  const [password, setPassword] = useState('')
  const [nickname, setNickname] = useState('')
  const [code, setCode] = useState('')
  const [verify, setVerify] = useState(false)
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const countdown = useCountdown()

  const canSubmit = verify ? code.trim() !== '' : email.trim() !== '' && password !== ''

  const submit = async () => {
    if (busy || !canSubmit) return
    const trimmed = email.trim()
    const outcome = await run(
      () => {
        if (verify) return rpc.agent.auth.registerVerify({ email: trimmed, code: code.trim() })
        const name = nickname.trim()
        return rpc.agent.auth.register(name ? { email: trimmed, password, nickname: name } : { email: trimmed, password })
      },
      verify ? 'code' : 'register',
      setErrorKey,
      setBusy,
    )
    if (outcome.kind === 'ok') onClose()
    else if (outcome.kind === 'verify') {
      setVerify(true)
      countdown.start(outcome.ttlSeconds)
    }
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !busy && onClose()}
      title={t('accountRegisterDialogTitle')}
      modalLocked={busy}
      footer={<ConfirmFooter okLabel={verify ? t('accountVerifySubmit') : t('accountRegister')} onCancel={onClose} onOk={() => void submit()} okDisabled={!canSubmit} loading={busy} />}
    >
      <Form onSubmit={() => void submit()}>
        {verify ? (
          <VerifyStep
            title={t('accountRegisterVerifyTitle')}
            subtitle={t('accountRegisterVerifySubtitle', { email: email.trim() })}
            code={code}
            onCode={setCode}
            remaining={countdown.remaining}
            busy={busy}
          />
        ) : (
          <>
            <FormField label={t('accountEmailPlaceholder')} htmlFor="account-register-email">
              <Input
                id="account-register-email"
                type="email"
                value={email}
                onChange={(event) => setEmail(event.target.value)}
                autoComplete="email"
                autoCapitalize="none"
                autoFocus
              />
            </FormField>
            <FormField label={t('accountPasswordPlaceholder')} htmlFor="account-register-password" hint={t('accountPasswordHint')}>
              <PasswordInput
                id="account-register-password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoComplete="new-password"
              />
            </FormField>
            <FormField label={t('accountFieldNickname')} htmlFor="account-register-nickname">
              <Input id="account-register-nickname" value={nickname} onChange={(event) => setNickname(event.target.value)} placeholder={t('accountNicknamePlaceholder')} />
            </FormField>
          </>
        )}
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}
