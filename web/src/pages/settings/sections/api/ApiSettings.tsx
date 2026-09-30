// API 服务（GPUI `crates/settings/src/sections/api.rs`）。
// Web 只在 server 模式出现：监听地址/端口由 FLUXDOWN_BIND 决定（LAN 开关无效），所以只读展示访问地址；
// 令牌 = 访问密钥（gateway user token）：查看/复制、重新生成、自定义（须过 token-policy），
// 修改后同步更新本浏览器保存的密钥以保持会话。

import { useNavigate } from '@tanstack/react-router'
import { Copy, Eye, EyeOff, RefreshCw, Check } from 'lucide-react'
import { useCallback, useEffect, useState } from 'react'
import { useT } from '../../../../i18n'
import { copyText } from '../../../../lib/copy'
import { clearToken, updateStoredToken } from '../../../../lib/access'
import { rpc, useAgent } from '../../../../lib/rpc'
import type { GatewayPatchParams, GatewayStatusDto } from '../../../../lib/rpc'
import { ACCESS_KEY_MAX_LEN, ACCESS_KEY_MIN_LEN, validateAccessKey } from '../../../../lib/token-policy'
import type { AccessKeyIssue } from '../../../../lib/token-policy'
import { Button, Icon, Input, Switch, confirmDialog, toast } from '../../../../ui'
import { SettingsCustomRow, SettingsPage, SettingsRow, SettingsSection, rpcErrorText, useSettingsReadOnly } from '../../kit'

type FlagKey = 'takeoverEnabled' | 'jsonrpcEnabled' | 'apiEnabled' | 'mcpEnabled' | 'corsEnabled'

const FEATURES: readonly { flag: FlagKey; titleKey: string; descKey: string }[] = [
  { flag: 'takeoverEnabled', titleKey: 'apiServiceTakeover', descKey: 'apiServiceTakeoverDesc' },
  { flag: 'jsonrpcEnabled', titleKey: 'apiServiceJsonrpc', descKey: 'apiServiceJsonrpcDesc' },
  { flag: 'apiEnabled', titleKey: 'apiServiceApi', descKey: 'apiServiceApiDesc' },
  { flag: 'mcpEnabled', titleKey: 'apiServiceMcp', descKey: 'apiServiceMcpDesc' },
  { flag: 'corsEnabled', titleKey: 'apiServiceCorsAllowAll', descKey: 'apiServiceCorsAllowAllDesc' },
]

const ISSUE_KEYS: Record<AccessKeyIssue, string> = {
  badChars: 'webKeyBadChars',
  tooShort: 'webKeyTooShort',
  tooLong: 'webKeyTooLong',
  needsMix: 'webKeyNeedsMix',
}

/** 复制按钮：点击后 2 秒内图标换成对勾。 */
function CopyButton({ value, label, disabled }: { value: string; label: string; disabled?: boolean }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = window.setTimeout(() => setCopied(false), 2000)
    return () => window.clearTimeout(timer)
  }, [copied])
  return (
    <Button
      iconOnly
      aria-label={copied ? t('apiServiceCopied') : label}
      title={copied ? t('apiServiceCopied') : label}
      disabled={disabled || value === ''}
      onClick={() => {
        copyText(value)
        setCopied(true)
      }}
    >
      <Icon icon={copied ? Check : Copy} />
    </Button>
  )
}

function AddressRow({ titleKey, path, descKey }: { titleKey: string; path: string; descKey?: string }) {
  const t = useT()
  const url = `${window.location.origin}${path}`
  return (
    <SettingsRow title={t(titleKey)} description={descKey ? t(descKey) : undefined}>
      <div className="flex min-w-0 items-center gap-2">
        <code className="min-w-0 flex-1 break-all text-sm tabular text-foreground">{url}</code>
        <CopyButton value={url} label={t('apiServiceCopy')} />
      </div>
    </SettingsRow>
  )
}

function FeatureRow({ gateway, flag, titleKey, descKey }: { gateway: GatewayStatusDto; flag: FlagKey; titleKey: string; descKey: string }) {
  const t = useT()
  const disabled = useSettingsReadOnly()
  const [pending, setPending] = useState<boolean | null>(null)
  const actual = gateway[flag]
  const checked = pending ?? actual
  useEffect(() => setPending(null), [actual])
  return (
    <SettingsRow title={t(titleKey)} description={t(descKey)} compact>
      <Switch
        checked={checked}
        disabled={disabled}
        aria-label={t(titleKey)}
        onCheckedChange={(next) => {
          setPending(next)
          rpc.agent.gateway.patch({ [flag]: next } as GatewayPatchParams).catch((error: unknown) => {
            setPending(null)
            toast.error(rpcErrorText(error, t))
          })
        }}
      />
    </SettingsRow>
  )
}

