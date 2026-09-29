import { createContext, useContext } from 'react'
import type { AppearancePreferences, ThemeModeName, ThemePreference } from './appearance'

export interface ThemeContextValue {
  /** 生效的明暗模式（`system` 已解析）。 */
  mode: ThemeModeName
  preference: ThemePreference
  prefs: AppearancePreferences
  /** 亮 ↔ 暗切换：写 agent 偏好 `appearance.theme_mode`（GPUI `toggle_theme` 同语义）。 */
  toggle: () => Promise<void>
}

export const ThemeContext = createContext<ThemeContextValue | null>(null)

export function useTheme(): ThemeContextValue {
  const value = useContext(ThemeContext)
  if (!value) throw new Error('useTheme must be used inside <ThemeProvider>')
  return value
}
