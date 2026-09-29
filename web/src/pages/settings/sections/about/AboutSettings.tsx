// 关于（GPUI `crates/settings/src/sections/about.rs`）：版本、软件更新、日志导出、浏览器扩展与捐赠链接。
// 桌面专属的「打开日志目录」不在 Web 出现；更新只做版本检查并跳转发布页（服务端用 Docker/二进制自行升级）。

import { useState } from 'react'
import { useT } from '../../../../i18n'
import { exportLogs, rpc, useRpcSelector } from '../../../../lib/rpc'
import type { UpdateCheckResultDto } from '../../../../lib/rpc'
import { Button, toast } from '../../../../ui'
import { NumberField, PrefDropdownRow, PrefSwitchRow, SettingsCustomRow, SettingsPage, SettingsRow, SettingsSection, rpcErrorText, setPref, useSettingsReadOnly, usePrefNumber, usePrefString } from '../../kit'

const CHROME_STORE = 'https://chromewebstore.google.com/search/FluxDown'
const FIREFOX_STORE = 'https://addons.mozilla.org/firefox/addon/fluxdown/'
const EDGE_STORE = 'https://microsoftedge.microsoft.com/addons/search/FluxDown'
const DONATE = 'https://fluxdown.zerx.dev/sponsor'
const WEBSITE = 'https://fluxdown.zerx.dev'
/** headless 服务端独立 tag `server-v*`。 */
const SERVER_RELEASES = 'https://github.com/zerx-lab/FluxDown/releases?q=server-v'

const LOG_MAX_MIN = 1
const LOG_MAX_MAX = 1024
const LOG_MAX_DEFAULT = 10

function openUrl(url: string) {
  window.open(url, '_blank', 'noopener,noreferrer')
}

function LinkButtons({ links }: { links: readonly { label: string; url: string }[] }) {
  return (
    <div className="flex flex-wrap gap-2">
      {links.map((link) => (
        <Button key={link.url} onClick={() => openUrl(link.url)}>
          {link.label}
        </Button>
      ))}
    </div>
  )
}

function UpdateRow() {
  const t = useT()
  const disabled = useSettingsReadOnly()
  const channel = usePrefString('general.update_channel', 'stable')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<UpdateCheckResultDto | null>(null)

  const check = async () => {
    if (busy) return
    setBusy(true)
    try {
      setResult(await rpc.agent.update.check({ channel: channel === 'frontier' ? 'frontier' : 'stable' }))
    } catch (error) {
      toast.error(rpcErrorText(error, t))
    } finally {
      setBusy(false)
    }
  }

  const status = result ? (result.hasUpdate ? t('updateAvailableToast', { v: result.latestVersion }) : `${t('latestVersion')}: v${result.latestVersion}`) : null
  const pageUrl = result?.hasUpdate ? result.releasePageUrl : ''

  return (
    <>
      <SettingsRow title={t('checkUpdate')} description={t('checkUpdateDesc')}>
        <div className="flex flex-wrap items-center gap-2 desktop:justify-end">
          {status ? <span className="text-xs text-muted-foreground">{status}</span> : null}
          {pageUrl ? (
            <Button variant="primary" onClick={() => openUrl(pageUrl)}>
              {t('updateNow')}
            </Button>
          ) : null}
          <Button loading={busy} disabled={disabled} onClick={() => void check()}>
            {t('checkUpdate')}
          </Button>
        </div>
      </SettingsRow>
      {result && result.notes.length > 0 ? (
        <SettingsCustomRow className="flex flex-col gap-3">
          {result.notes.slice(0, 10).map((note) => (
            <div key={note.version} className="flex flex-col gap-0.5">
              <div className="text-sm text-foreground">
                v{note.version} {note.publishedAt}
              </div>
              <div className="whitespace-pre-wrap break-words text-xs text-muted-foreground">{note.body}</div>
            </div>
          ))}
        </SettingsCustomRow>
      ) : null}
    </>
  )
}

function ExportRow() {
  const t = useT()
  const disabled = useSettingsReadOnly()
  const [busy, setBusy] = useState(false)
  const exportNow = async () => {
    if (busy) return
    setBusy(true)
    try {
      await exportLogs()
    } catch (error) {
      toast.error(rpcErrorText(error, t), t('logExportFailed'))
    } finally {
      setBusy(false)
    }
  }
  return (
    <SettingsRow title={t('logExportButton')}>
      <div className="flex desktop:justify-end">
        <Button variant="primary" loading={busy} disabled={disabled} onClick={() => void exportNow()}>
          {t('logExportButton')}
        </Button>
      </div>
    </SettingsRow>
  )
}

export function AboutSettings() {
  const t = useT()
  const version = useRpcSelector((state) => state.hello?.serviceVersion ?? '')
  const logMax = usePrefNumber('log_max_size_mb', LOG_MAX_DEFAULT)

  return (
    <SettingsPage title={t('settingsCatAbout')} description={t('settingsCatAboutDesc')}>
      <SettingsSection title="FluxDown">
        <SettingsRow title={t('currentVersion')}>
          <span className="text-sm tabular text-foreground">{version ? `v${version}` : '—'}</span>
        </SettingsRow>
      </SettingsSection>
      <SettingsSection title={t('softwareUpdate')}>
        <PrefDropdownRow
          prefKey="general.update_channel"
          fallback="stable"
          titleKey="updateChannel"
          descKey="updateChannelDesc"
          options={[
            { value: 'stable', label: t('updateChannelStable') },
            { value: 'frontier', label: t('updateChannelFrontier') },
          ]}
        />
        <PrefSwitchRow prefKey="general.auto_check_update" fallback titleKey="autoCheckUpdate" descKey="autoCheckUpdateDesc" />
        <UpdateRow />
        <SettingsRow title={t('webServerReleases')}>
          <LinkButtons links={[{ label: 'GitHub', url: SERVER_RELEASES }]} />
        </SettingsRow>
      </SettingsSection>
      <SettingsSection title={t('logExport')} subtitle={t('logExportDesc')}>
        <SettingsRow title={t('logMaxSize')} description={t('logMaxSizeDesc')}>
          <NumberField value={logMax} min={LOG_MAX_MIN} max={LOG_MAX_MAX} unit="MB" aria-label={t('logMaxSize')} onCommit={(next) => setPref('log_max_size_mb', next)} />
        </SettingsRow>
        <ExportRow />
      </SettingsSection>
      <SettingsSection title={t('extensionCardTitle')} subtitle={t('extensionCardDesc')}>
        <SettingsRow title={t('extensionCardTitle')}>
          <LinkButtons
            links={[
              { label: 'Chrome', url: CHROME_STORE },
              { label: 'Firefox', url: FIREFOX_STORE },
              { label: 'Edge', url: EDGE_STORE },
            ]}
          />
        </SettingsRow>
        <SettingsRow title={t('donateTitle')}>
          <LinkButtons
            links={[
              { label: t('donateButton'), url: DONATE },
              { label: t('officialWebsite'), url: WEBSITE },
            ]}
          />
        </SettingsRow>
      </SettingsSection>
    </SettingsPage>
  )
}
