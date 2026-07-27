// RSS 条目流：侧边栏选中某订阅时接管中央主区（与任务列表互斥，见 routes/tasks.tsx）。
// 视觉语言照任务列表——头部摘要 + 工具条 + 行（标题/时间/体积/状态 chip/行尾操作）。
//
// 状态与可用操作（RssItemDto.status，值域见 lib/types.ts）：
//   0 新       → 「下载」「忽略」
//   1 已下载   → chip 可点，跳到对应任务（taskId 回链），无操作按钮
//   2/3/4/5    → 「仍要下载」（download 会绕过规则强制建任务）
// reason 是稳定原因码，一律经 lib/rss-filter.ts 的 reasonText() 映射为人读文案。

import { useState } from 'react'
import { RefreshCw, CheckCheck, Loader2, Radio, Search } from 'lucide-react'
import { cn } from '../../lib/cn'
import { fmtBytes, fmtTime } from '../../lib/format'
import { useI18n, type I18nKey } from '../../lib/i18n'
import { reasonText, sourceDisplayName } from '../../lib/rss-filter'
import type { RssItemDto, RssSourceDto } from '../../lib/types'
import {
  beginRssFetch,
  useRefreshRssSourceMutation,
  useRssFetching,
  useRssItemActionMutation,
  useRssItemsQuery,
} from '../../hooks/useRss'
import { useTasksUi } from './context'

/** 状态 → chip 配色（对齐任务行的 done/err/pause 语义色）。 */
const CHIP_CLASS: Record<number, string> = {
  0: 'new',
  1: 'done',
  2: 'skip',
  3: 'skip',
  4: 'dup',
  5: 'skip',
}

/** 抓取间隔摘要：整小时的间隔说「每 N 小时」，否则说分钟（1440 分钟不好读）。 */
function intervalText(minutes: number, t: (k: I18nKey, p?: Record<string, string | number>) => string): string {
  return minutes >= 60 && minutes % 60 === 0
    ? t('rss.everyHours', { n: minutes / 60 })
    : t('rss.everyMinutes', { n: minutes })
}

export function RssItemList({ source }: { source: RssSourceDto }) {
  const { t } = useI18n()
  const { setRssFilter, selectTask } = useTasksUi()
  const [search, setSearch] = useState('')
  const { data: items = [], isPending } = useRssItemsQuery(source.sourceId)
  const refresh = useRefreshRssSourceMutation()
  // 抓取是异步派发：mutation 的 isPending 只覆盖那一次 POST，真正的完成信号来自
  // 引擎回写 lastFetchAt 后的广播（见 hooks/useRss.ts 的 rssFetchingStore）。
  const fetching = useRssFetching(source.sourceId) || refresh.isPending
  const action = useRssItemActionMutation()

  const act = (guid: string, kind: 'download' | 'ignore') =>
    action.mutate({ sourceId: source.sourceId, req: { guid, action: kind } })

  /** 「已下载」chip：回到任务列表并选中回链任务（条目流与任务列表互斥占用主区）。 */
  function jumpToTask(taskId: string) {
    if (!taskId) return
    setRssFilter(null)
    selectTask(taskId)
  }

  const q = search.trim().toLowerCase()
  const visible = q ? items.filter((i) => i.title.toLowerCase().includes(q)) : items
  const unread = items.filter((i) => i.status === 0).length

  return (
    <div className="rss-pane">
      <header className="rss-head">
        <span className={cn('rss-head-icon', source.lastError && 'err')}>
          <Radio size={18} />
        </span>
        <div className="rss-head-main">
          <div className="rss-head-name">
            <b>{sourceDisplayName(source)}</b>
            {unread > 0 && <span className="rss-chip new">{t('rss.unreadCount', { count: unread })}</span>}
          </div>
          <div className="rss-head-meta">
            <span>{intervalText(source.intervalMinutes || 30, t)}</span>
            <span className="sep">·</span>
            <span>
              {source.lastSuccessAt > 0
                ? t('rss.lastSuccessAt', { time: fmtTime(source.lastSuccessAt) })
                : t('rss.neverFetched')}
            </span>
            <span className="sep">·</span>
            <span>{source.autoDownload ? t('rss.modeAuto') : t('rss.modeCollect')}</span>
            {source.lastError && (
              <>
                <span className="sep">·</span>
                <span className="err">{t('rss.lastErrorAt', { error: source.lastError, count: source.failCount })}</span>
              </>
            )}
          </div>
        </div>
      </header>

      <div className="rss-bar">
        <button
          type="button"
          className="btn ghost sm"
          disabled={action.isPending || unread === 0}
          onClick={() => action.mutate({ sourceId: source.sourceId, req: { guid: '', action: 'readAll' } })}
        >
          <CheckCheck size={13} />
          {t('rss.markAllRead')}
        </button>
        <button
          type="button"
          className="btn ghost sm"
          disabled={fetching}
          onClick={() => { beginRssFetch(source.sourceId, source.lastFetchAt); refresh.mutate(source.sourceId) }}
        >
          {fetching ? <Loader2 size={13} className="animate-spin" /> : <RefreshCw size={13} />}
          {fetching ? t('rss.refreshing') : t('rss.refreshNow')}
        </button>
        <div className="search rss-search">
          <Search size={14} />
          <input
            type="text"
            placeholder={t('rss.searchPlaceholder')}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Escape') {
                setSearch('')
                e.currentTarget.blur()
              }
            }}
          />
        </div>
      </div>

      <div className="task-scroll">
        {items.length === 0 ? (
          <div className="rss-empty">
            <Radio size={30} />
            <b>{isPending ? t('common.loading') : t('rss.emptyTitle')}</b>
            <span>{t('rss.emptyHint')}</span>
          </div>
        ) : visible.length === 0 ? (
          <p className="empty-tip">{t('rss.noMatch', { query: search.trim() })}</p>
        ) : (
          visible.map((item) => (
            <RssRow key={item.guid} item={item} busy={action.isPending} onAct={act} onJump={jumpToTask} />
          ))
        )}
      </div>
    </div>
  )
}

