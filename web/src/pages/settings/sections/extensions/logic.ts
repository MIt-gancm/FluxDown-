// 扩展设置的纯逻辑（无 React / 别名依赖，可被 bun test 直接加载）。
// 语义与 crates/extensions（plugin_settings.rs / plugins.rs / plugin_auth.rs / managed_components.rs）逐条对齐。

import type { MarketEntryDto, SettingFieldDto } from '../../../../lib/rpc/protocol'

// ── 插件设置校验 ────────────────────────────────────────────

/** 前置校验失败原因；文案映射留给 UI 层。 */
export type FieldError =
  | { kind: 'required' }
  | { kind: 'number' }
  | { kind: 'min'; min: string }
  | { kind: 'max'; max: string }
  | { kind: 'select' }
  /** daemon 校验失败并指明了字段（如 `pattern` 不匹配）；文案已本地化。 */
  | { kind: 'server'; message: string }

/** 十进制浮点（与 Rust `f64::from_str` 对常规输入的接受范围一致，不含 hex / inf / NaN）。 */
const DECIMAL = /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/

/** 单条设置项前置校验：required → number/min/max → select 成员。 */
export function validateField(field: SettingFieldDto, raw: string): FieldError | null {
  const value = raw.trim()
  if (field.required && value === '') return { kind: 'required' }
  if (value === '') return null
  if (field.type === 'number') {
    if (!DECIMAL.test(value)) return { kind: 'number' }
    const number = Number(value)
    if (!Number.isFinite(number)) return { kind: 'number' }
    if (field.min !== null && number < field.min) return { kind: 'min', min: String(field.min) }
    if (field.max !== null && number > field.max) return { kind: 'max', max: String(field.max) }
  }
  if (field.widget === 'select' && field.options.length > 0 && !field.options.some((option) => option.value === value)) {
    return { kind: 'select' }
  }
  return null
}

/** 字段初始值：已保存值优先，否则 manifest 默认值（toggle 默认 `false`）。 */
export function initialFieldValue(field: SettingFieldDto, saved: Readonly<Record<string, string>>): string {
  const stored = saved[field.key]
  if (stored !== undefined) return stored
  if (field.default) return field.default
  return field.widget === 'toggle' ? 'false' : ''
}

/** 数字字段占位提示：`1 – 10` / `≥ 1` / `≤ 10`。 */
export function rangeHint(field: SettingFieldDto): string | null {
  const { min, max } = field
  if (min !== null && max !== null) return `${String(min)} – ${String(max)}`
  if (min !== null) return `≥ ${String(min)}`
  if (max !== null) return `≤ ${String(max)}`
  return null
}

// ── 市场 ────────────────────────────────────────────────────

/** 市场列表每次展开的条数。 */
export const MARKET_PAGE_SIZE = 50

/** 市场条目关键字过滤：名称 / id / 描述 / 作者 / 标签任一命中（大小写不敏感）。 */
export function filterMarket(entries: readonly MarketEntryDto[], query: string): MarketEntryDto[] {
  const needle = query.trim().toLowerCase()
  if (needle === '') return [...entries]
  const hit = (value: string) => value.toLowerCase().includes(needle)
  return entries.filter(
    (entry) =>
      hit(entry.name) || hit(entry.pluginId) || hit(entry.description) || hit(entry.author) || entry.tags.some(hit),
  )
}

/** 市场 `yanked` 标记 → i18n 键；空 / 未知值不展示。 */
export function yankedLabelKey(yanked: string): string | null {
  switch (yanked) {
    case 'deprecated':
      return 'marketYankedDeprecated'
    case 'vulnerable':
      return 'marketYankedVulnerable'
    case 'malicious':
      return 'marketYankedMalicious'
    default:
      return null
  }
}

/** 权限 → i18n 键（名称、说明）；未知权限返回 null（调用侧回退展示原始名 + 未知说明）。 */
export function permissionKeys(permission: string): { name: string; desc: string } | null {
  switch (permission) {
    case 'ffmpeg':
      return { name: 'pluginPermFfmpegName', desc: 'pluginPermFfmpegDesc' }
    case 'ytdlp':
      return { name: 'pluginPermYtdlpName', desc: 'pluginPermYtdlpDesc' }
    case 'auth':
      return { name: 'pluginPermAuthName', desc: 'pluginPermAuthDesc' }
    default:
      return null
  }
}

// ── 插件登录挑战 ────────────────────────────────────────────

/** 挑战文本超过该长度即截断显示（复制按钮始终给出完整原文）。 */
export const CHALLENGE_TEXT_LIMIT = 512

/** `data:image/...;base64,` 挑战超过该长度直接放弃图片渲染，退化为文本 + 复制按钮。 */
export const MAX_CHALLENGE_DATA_URL_LEN = 256 * 1024

const IMAGE_MIMES: Record<string, true> = {
  'image/png': true,
  'image/jpeg': true,
  'image/gif': true,
  'image/webp': true,
  'image/bmp': true,
  'image/svg+xml': true,
}
const BASE64_BODY = /^[A-Za-z0-9+/]*={0,2}$/

/**
 * 插件挑战不可信：只有形如 `data:image/<mime>[;..];base64,<payload>` 且 mime 已知、载荷是合法 base64、
 * 长度未超限的值才当图片渲染；其余返回 null（调用侧退化为截断文本 + 复制）。
 * 返回值可直接作 `<img src>`。
 */
export function dataImageChallengeSrc(value: string): string | null {
  if (value.length > MAX_CHALLENGE_DATA_URL_LEN) return null
  if (!value.toLowerCase().startsWith('data:')) return null
  const comma = value.indexOf(',')
  if (comma < 0) return null
  const [mimeRaw = '', ...params] = value.slice('data:'.length, comma).split(';')
  const mime = mimeRaw.toLowerCase()
  if (!params.some((param) => param.toLowerCase() === 'base64')) return null
  if (!Object.hasOwn(IMAGE_MIMES, mime)) return null
  const payload = value.slice(comma + 1).replace(/\s+/g, '')
  if (payload === '' || payload.length % 4 === 1 || !BASE64_BODY.test(payload)) return null
  return `data:${mime};base64,${payload}`
}

/** 挑战文本超过 {@link CHALLENGE_TEXT_LIMIT} 个字符（按码点计）时截断并追加省略号。 */
export function truncateChallengeText(value: string): string {
  const chars = Array.from(value)
  if (chars.length <= CHALLENGE_TEXT_LIMIT) return value
  return `${chars.slice(0, CHALLENGE_TEXT_LIMIT).join('')}…`
}

/** 仅 http(s) 链接可点击（挑战 / 主页均不可信，拒绝 `javascript:` 等）。 */
export function safeHttpUrl(value: string): string | null {
  try {
    const url = new URL(value)
    return url.protocol === 'http:' || url.protocol === 'https:' ? url.href : null
  } catch {
    return null
  }
}

// ── 组件 ────────────────────────────────────────────────────

/** 1024 进制的人类可读字节数（`0 B` / `1.5 KB` / `12.3 MB` / `1.0 GB`）。 */
export function formatBytes(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB']
  const clamped = Math.max(0, bytes)
  let value = clamped
  let unit = 0
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024
    unit += 1
  }
  return unit === 0 ? `${Math.trunc(clamped)} B` : `${value.toFixed(1)} ${units[unit]}`
}
