// 高级选项分组：HMAC 签名（含密钥行）、允许明文 HTTP、经全局代理发送。

import { useEffect, useRef, useState } from 'react'
import { useT } from '../../i18n'
import { copyText } from '../../lib/copy'
import { Button, Input, OptionGroup, OptionRow, Switch } from '../../ui'
import { generateSecret } from './template'

export interface OptionsState {
  signEnabled: boolean
  secret: string
  allowHttp: boolean
  useProxy: boolean
}

export function EndpointOptions({ value, onChange }: { value: OptionsState; onChange: (next: OptionsState) => void }) {
  const t = useT()
  const [copied, setCopied] = useState(false)
  const timer = useRef<number | undefined>(undefined)
  useEffect(() => () => window.clearTimeout(timer.current), [])

  const copySecret = () => {
    copyText(value.secret)
    setCopied(true)
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => setCopied(false), 2000)
  }

  return (
    <OptionGroup>
      <OptionRow
        title={t('webhookFieldSign')}
        description={t('webhookSignDesc')}
        control={
          <Switch
            checked={value.signEnabled}
            aria-label={t('webhookFieldSign')}
            onCheckedChange={(enabled) =>
              // 开启签名时给一个够长够随机的起点；用户可随时改成自己的。
              onChange({ ...value, signEnabled: enabled, secret: enabled && value.secret.trim() === '' ? generateSecret() : value.secret })
            }
          />
        }
      />
      {value.signEnabled ? (
        <div className="flex w-full flex-col gap-2 px-3 py-2.5 min-[560px]:flex-row min-[560px]:items-center">
          <Input
            className="font-mono"
            value={value.secret}
            autoCapitalize="off"
            autoCorrect="off"
            spellCheck={false}
            aria-label={t('webhookFieldSign')}
            onChange={(event) => onChange({ ...value, secret: event.target.value })}
          />
          <div className="flex shrink-0 gap-2 narrow:[&>*]:flex-1">
            <Button
              onClick={() => {
                setCopied(false)
                onChange({ ...value, secret: generateSecret() })
              }}
            >
              {t('webhookRegenerate')}
            </Button>
            <Button onClick={copySecret}>{t(copied ? 'webhookCopied' : 'webhookCopy')}</Button>
          </div>
        </div>
      ) : null}
      <OptionRow
        title={t('webhookFieldAllowHttp')}
        description={t('webhookAllowHttpDesc')}
        control={
          <Switch checked={value.allowHttp} aria-label={t('webhookFieldAllowHttp')} onCheckedChange={(allowHttp) => onChange({ ...value, allowHttp })} />
        }
      />
      <OptionRow
        title={t('webhookFieldUseProxy')}
        description={t('webhookUseProxyDesc')}
        control={
          <Switch checked={value.useProxy} aria-label={t('webhookFieldUseProxy')} onCheckedChange={(useProxy) => onChange({ ...value, useProxy })} />
        }
      />
    </OptionGroup>
  )
}
