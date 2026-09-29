// 插件平台登录对话框：一次登录后凭据由引擎保存，后续插件请求自动复用。
// 流程与 GPUI `plugin_auth.rs` 一致：打开即 status → begin →（二维码挑战每 2s 自动 poll；
// 其它挑战手动「检查状态」）→ success；已登录可 logout；任何方式关闭都向引擎 cancel 当前会话。

import { CircleCheck, Clock, Copy } from 'lucide-react'
import { useEffect, useId, useRef, useState } from 'react'
import { useT } from '../../../../i18n'
import { copyText } from '../../../../lib/copy'
import { rpc } from '../../../../lib/rpc'
import type { PluginAuthRequest, PluginAuthResponse, PluginDto } from '../../../../lib/rpc'
import { Button, Dialog, DialogFooter, FieldError, FormField, Icon, Input, toast } from '../../../../ui'
import { extensionErrorText } from './errors'
import { dataImageChallengeSrc, safeHttpUrl, truncateChallengeText } from './logic'

const POLL_INTERVAL_MS = 2000

type AuthAction = 'begin' | 'poll' | 'logout' | 'status'

const isQrcode = (challengeType: string | null) => challengeType?.toLowerCase() === 'qrcode'

interface AuthState {
  status: string
  sessionId: string
  authRef: string
  challenge: string | null
  challengeType: string | null
  message: string | null
}

const INITIAL: AuthState = { status: '', sessionId: '', authRef: '', challenge: null, challengeType: null, message: null }

function ChallengeView({ value, type }: { value: string; type: string }) {
  const t = useT()
  // 载荷不可信：图片只接受校验过的 data:image；其余一律截断文本 + 复制，原文始终可取出。
  const image = dataImageChallengeSrc(value)
  const link = image ? null : safeHttpUrl(value)
  return (
    <div className="flex w-full min-w-0 flex-col gap-2 rounded-md bg-muted p-3">
      {type ? <div className="text-xs text-muted-foreground">{type}</div> : null}
      {image ? (
        <div className="flex w-full justify-center">
          <img src={image} alt={type} className="size-[200px] max-w-full rounded-sm bg-white object-contain p-2" />
        </div>
      ) : (
        <>
          <div className="whitespace-pre-wrap break-all text-xs text-foreground">{truncateChallengeText(value)}</div>
          {link ? (
            <a href={link} target="_blank" rel="noopener noreferrer" className="w-fit text-xs text-accent-text hover:underline">
              {link}
            </a>
          ) : null}
          <div>
            <Button
              variant="outline"
              icon={Copy}
              onClick={() => {
                copyText(value)
                toast.success(t('apiServiceCopied'))
              }}
            >
              {t('apiServiceCopy')}
            </Button>
          </div>
        </>
      )}
    </div>
  )
}

