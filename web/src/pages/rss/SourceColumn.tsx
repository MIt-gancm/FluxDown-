// 订阅源列（GPUI `RssView::render_source` + 侧栏头）：桌面 240px 可拖拽；移动端为整屏列表。

import { ChevronRight, Plus, Rss } from 'lucide-react'
import { useT } from '../../i18n'
import { cn } from '../../lib/cn'
import type { RssSourceDto } from '../../lib/rpc'
import { Button, Icon, Tooltip } from '../../ui'
import { sourceDisplayName } from './filter'
import { sourceStatusLine } from './format'

function SourceRow({
  source,
  selected,
  refreshing,
  now,
  onSelect,
}: {
  source: RssSourceDto
  selected: boolean
  refreshing: boolean
  now: number
  onSelect: () => void
}) {
  const t = useT()
  const unhealthy = source.lastError !== ''
  return (
    <button
      type="button"
      aria-current={selected ? 'true' : undefined}
      onClick={onSelect}
      className={cn(
        'flex w-full min-w-0 items-center gap-2 rounded-[var(--fx-components-nav-item-radius)] px-2 py-1.5 text-left transition-colors',
        'mobile:min-h-touch mobile:gap-3 mobile:px-3',
        selected ? 'bg-nav-selected' : 'hover:bg-nav-hover',
      )}
    >
      <Icon icon={Rss} className={cn(unhealthy ? 'text-destructive' : selected ? 'text-nav-selected-icon' : 'text-muted-foreground')} />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className={cn('truncate text-sm', selected ? 'font-medium text-nav-selected-foreground' : 'text-muted-foreground mobile:text-foreground')}>
          {sourceDisplayName(source)}
        </span>
        <span className={cn('truncate text-xs tabular', unhealthy ? 'text-destructive' : 'text-text-tertiary')}>
          {sourceStatusLine(t, source, refreshing, now)}
        </span>
      </span>
      {source.unreadCount > 0 ? (
        <Tooltip content={t('rssUnreadCount', { n: source.unreadCount })}>
          <span className="shrink-0 rounded-full bg-muted px-1.5 text-caption font-medium text-muted-foreground tabular">{source.unreadCount}</span>
        </Tooltip>
      ) : null}
      <Icon icon={ChevronRight} className="text-text-tertiary desktop:hidden" />
    </button>
  )
}

export function SourceColumn({
  sources,
  selectedId,
  refreshingId,
  now,
  disabled,
  onSelect,
  onAdd,
}: {
  sources: readonly RssSourceDto[]
  selectedId: string | null
  refreshingId: string | null
  now: number
  disabled: boolean
  onSelect: (sourceId: string) => void
  onAdd: () => void
}) {
  const t = useT()
  return (
    <div className="flex h-full min-h-0 flex-col bg-chrome">
      <div className="flex shrink-0 items-center gap-1 pt-3 pr-2 pb-2 pl-4">
        <span className="text-caption font-medium text-text-tertiary">{t('rssSubscriptions')}</span>
        <span className="rounded-full bg-muted px-1.5 text-caption font-medium text-muted-foreground tabular">{sources.length}</span>
        <span className="flex-1" />
        <Tooltip content={t('rssAddSource')}>
          <Button variant="ghost" iconOnly aria-label={t('rssAddSource')} className="text-muted-foreground hover:text-foreground" disabled={disabled} onClick={onAdd}>
            <Icon icon={Plus} size="md" />
          </Button>
        </Tooltip>
      </div>
      <div className="flex min-h-0 flex-1 flex-col gap-0.5 overflow-y-auto px-2 pb-2">
        {sources.length === 0 ? <p className="px-2 py-4 text-xs text-text-tertiary">{t('rssSidebarEmptyHint')}</p> : null}
        {sources.map((source) => (
          <SourceRow
            key={source.sourceId}
            source={source}
            selected={source.sourceId === selectedId}
            refreshing={source.sourceId === refreshingId}
            now={now}
            onSelect={() => onSelect(source.sourceId)}
          />
        ))}
      </div>
    </div>
  )
}
