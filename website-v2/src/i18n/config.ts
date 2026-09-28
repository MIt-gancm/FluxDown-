/**
 * 站点语言契约(单一事实源)。
 *
 * - en 位于根路径(`/download`),zh 位于 `/zh` 前缀(`/zh/download`)。
 * - 每个页面都由服务端按 URL 直出目标语言;客户端不做语言探测或切换重渲染。
 * - `astro.config.mjs` 的 `langPair()` 与本文件同契约(config 无法 import TS,双处注释互指)。
 */

export const LANGS = ["en", "zh"] as const;
export type Lang = (typeof LANGS)[number];
export const DEFAULT_LANG: Lang = "en";

/** `<html lang>` / hreflang 用的 BCP 47 标签。 */
export const HTML_LANG: Record<Lang, string> = { en: "en", zh: "zh-CN" };
/** hreflang 码(Google 现行规范,ISO 639-1)。 */
export const HREFLANG: Record<Lang, string> = { en: "en", zh: "zh" };
export const OG_LOCALE: Record<Lang, string> = { en: "en_US", zh: "zh_CN" };
/** 语言切换器显示名(各自语言自称)。 */
export const LANG_LABEL: Record<Lang, string> = { en: "English", zh: "简体中文" };

export function isLang(value: unknown): value is Lang {
  return typeof value === "string" && (LANGS as readonly string[]).includes(value);
}
