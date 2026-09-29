// 移动端底部标签栏（<=820px）：替代活动栏。44px 触摸目标、图标 + caption 文字标签，
// 底部避开 Home 指示条（safe-area-inset-bottom）。条目与桌面活动栏同源（activity.ts）。

import { Link, useLocation } from '@tanstack/react-router'
import { useT } from '../i18n'
import { cn } from '../lib/cn'
import { useTheme } from '../theme'
import { Icon, toast } from '../ui'
import { isActivityActive, themeToggleIcon } from './activity'
import type { ActivityEntry } from './activity'
import { useVisibleActivityEntries } from './useVisibleActivityEntries'

/** 标签栏内容高度（不含安全区）。与 index.css 的 `--fx-bottom-bar` 保持一致。 */
export const BOTTOM_BAR_HEIGHT = 56

export function BottomTabBar() {
  const t = useT()
  const location = useLocation()
  const theme = useTheme()
  const entries = useVisibleActivityEntries()

  const renderEntry = (entry: ActivityEntry) => {
    const active = isActivityActive(entry, location.pathname)
    const icon = entry.id === 'theme' ? themeToggleIcon(theme.mode) : entry.icon
    const label = entry.id === 'theme' ? t(theme.mode === 'dark' ? 'themeModeLight' : 'themeModeDark') : t(entry.labelKey)
    const content = (
      <>
        <Icon icon={icon} size="xl" className={cn(active && 'text-nav-selected-icon')} />
        <span className={cn('max-w-full truncate text-caption', active ? 'font-medium text-foreground' : 'text-muted-foreground')}>
          {label}
        </span>
      </>
    )
    const className = 'flex min-h-touch min-w-touch flex-1 flex-col items-center justify-center gap-0.5 px-1 active:bg-nav-hover'
    return entry.to ? (
      <Link key={entry.id} to={entry.to} aria-current={active ? 'page' : undefined} className={className}>
        {content}
      </Link>
    ) : (
      <button
        key={entry.id}
        type="button"
        className={className}
        onClick={() => {
          if (entry.id === 'theme') theme.toggle().catch((error: unknown) => toast.error(error))
        }}
      >
        {content}
      </button>
    )
  }

  return (
    <nav
      aria-label="Activity"
      className="flex shrink-0 items-stretch border-t border-hairline bg-chrome pb-safe pl-safe pr-safe desktop:hidden"
      style={{ minHeight: BOTTOM_BAR_HEIGHT }}
    >
      {entries.map(renderEntry)}
    </nav>
  )
}
