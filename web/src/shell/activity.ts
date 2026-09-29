// 活动栏注册表：镜像 `crates/app/src/activity.rs`（条目顺序、文案键、可选入口偏好键）。
// 路由在上、动作在下；可选入口偏好缺省视为显示。

import { Download, Moon, Rss, Settings, Sun, Webhook } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

export type ActivityId = 'downloads' | 'rss' | 'webhooks' | 'theme' | 'settings'

export interface ActivityEntry {
  id: ActivityId
  labelKey: string
  icon: LucideIcon
  /** 路由条目的目标路径；动作条目为 undefined。 */
  to?: '/' | '/rss' | '/webhooks' | '/settings'
  /** 可选入口的可见性偏好键；undefined = 固定显示。 */
  prefKey?: string
  /** 是否在底部对齐（动作区）。 */
  bottom: boolean
}

export const ACTIVITY_ENTRIES: readonly ActivityEntry[] = [
  { id: 'downloads', labelKey: 'mobileNavDownloads', icon: Download, to: '/', bottom: false },
  { id: 'rss', labelKey: 'sidebarRss', icon: Rss, to: '/rss', prefKey: 'ui.show_activity_rss', bottom: false },
  { id: 'webhooks', labelKey: 'webhookNavTitle', icon: Webhook, to: '/webhooks', prefKey: 'ui.show_activity_webhooks', bottom: false },
  { id: 'theme', labelKey: 'activityThemeToggle', icon: Sun, prefKey: 'ui.show_activity_theme', bottom: true },
  { id: 'settings', labelKey: 'settings', icon: Settings, to: '/settings', bottom: true },
]

/** 主题切换按钮图标：暗色下显示 Sun（点击切亮），亮色下显示 Moon。 */
export function themeToggleIcon(mode: 'light' | 'dark'): LucideIcon {
  return mode === 'dark' ? Sun : Moon
}

/** 当前路径是否命中该路由条目（Downloads 精确匹配 `/`，其余前缀匹配）。 */
export function isActivityActive(entry: ActivityEntry, pathname: string): boolean {
  if (!entry.to) return false
  return entry.to === '/' ? pathname === '/' : pathname === entry.to || pathname.startsWith(`${entry.to}/`)
}
