// 已保存的站点 HTTP Basic 凭据（crates/settings/src/sections/site_auth.rs）：只列站点与用户名，可逐条删除或清空。

import { useEffect, useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { SiteAuthEntryDto } from '../../../../lib/rpc'
import { Button, toast } from '../../../../ui'
import { SettingsCustomRow, SettingsSection, rpcErrorText } from '../../kit'

export function SiteAuthGroup() {
  const t = useT()
  const [entries, setEntries] = useState<SiteAuthEntryDto[]>([])
  /** 进行中的动作标记：站点名 / 'clearAll'。 */
  const [busy, setBusy] = useState<string | null>(null)

  useEffect(() => {
    let cancelled = false
    rpc.daemon.siteAuth
      .list()
      .then((list) => {
        if (!cancelled) setEntries(list)
      })
      .catch((error: unknown) => {
        if (!cancelled) toast.error(rpcErrorText(error, t))
      })
    return () => {
      cancelled = true
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅首次进入时拉取
  }, [])

  const run = async (tag: string, action: () => Promise<SiteAuthEntryDto[]>) => {
    setBusy(tag)
    try {
      setEntries(await action())
    } catch (error) {
      toast.error(rpcErrorText(error, t))
    } finally {
      setBusy(null)
    }
  }

  return (
    <SettingsSection title={t('settingsSiteAuthTitle')} subtitle={t('settingsSiteAuthDesc')}>
      <SettingsCustomRow className="flex flex-col gap-1">
        {entries.length === 0 ? <div className="text-xs text-muted-foreground">{t('settingsSiteAuthEmpty')}</div> : null}
        {entries.map((entry) => (
          <div key={entry.site} className="flex items-center justify-between gap-3 border-b border-hairline py-1">
            <div className="flex min-w-0 flex-col gap-0.5">
              <span className="truncate text-sm text-foreground">{entry.site}</span>
              <span className="truncate text-xs text-muted-foreground">{entry.user}</span>
            </div>
            <Button
              variant="ghost"
              className="text-destructive"
              loading={busy === entry.site}
              disabled={busy !== null}
              onClick={() => void run(entry.site, () => rpc.daemon.siteAuth.delete({ site: entry.site }))}
            >
              {t('settingsSiteAuthDelete')}
            </Button>
          </div>
        ))}
        <div className="flex justify-end pt-1">
          <Button variant="outline" loading={busy === 'clearAll'} disabled={busy !== null || entries.length === 0} onClick={() => void run('clearAll', () => rpc.daemon.siteAuth.clear())}>
            {t('settingsSiteAuthClearAll')}
          </Button>
        </div>
      </SettingsCustomRow>
    </SettingsSection>
  )
}