function RssRow({
  item,
  busy,
  onAct,
  onJump,
}: {
  item: RssItemDto
  busy: boolean
  onAct: (guid: string, kind: 'download' | 'ignore') => void
  onJump: (taskId: string) => void
}) {
  const { t } = useI18n()
  const why = reasonText(item.reason, item.episodeKey)
  const chipText =
    item.status === 0
      ? t('rss.statusNew')
      : item.status === 1
        ? t('rss.statusDownloaded')
        : item.status === 2
          ? t('rss.statusIgnored')
          : item.status === 4
            ? t('rss.statusDupEpisode')
            : item.status === 5
              ? t('rss.statusSeedSkipped')
              : t('rss.statusFiltered')

  return (
    <div className="rss-row">
      <div className="rss-row-main">
        <div className="rss-row-name">
          <b title={item.title}>{item.title}</b>
        </div>
        <div className="rss-row-meta">
          <span>{item.pubDate > 0 ? fmtTime(item.pubDate) : t('common.unknown')}</span>
          {item.enclosureLength > 0 && (
            <>
              <span className="sep">·</span>
              <span>{fmtBytes(item.enclosureLength)}</span>
            </>
          )}
          {item.status !== 1 && why && (
            <>
              <span className="sep">·</span>
              <span>{why}</span>
            </>
          )}
        </div>
      </div>
      <div className="rss-row-status">
        {item.status === 1 ? (
          <button type="button" className="rss-chip done clickable" title={t('rss.jumpToTask')} onClick={() => onJump(item.taskId)}>
            {chipText}
          </button>
        ) : (
          <span className={cn('rss-chip', CHIP_CLASS[item.status])} title={chipText}>{chipText}</span>
        )}
      </div>
      <div className="rss-row-ops">
        {item.status === 0 ? (
          <>
            <button type="button" className="btn primary sm" disabled={busy} onClick={() => onAct(item.guid, 'download')}>
              {t('rss.download')}
            </button>
            <button type="button" className="btn ghost sm" disabled={busy} onClick={() => onAct(item.guid, 'ignore')}>
              {t('rss.ignore')}
            </button>
          </>
        ) : (
          // 已下载也允许再来一遍（任务可能被删了、下崩了）——挡住重下只会逼用户
          // 去别处找种子。其余被过滤/忽略的状态则是「绕过规则强制下载」。
          <button type="button" className="btn ghost sm" disabled={busy} onClick={() => onAct(item.guid, 'download')}>
            {t(item.status === 1 ? 'rss.actionRedownload' : 'rss.downloadAnyway')}
          </button>
        )}
      </div>
    </div>
  )
}
