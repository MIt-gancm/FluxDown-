// i18n 运行时：Context、hooks 与非 React 场景的 `t`。Provider 见 I18nProvider.tsx。

import { createContext, useContext } from 'react'
import { AVAILABLE_LOCALES, nativeName, translate } from './catalog'
import type { Params } from './catalog'

/** 语言偏好键（GPUI 外观页同键）：`system` 或 locale 代码。 */
export const LOCALE_PREF_KEY = 'general.locale'

export type TFunction = (key: string, params?: Params) => string

/** 非 React 场景（toast、错误映射）用：读取当前语言的模块级翻译函数。 */
let currentLocale = 'en'
export const t: TFunction = (key, params) => translate(currentLocale, key, params)
export function setCurrentLocale(locale: string): void {
  currentLocale = locale
}

export interface I18nContextValue {
  /** 实际生效的 locale 代码。 */
  locale: string
  t: TFunction
  /** 可选语言（代码 + 自称），顺序与 GPUI 一致。 */
  languages: readonly { code: string; name: string }[]
}

export const LANGUAGES = AVAILABLE_LOCALES.map((code) => ({ code, name: nativeName(code) }))

export const I18nContext = createContext<I18nContextValue | null>(null)

export function useI18n(): I18nContextValue {
  const value = useContext(I18nContext)
  if (!value) throw new Error('useI18n must be used inside <I18nProvider>')
  return value
}

/** 组件内翻译：`const t = useT(); t('settings')`、`t('proxyTestSuccess', { ms: 12 })`。 */
export function useT(): TFunction {
  return useI18n().t
}