/** 访问密钥：显示/复制；自定义（回车或「保存」提交）；重新生成（确认）。 */
function AccessKeyRow({ configured }: { configured: boolean }) {
  const t = useT()
  const disabled = useSettingsReadOnly()
  const [token, setToken] = useState<string | null>(null)
  const [draft, setDraft] = useState('')
  const [reveal, setReveal] = useState(false)
  const [busy, setBusy] = useState<'save' | 'regenerate' | null>(null)
  const [issue, setIssue] = useState<AccessKeyIssue | null>(null)

  const load = useCallback(async () => {
    try {
      const { userToken } = await rpc.agent.gateway.revealToken()
      setToken(userToken)
      setDraft(userToken)
    } catch (error) {
      toast.error(rpcErrorText(error, t))
    }
  }, [t])

  useEffect(() => {
    void load()
  }, [load, configured])

  const dirty = token !== null && draft !== token

  const save = async () => {
    if (busy || !dirty) return
    const value = draft.trim()
    const problem = validateAccessKey(value)
    if (problem) {
      setIssue(problem)
      return
    }
    setIssue(null)
    setBusy('save')
    try {
      await rpc.agent.gateway.patch({ userToken: value })
      updateStoredToken(value)
      setToken(value)
      setDraft(value)
    } catch (error) {
      toast.error(rpcErrorText(error, t))
    } finally {
      setBusy(null)
    }
  }

  const regenerate = async () => {
    if (busy) return
    const ok = await confirmDialog({
      title: t('apiServiceTokenGenerate'),
      description: t('apiServiceTokenDesc'),
      okLabel: t('apiServiceTokenGenerate'),
    })
    if (!ok) return
    setIssue(null)
    setBusy('regenerate')
    try {
      await rpc.agent.gateway.patch({ regenerateUserToken: true })
      const { userToken } = await rpc.agent.gateway.revealToken()
      updateStoredToken(userToken)
      setToken(userToken)
      setDraft(userToken)
    } catch (error) {
      toast.error(rpcErrorText(error, t))
    } finally {
      setBusy(null)
    }
  }

  return (
    <SettingsRow title={t('apiServiceToken')} description={t('apiServiceTokenDesc')} vertical>
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center gap-2">
          <Input
            className="min-w-0 flex-1 font-mono"
            type={reveal ? 'text' : 'password'}
            value={draft}
            invalid={issue !== null}
            placeholder={t('proxyNotConfigured')}
            aria-label={t('apiServiceToken')}
            autoComplete="off"
            spellCheck={false}
            disabled={disabled || busy !== null || token === null}
            onChange={(event) => {
              setDraft(event.target.value)
              setIssue(null)
            }}
            onKeyDown={(event) => {
              if (event.key === 'Enter') void save()
            }}
          />
          <Button iconOnly aria-label={reveal ? t('webHideKey') : t('webShowKey')} title={reveal ? t('webHideKey') : t('webShowKey')} onClick={() => setReveal((value) => !value)}>
            <Icon icon={reveal ? EyeOff : Eye} />
          </Button>
          <CopyButton value={token ?? ''} label={t('apiServiceCopy')} />
          <Button iconOnly aria-label={t('apiServiceTokenGenerate')} title={t('apiServiceTokenGenerate')} loading={busy === 'regenerate'} disabled={disabled || busy !== null || token === null} onClick={() => void regenerate()}>
            <Icon icon={RefreshCw} />
          </Button>
        </div>
        {issue ? (
          <div role="alert" className="text-xs text-destructive">
            {t(ISSUE_KEYS[issue], { min: ACCESS_KEY_MIN_LEN, max: ACCESS_KEY_MAX_LEN })}
          </div>
        ) : null}
        {dirty ? (
          <div className="flex justify-end">
            <Button variant="primary" loading={busy === 'save'} disabled={disabled || busy !== null} onClick={() => void save()}>
              {t('webSetupSubmit')}
            </Button>
          </div>
        ) : null}
      </div>
    </SettingsRow>
  )
}

export function ApiSettings() {
  const t = useT()
  const navigate = useNavigate()
  const signOut = () => {
    // 清凭证即触发 ConnectionController 断开连接；随后回登录页。
    clearToken()
    void navigate({ to: '/login', replace: true })
  }
  const gateway = useAgent((snapshot) => snapshot.gateway, null)
  const port = window.location.port || (window.location.protocol === 'https:' ? '443' : '80')

  return (
    <SettingsPage title={t('settingsCatApiService')} description={t('settingsCatApiServiceDesc')}>
      <SettingsSection title={t('settingsCatApiService')}>
        <SettingsRow title={t('apiServicePort')} description={t('webApiBindFixed')}>
          <span className="text-sm tabular text-foreground">{port}</span>
        </SettingsRow>
        <AddressRow titleKey="apiServiceAddress" path="" />
        <AddressRow titleKey="apiServiceJsonrpc" path="/jsonrpc" />
        <AddressRow titleKey="apiServiceMcp" path="/mcp" />
        <AddressRow titleKey="apiServiceApi" path="/api/v1" />
        {gateway ? <AccessKeyRow configured={gateway.userTokenConfigured} /> : <SettingsCustomRow>{null}</SettingsCustomRow>}
        <SettingsRow title={t('webLogoutTitle')} description={t('webLogoutDesc')}>
          <Button variant="outline" onClick={signOut}>
            {t('accountLogout')}
          </Button>
        </SettingsRow>
      </SettingsSection>
      {gateway ? (
        <SettingsSection title={t('apiServiceFeaturesTitle')} subtitle={t('apiServiceFeaturesDesc')}>
          {FEATURES.map((feature) => (
            <FeatureRow key={feature.flag} gateway={gateway} {...feature} />
          ))}
        </SettingsSection>
      ) : null}
    </SettingsPage>
  )
}