function AuthBody({ plugin, onClose }: { plugin: PluginDto; onClose: () => void }) {
  const t = useT()
  const siteId = useId()
  const inputId = useId()
  const [auth, setAuth] = useState<AuthState>(INITIAL)
  const [site, setSite] = useState('')
  const [input, setInput] = useState('')
  const [busy, setBusy] = useState(true)

  // 定时器 / 卸载清理读取最新值，避免闭包过期。
  const latest = useRef({ auth, site, input })
  latest.current = { auth, site, input }
  const busyRef = useRef(true)
  const queriedSite = useRef('')
  const alive = useRef(true)

  const send = async (action: AuthAction, siteValue: string): Promise<PluginAuthResponse> => {
    const { auth: current, input: inputValue } = latest.current
    const request: PluginAuthRequest = {
      identity: plugin.identity,
      action,
      site: siteValue,
      authRef: action === 'status' ? '' : current.authRef,
      sessionId: action === 'status' ? '' : current.sessionId,
      input: action === 'status' ? '' : inputValue,
    }
    return rpc.daemon.plugin.auth(request)
  }

  const apply = (response: PluginAuthResponse, options: { notifySuccess: boolean; wasLogout: boolean }) => {
    setAuth((previous) => {
      // pending 回包的 challenge 为可选字段：后续 poll 缺失时保留上一帧，避免二维码中途被抹掉。
      const pending = response.status === 'pending'
      let next: AuthState = {
        status: response.status,
        sessionId: response.sessionId,
        authRef: response.authRef ?? '',
        challenge: pending ? (response.challenge ?? previous.challenge) : response.challenge,
        challengeType: pending ? (response.challengeType ?? previous.challengeType) : response.challengeType,
        message: response.message ? response.message : null,
      }
      // logout 无论成败，引擎都已删除本地档案；客户端同步清空登录态，不依赖插件回包内容。
      if (options.wasLogout) next = { ...next, authRef: '', sessionId: '', challenge: null, challengeType: null }
      return next
    })
    if (options.notifySuccess && response.status === 'success') toast.success(t('pluginAuthSuccess'))
  }

  const run = async (action: AuthAction, options: { siteValue?: string; notifySuccess?: boolean } = {}) => {
    if (busyRef.current) return
    busyRef.current = true
    setBusy(true)
    if (action !== 'poll') setAuth((previous) => ({ ...previous, message: null }))
    try {
      const response = await send(action, options.siteValue ?? latest.current.site)
      if (!alive.current) return
      if (typeof response?.status !== 'string') {
        setAuth((previous) => ({ ...previous, message: t('pluginAuthInvalidResponse') }))
      } else {
        apply(response, { notifySuccess: options.notifySuccess ?? false, wasLogout: action === 'logout' })
      }
    } catch (error) {
      if (alive.current) {
        setAuth((previous) => ({ ...previous, message: t('pluginAuthFailed', { message: extensionErrorText(t, error) }) }))
      }
    } finally {
      if (alive.current) {
        busyRef.current = false
        setBusy(false)
      }
    }
  }
  const runRef = useRef(run)
  runRef.current = run

  // 打开即查询登录状态；卸载（X / Esc / 遮罩 / 取消按钮）时向引擎释放悬空会话。
  useEffect(() => {
    alive.current = true
    busyRef.current = false
    void runRef.current('status', { siteValue: '' })
    return () => {
      alive.current = false
      const { auth: current, site: siteValue } = latest.current
      if (current.sessionId === '') return
      // fire-and-forget：与在途 poll 并存无害。
      void rpc.daemon.plugin
        .auth({
          identity: plugin.identity,
          action: 'cancel',
          site: siteValue,
          authRef: current.authRef,
          sessionId: current.sessionId,
          input: '',
        })
        .catch(() => undefined)
    }
  }, [plugin.identity])

  const qrPolling = auth.status === 'pending' && auth.sessionId !== '' && isQrcode(auth.challengeType)
  useEffect(() => {
    if (!qrPolling) return
    const timer = setInterval(() => void runRef.current('poll', { notifySuccess: true }), POLL_INTERVAL_MS)
    return () => clearInterval(timer)
  }, [qrPolling])

  // status 在不同 action 下语义不同（logout 的 success 是「注销成功」），故已登录额外要求 authRef 非空。
  const loggedIn = auth.status === 'success' && auth.authRef !== ''
  const sessionPending = auth.status === 'pending' && auth.sessionId !== ''

  const querySite = () => {
    if (busyRef.current || site === queriedSite.current) return
    queriedSite.current = site
    void run('status', { siteValue: site })
  }

  let primary = null
  if (loggedIn) {
    primary = (
      <Button variant="outline" loading={busy} onClick={() => void run('logout')}>
        {t('pluginAuthLogout')}
      </Button>
    )
  } else if (sessionPending && !isQrcode(auth.challengeType)) {
    // 非二维码挑战没有自动轮询，提供手动「检查状态」。
    primary = (
      <Button variant="primary" loading={busy} onClick={() => void run('poll', { notifySuccess: true })}>
        {t('pluginAuthPoll')}
      </Button>
    )
  } else if (!sessionPending) {
    primary = (
      <Button variant="primary" loading={busy} onClick={() => void run('begin', { notifySuccess: true })}>
        {t('pluginAuthBegin')}
      </Button>
    )
  }

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t('pluginAuthDialogTitle', { name: plugin.name })}
      footer={
        <DialogFooter>
          <Button variant="outline" onClick={onClose} disabled={busy}>
            {t('cancel')}
          </Button>
          {primary}
        </DialogFooter>
      }
    >
      <div className="flex w-full flex-col gap-4 py-1">
        <div className="text-xs text-muted-foreground">{t('pluginAuthDescription')}</div>
        <FormField label={t('pluginAuthSiteLabel')} htmlFor={siteId}>
          <Input
            id={siteId}
            value={site}
            disabled={busy}
            placeholder={t('pluginAuthSitePlaceholder')}
            onChange={(event) => setSite(event.target.value)}
            onBlur={querySite}
            onKeyDown={(event) => {
              if (event.key === 'Enter') querySite()
            }}
          />
        </FormField>
        <FormField label={t('pluginAuthInputLabel')} htmlFor={inputId}>
          <Input
            id={inputId}
            value={input}
            disabled={busy}
            placeholder={t('pluginAuthInputPlaceholder')}
            onChange={(event) => setInput(event.target.value)}
          />
        </FormField>
        {loggedIn ? (
          <div className="flex items-start gap-2 text-sm text-foreground">
            <Icon icon={CircleCheck} className="mt-0.5 text-success" />
            <span className="min-w-0 flex-1">{t('pluginAuthSuccess')}</span>
          </div>
        ) : sessionPending ? (
          <div className="flex items-start gap-2 text-sm text-foreground">
            <Icon icon={Clock} className="mt-0.5 text-primary" />
            <span className="min-w-0 flex-1">{t('pluginAuthPending')}</span>
          </div>
        ) : null}
        {auth.challenge ? <ChallengeView value={auth.challenge} type={auth.challengeType ?? ''} /> : null}
        {auth.message ? <FieldError>{auth.message}</FieldError> : null}
      </div>
    </Dialog>
  )
}

export function PluginAuthDialog({ plugin, onClose }: { plugin: PluginDto | null; onClose: () => void }) {
  return plugin ? <AuthBody key={plugin.identity} plugin={plugin} onClose={onClose} /> : null
}
