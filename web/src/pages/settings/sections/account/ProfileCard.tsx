// 已登录资料卡（GPUI profile.rs）：头像 + 昵称/套餐徽标 + Origin ID 胶囊 + 刷新 + 退出登录；
// 额外提供昵称 / Origin ID 修改（Flutter 同款一次性流程，走 agent.profile.*）。

import { Check, Copy, Pencil, RefreshCw } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useT } from '../../../../i18n'
import { copyText } from '../../../../lib/copy'
import { rpc } from '../../../../lib/rpc'
import type { AgentSessionDto } from '../../../../lib/rpc'
import { Button, Card, ConfirmFooter, Dialog, FieldError, FieldHint, Form, FormField, Icon, Input, Tooltip, toast } from '../../../../ui'
import { accountErrorKey } from './errorText'
import { PlanBadge } from './PlanBadge'

function displayName(session: AgentSessionDto): string {
  const nickname = session.user.nickname.trim()
  if (nickname) return nickname
  return session.user.email.split('@')[0] ?? session.user.email
}

export function NicknameDialog({ current, onClose }: { current: string; onClose: () => void }) {
  const t = useT()
  const [value, setValue] = useState(current)
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const trimmed = value.trim()
  const invalid = trimmed.length < 1 || [...trimmed].length > 32

  const submit = async () => {
    if (busy || invalid) return
    setBusy(true)
    setErrorKey(null)
    try {
      await rpc.agent.profile.changeNickname({ nickname: trimmed })
      toast.key('accountNicknameEditSuccess', 'success')
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
      title={t('accountNicknameEditTitle')}
      modalLocked={busy}
      footer={<ConfirmFooter okLabel={t('confirm')} onCancel={onClose} onOk={() => void submit()} okDisabled={invalid} loading={busy} />}
    >
      <Form onSubmit={() => void submit()}>
        <FormField label={t('accountFieldNickname')} htmlFor="account-nickname" {...(invalid && value !== '' ? { error: t('accountNicknameEditInvalid') } : {})}>
          <Input id="account-nickname" value={value} onChange={(event) => setValue(event.target.value)} invalid={invalid && value !== ''} autoFocus />
        </FormField>
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}

type OriginCheck = 'idle' | 'checking' | 'available' | 'taken' | 'invalid'

export function OriginIdDialog({ onClose }: { onClose: () => void }) {
  const t = useT()
  const [value, setValue] = useState('')
  const [check, setCheck] = useState<OriginCheck>('idle')
  const [busy, setBusy] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)

  const parsed = /^\d+$/.test(value.trim()) ? Number(value.trim()) : Number.NaN
  const valid = Number.isSafeInteger(parsed) && parsed >= 10000

  useEffect(() => {
    if (value.trim() === '') {
      setCheck('idle')
      return
    }
    if (!valid) {
      setCheck('invalid')
      return
    }
    setCheck('checking')
    let cancelled = false
    const timer = window.setTimeout(() => {
      rpc.agent.profile
        .checkOriginId({ value: parsed })
        .then((result) => {
          if (cancelled) return
          setCheck(result.available ? 'available' : result.reason === 'invalid' ? 'invalid' : 'taken')
        })
        .catch(() => {
          if (!cancelled) setCheck('idle')
        })
    }, 400)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
  }, [value, valid, parsed])

  const roll = async () => {
    try {
      const { originId } = await rpc.agent.profile.randomOriginId()
      setValue(String(originId))
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    }
  }

  const submit = async () => {
    if (busy || !valid || check === 'taken' || check === 'invalid') return
    setBusy(true)
    setErrorKey(null)
    try {
      await rpc.agent.profile.changeOriginId({ originId: parsed })
      toast.key('accountOriginIdEditSuccess', 'success')
      onClose()
    } catch (error) {
      setBusy(false)
      // agent 没有 Origin ID 专属 reason（云端 code 落入通用映射），「已被占用 / 无权限」由提交前的
      // checkOriginId 判定，这里只按 reason / code 显示通用文案。
      setErrorKey(accountErrorKey(error))
    }
  }

  const fieldError = check === 'invalid' ? t('accountOriginIdInvalid') : check === 'taken' ? t('accountOriginIdErrorTaken') : undefined

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && !busy && onClose()}
      title={t('accountOriginIdEditTitle')}
      description={t('accountOriginIdEditDesc')}
      modalLocked={busy}
      footer={
        <ConfirmFooter
          okLabel={t('accountOriginIdEditConfirm')}
          onCancel={onClose}
          onOk={() => void submit()}
          okDisabled={!valid || check === 'taken' || check === 'invalid' || check === 'checking'}
          loading={busy}
        />
      }
    >
      <Form onSubmit={() => void submit()}>
        <FormField label={t('accountOriginIdEditPlaceholder')} htmlFor="account-origin-id" {...(fieldError ? { error: fieldError } : {})}>
          <div className="flex items-center gap-2">
            <Input
              id="account-origin-id"
              value={value}
              inputMode="numeric"
              onChange={(event) => setValue(event.target.value)}
              invalid={fieldError !== undefined}
              autoFocus
            />
            <Button onClick={() => void roll()} disabled={busy}>
              {t('accountOriginIdEditRoll')}
            </Button>
          </div>
        </FormField>
        {check === 'available' ? <FieldHint className="text-success">#{parsed}</FieldHint> : null}
        <FieldHint className="text-warning">{t('accountOriginIdEditWarning')}</FieldHint>
        {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
        <button type="submit" className="hidden" />
      </Form>
    </Dialog>
  )
}

export function ProfileCard({ session, disabled }: { session: AgentSessionDto; disabled: boolean }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const [refreshing, setRefreshing] = useState(false)
  const [errorKey, setErrorKey] = useState<string | null>(null)
  const [editing, setEditing] = useState<'nickname' | 'originId' | null>(null)

  useEffect(() => {
    if (!copied) return
    const timer = window.setTimeout(() => setCopied(false), 2000)
    return () => window.clearTimeout(timer)
  }, [copied])

  const name = displayName(session)
  const initial = [...name.trim()][0]?.toUpperCase()
  const originId = session.user.originId

  const refresh = async () => {
    if (refreshing) return
    setRefreshing(true)
    setErrorKey(null)
    try {
      await rpc.agent.auth.refreshProfile()
      await rpc.agent.device.list()
      toast.key('accountCloudRefreshDone', 'success')
    } catch (error) {
      setErrorKey(accountErrorKey(error))
      toast.key(accountErrorKey(error), 'error')
    } finally {
      setRefreshing(false)
    }
  }

  const logout = async () => {
    setErrorKey(null)
    try {
      await rpc.agent.auth.logout()
    } catch (error) {
      setErrorKey(accountErrorKey(error))
    }
  }

  return (
    <Card className="flex w-full flex-col gap-3 p-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="flex size-12 shrink-0 items-center justify-center rounded-full bg-accent-text/12 text-title font-semibold text-accent-text">
          {initial ?? '·'}
        </div>
        <div className="flex min-w-0 flex-1 basis-48 flex-col items-start gap-1">
          <div className="flex max-w-full items-center gap-2">
            <span className="min-w-0 truncate text-title font-semibold text-foreground">{name}</span>
            {session.currentPlan ? <PlanBadge plan={session.currentPlan} ordinal={session.user.membershipOrdinal} /> : null}
            <Tooltip content={t('accountNicknameEditTooltip')}>
              <Button variant="ghost" iconOnly className="text-muted-foreground hover:text-foreground" aria-label={t('accountNicknameEditTooltip')} disabled={disabled} onClick={() => setEditing('nickname')}>
                <Icon icon={Pencil} size="md" />
              </Button>
            </Tooltip>
          </div>
          <div className="flex items-center gap-1">
            {originId !== null ? (
              <button
                type="button"
                className="inline-flex items-center gap-1 rounded-full bg-accent-text/12 px-2 py-0.5 text-xs font-medium text-accent-text tabular coarse:min-h-touch"
                onClick={() => {
                  copyText(String(originId))
                  setCopied(true)
                }}
              >
                {copied ? t('accountOriginIdCopied') : `#${originId}`}
                <Icon icon={copied ? Check : Copy} />
              </button>
            ) : (
              <span className="inline-flex items-center rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">#—</span>
            )}
            {originId !== null && !session.user.originIdChanged ? (
              <Tooltip content={t('accountOriginIdEditTooltip')}>
                <Button variant="ghost" iconOnly className="text-muted-foreground hover:text-foreground" aria-label={t('accountOriginIdEditTooltip')} disabled={disabled} onClick={() => setEditing('originId')}>
                  <Icon icon={Pencil} size="md" />
                </Button>
              </Tooltip>
            ) : null}
          </div>
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <Tooltip content={t('accountCloudRefresh')}>
            <Button variant="ghost" iconOnly className="text-muted-foreground hover:text-foreground" aria-label={t('accountCloudRefresh')} disabled={disabled} loading={refreshing} onClick={() => void refresh()}>
              <Icon icon={RefreshCw} />
            </Button>
          </Tooltip>
          <Button disabled={disabled} onClick={() => void logout()}>
            {t('accountLogout')}
          </Button>
        </div>
      </div>
      {errorKey ? <FieldError>{t(errorKey)}</FieldError> : null}
      {editing === 'nickname' ? <NicknameDialog current={session.user.nickname} onClose={() => setEditing(null)} /> : null}
      {editing === 'originId' ? <OriginIdDialog onClose={() => setEditing(null)} /> : null}
    </Card>
  )
}
