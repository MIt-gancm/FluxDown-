// 活动栏（桌面）：48px 宽、chrome 底；路由在上、动作（主题 / 设置）在下。
// 选中项：中性底 `navSelected` + 图标 `navSelectedIcon`（强调色只落在图标上）。

import { Link, useLocation } from '@tanstack/react-router'
import { useT } from '../i18n'
import { cn } from '../lib/cn'
import { useTheme } from '../theme'
import { Icon, Tooltip, toast } from '../ui'
import { isActivityActive, themeToggleIcon } from './activity'
import type { ActivityEntry } from './activity'
import { useVisibleActivityEntries } from './useVisibleActivityEntries'

const BUTTON =
  'inline-flex size-8 coarse:size-touch items-center justify-center rounded-[var(--fx-components-nav-item-radius)] text-muted-foreground transition-colors hover:bg-nav-hover hover:text-foreground'
/** 选中项悬停不变色（GPUI `activity_button`：选中态 hover 与常态一致）。 */
const BUTTON_ACTIVE = 'bg-nav-selected text-nav-selected-foreground hover:bg-nav-selected hover:text-nav-selected-foreground'

export function ActivityRail() {
  const t = useT()
  const location = useLocation()
  const theme = useTheme()
  const entries = useVisibleActivityEntries()
  const top = entries.filter((entry) => !entry.bottom)
  const bottom = entries.filter((entry) => entry.bottom)

  const renderEntry = (entry: ActivityEntry) => {
    const active = isActivityActive(entry, location.pathname)
    const icon = entry.id === 'theme' ? themeToggleIcon(theme.mode) : entry.icon
    const glyph = <Icon icon={icon} size="xl" className={cn(active && 'text-nav-selected-icon')} />
    const className = cn(BUTTON, active && BUTTON_ACTIVE)
    const label = t(entry.labelKey)
    return (
      <Tooltip key={entry.id} content={label} side="right">
        {entry.to ? (
          <Link to={entry.to} aria-label={label} aria-current={active ? 'page' : undefined} className={className}>
            {glyph}
          </Link>
        ) : (
          <button
            type="button"
            aria-label={label}
            className={className}
            onClick={() => {
              if (entry.id === 'theme') theme.toggle().catch((error: unknown) => toast.error(error))
            }}
          >
            {glyph}
          </button>
        )}
      </Tooltip>
    )
  }

  return (
    <nav aria-label="Activity" className="flex w-rail shrink-0 flex-col items-center justify-between gap-1 bg-chrome py-2 pl-safe mobile:hidden">
      <div className="flex flex-col items-center gap-1">{top.map(renderEntry)}</div>
      <div className="flex flex-col items-center gap-1">{bottom.map(renderEntry)}</div>
    </nav>
  )
}
