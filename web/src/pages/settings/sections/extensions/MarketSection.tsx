// 插件市场：`daemon.plugin.marketList` 拉取索引，前端关键字过滤 + 分页展开，`marketInstall` 安装。

import { CircleAlert, Info, Package, Search, X } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { InstalledPlugin, MarketEntryDto } from '../../../../lib/rpc'
import { Badge, Button, EmptyState, Icon, Input, Spinner } from '../../../../ui'
import { BlockTitle, ExtLink, IconButton, ListCard, ListRow } from './common'
import { detailFromMarket } from './detail'
import type { PluginDetail } from './detail'
import { extensionErrorText } from './errors'
import { MARKET_PAGE_SIZE, filterMarket, yankedLabelKey } from './logic'

interface MarketState {
  loading: boolean
  error: string | null
  entries: MarketEntryDto[]
}

export function MarketSection({
  stale,
  installedIds,
  onInstalled,
  onInstallFailed,
  onShowDetail,
}: {
  stale: boolean
  installedIds: ReadonlySet<string>
  onInstalled: (result: InstalledPlugin) => void
  onInstallFailed: (error: unknown) => void
  onShowDetail: (detail: PluginDetail) => void
}) {
  const t = useT()
  const [market, setMarket] = useState<MarketState>({ loading: false, error: null, entries: [] })
  const [query, setQuery] = useState('')
  const [limit, setLimit] = useState(MARKET_PAGE_SIZE)
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set())
  const requested = useRef(false)

  const load = async () => {
    requested.current = true
    setMarket((current) => ({ ...current, loading: true, error: null }))
    try {
      const entries = await rpc.daemon.plugin.marketList()
      setMarket({ loading: false, error: null, entries })
      setLimit(MARKET_PAGE_SIZE)
    } catch (error) {
      setMarket((current) => ({ ...current, loading: false, error: extensionErrorText(t, error) }))
    }
  }
  const loadRef = useRef(load)
  loadRef.current = load

  // 首次进入插件页且连接就绪时自动拉取一次（与 GPUI `ensure_market_loaded` 一致）。
  useEffect(() => {
    if (!requested.current && !stale) void loadRef.current()
  }, [stale])

  const install = async (pluginId: string) => {
    if (pending.has(pluginId)) return
    setPending((current) => new Set(current).add(pluginId))
    try {
      onInstalled(await rpc.daemon.plugin.marketInstall({ pluginId }))
    } catch (error) {
      onInstallFailed(error)
    } finally {
      setPending((current) => {
        const next = new Set(current)
        next.delete(pluginId)
        return next
      })
    }
  }

  const filtered = useMemo(() => filterMarket(market.entries, query), [market.entries, query])
  const remaining = Math.max(0, filtered.length - limit)

  let body
  if (market.loading && market.entries.length === 0) {
    body = (
      <div className="flex items-center gap-2 py-3 text-xs text-muted-foreground">
        <Spinner size="md" />
        {t('pluginCommonLoading')}
      </div>
    )
  } else if (market.error) {
    body = (
      <div className="flex items-start gap-2 text-xs text-muted-foreground">
        <Icon icon={CircleAlert} size="md" className="mt-0.5 text-destructive" />
        <span className="min-w-0 flex-1 break-words">{t('marketLoadFailed', { message: market.error })}</span>
      </div>
    )
  } else if (market.entries.length === 0) {
    body = <EmptyState icon={Package} title={t('marketEmpty')} />
  } else {
    body = (
      <>
        <Input
          inputMode="search"
          className="coarse:pr-12"
          value={query}
          placeholder={t('marketSearchPlaceholder')}
          aria-label={t('marketSearchPlaceholder')}
          onChange={(event) => {
            setQuery(event.target.value)
            setLimit(MARKET_PAGE_SIZE)
          }}
          trailing={
            query === '' ? (
              <span className="pr-2 text-text-tertiary">
                <Icon icon={Search} size="md" />
              </span>
            ) : (
              <Button variant="ghost" iconOnly className="text-muted-foreground hover:text-foreground" aria-label={t('componentsManualPathClear')} onClick={() => setQuery('')}>
                <Icon icon={X} size="md" />
              </Button>
            )
          }
        />
        {filtered.length === 0 ? (
          <EmptyState icon={Search} title={t('marketSearchNoResult')} />
        ) : (
          <ListCard>
            {filtered.slice(0, limit).map((entry) => {
              const installed = installedIds.has(entry.pluginId)
              const isPending = pending.has(entry.pluginId)
              const yankedKey = yankedLabelKey(entry.yanked)
              return (
                <ListRow
                  key={entry.pluginId}
                  info={
                    <>
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                        <span className="min-w-0 break-words text-sm font-medium text-foreground">{entry.name === '' ? entry.pluginId : entry.name}</span>
                        <span className="tabular text-xs text-muted-foreground">v{entry.version}</span>
                        {entry.author ? <span className="text-xs text-muted-foreground">{entry.author}</span> : null}
                        {yankedKey ? <Badge tone="destructive">{t(yankedKey)}</Badge> : null}
                      </div>
                      {entry.homepage ? <ExtLink href={entry.homepage} /> : null}
                      {entry.description ? <div className="line-clamp-2 break-words text-xs text-muted-foreground">{entry.description}</div> : null}
                    </>
                  }
                  actions={
                    <>
                      <IconButton icon={Info} label={t('pluginDetailDescription')} onClick={() => onShowDetail(detailFromMarket(entry))} />
                      <Button variant="outline" loading={isPending} disabled={installed || stale} onClick={() => void install(entry.pluginId)}>
                        {installed ? t('marketInstalledButton') : isPending ? t('marketInstallingButton') : t('marketInstallButton')}
                      </Button>
                    </>
                  }
                />
              )
            })}
          </ListCard>
        )}
        {remaining > 0 ? (
          <div className="flex justify-center">
            <Button variant="ghost" onClick={() => setLimit((current) => current + MARKET_PAGE_SIZE)}>
              {t('marketShowMore', { count: remaining })}
            </Button>
          </div>
        ) : null}
      </>
    )
  }

  return (
    <section className="flex w-full flex-col gap-2 pt-4">
      <BlockTitle
        title={t('marketSectionTitle')}
        description={t('marketSectionDesc')}
        trailing={
          <Button variant="ghost" loading={market.loading} disabled={stale} onClick={() => void load()}>
            {t('marketRefreshTooltip')}
          </Button>
        }
      />
      {body}
    </section>
  )
}
