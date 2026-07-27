// 外观：主题模式 + 强调色 + 语言 —— 主题/强调色纯前端（useTheme）；语言切换写穿服务器 config。
import { ACCENT_PRESETS, useTheme } from '../../lib/theme'
import type { ThemeMode } from '../../lib/theme'
import { cn } from '../../lib/cn'
import { LANGUAGE_CONFIG_KEY, useI18n } from '../../lib/i18n'
import type { Locale } from '../../lib/i18n'
import { LOCALES } from '../../lib/locales'
import { SetRow, SetSelect } from './controls'
import { useConfigMutation } from './useConfig'

const LANGUAGE_OPTIONS: { value: Locale; label: string }[] = LOCALES.map(({ code, name }) => ({
  value: code,
  label: name,
}))

export function AppearanceSettings() {
  const { mode, setMode, accent, setAccent } = useTheme()
  const { t, locale, setLocale } = useI18n()
  const mutation = useConfigMutation()

  const MODE_OPTIONS: { value: ThemeMode; label: string }[] = [
    { value: 'light', label: t('set.appearance.light') },
    { value: 'dark', label: t('set.appearance.dark') },
    { value: 'system', label: t('set.appearance.system') },
  ]

  function onLanguageChange(v: string) {
    setLocale(v as Locale)
    mutation.mutate({ [LANGUAGE_CONFIG_KEY]: v })
  }

  return (
    <>
      <h2 className="set-title">{t('set.appearance')}</h2>
      <p className="set-desc">{t('set.appearance.desc')}</p>
      <div className="set-group">
        <SetRow title={t('set.appearance.themeMode')}>
          <SetSelect value={mode} onValueChange={(v) => setMode(v as ThemeMode)} options={MODE_OPTIONS} />
        </SetRow>
        <SetRow
          title={t('set.appearance.accent')}
          desc={ACCENT_PRESETS.map((p) => t(p.nameKey)).join(' / ')}
        >
          <div className="color-dots">
            {ACCENT_PRESETS.map((p, i) => (
              <button
                key={p.nameKey}
                type="button"
                aria-label={t(p.nameKey)}
                className={cn('color-dot', i === accent && 'active')}
                style={{ background: p.light }}
                onClick={() => setAccent(i)}
              />
            ))}
          </div>
        </SetRow>
        <SetRow title={t('set.appearance.language')} desc={t('set.appearance.languageDesc')}>
          <SetSelect value={locale} onValueChange={onLanguageChange} options={LANGUAGE_OPTIONS} />
        </SetRow>
      </div>
    </>
  )
}
