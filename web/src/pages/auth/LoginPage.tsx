// 登录页：访问密钥 + 「记住此设备」。同源部署，无服务器地址字段。
// 流程：保存密钥 → ConnectionController 启动 /rpc 连接 → 观察连接阶段：
//   ready → 进入应用；unauthorized → 提示密钥无效；reconnecting → 服务不可达。
// 服务尚未初始化（无访问密钥）→ 跳转首次运行向导 /setup。

import { useNavigate } from '@tanstack/react-router'
import { Eye, EyeOff } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useT } from '../../i18n'
import { clearToken, fetchSetupStatus, saveToken } from '../../lib/access'
import { stopConnection } from '../../lib/rpc/client'
import { useConnection } from '../../lib/rpc/hooks'
import { validateAccessKey } from '../../lib/token-policy'
import { Button, CheckRow, FieldError, FormField, Form, Icon, Input } from '../../ui'
import { AuthCard } from './AuthCard'

export function LoginPage() {
  const t = useT()
  const navigate = useNavigate()
  const connection = useConnection()
  const [key, setKey] = useState('')
  const [remember, setRemember] = useState(true)
  const [reveal, setReveal] = useState(false)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // 服务需要初始化 → 向导。
  useEffect(() => {
    const controller = new AbortController()
    fetchSetupStatus(controller.signal)
      .then((status) => {
        if (status.setupRequired) void navigate({ to: '/setup', replace: true })
      })
      .catch(() => {
        // 探针失败不阻塞登录（可能是网络问题，提交时会给出明确错误）。
      })
    return () => controller.abort()
  }, [navigate])

  // 观察提交后的连接结果。
  useEffect(() => {
    if (!submitting) return
    switch (connection.phase) {
      case 'ready':
        setSubmitting(false)
        void navigate({ to: '/', replace: true })
        break
      case 'unauthorized':
        clearToken()
        setSubmitting(false)
        setError(t('webLoginInvalidKey'))
        break
      case 'setupRequired':
        clearToken()
        setSubmitting(false)
        void navigate({ to: '/setup', replace: true })
        break
      case 'reconnecting':
      case 'stopped':
      case 'incompatible':
        clearToken()
        stopConnection()
        setSubmitting(false)
        setError(connection.phase === 'incompatible' ? t('webIncompatible') : t('webLoginUnreachable'))
        break
      default:
        break
    }
  }, [submitting, connection.phase, navigate, t])

  const submit = () => {
    const token = key.trim()
    if (token === '' || submitting) return
    setError(null)
    if (validateAccessKey(token) !== null) {
      // 与服务端同策略：不满足策略的密钥必然无效，无需往返。
      setError(t('webLoginInvalidKey'))
      return
    }
    setSubmitting(true)
    saveToken(token, remember)
  }

  return (
    <AuthCard title={t('webLoginTitle')} subtitle={t('webLoginSubtitle')}>
      <Form onSubmit={submit}>
        <FormField label={t('webAccessKey')} htmlFor="access-key">
          <Input
            id="access-key"
            type={reveal ? 'text' : 'password'}
            autoComplete="current-password"
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            autoFocus
            value={key}
            invalid={error !== null}
            placeholder={t('webAccessKeyPlaceholder')}
            onChange={(event) => setKey(event.target.value)}
            trailing={
              <Button
                variant="ghost"
                iconOnly
                className="text-muted-foreground hover:text-foreground"
                aria-label={reveal ? t('webHideKey') : t('webShowKey')}
                onClick={() => setReveal((value) => !value)}
              >
                <Icon icon={reveal ? EyeOff : Eye} size="md" />
              </Button>
            }
          />
        </FormField>
        <CheckRow checked={remember} onCheckedChange={setRemember}>
          {t('webRememberDevice')}
        </CheckRow>
        {error ? <FieldError>{error}</FieldError> : null}
        <Button type="submit" variant="primary" loading={submitting} disabled={key.trim() === ''} className="w-full">
          {t('webSignIn')}
        </Button>
      </Form>
    </AuthCard>
  )
}
