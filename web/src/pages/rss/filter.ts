// RSS 过滤规则引擎（前端副本）—— 订阅编辑器「过滤」Tab 的实时预览用。
// 由遗留 `lib/rss-filter.ts` 迁入（纯逻辑，不依赖任何传输层）。
//
// 逐条对齐 native/engine/src/rss/filter.rs：预览一旦与引擎分叉就是在骗人，
// 所以这里不做任何「差不多就行」的简化，包括原型 docs/rss_ui_preview.html 之后
// 三处有意的收紧：
//   1. `NxM` 剧集格式限 `\b(\d{1,2})x(\d{1,3})\b`，避免把 1920x1080 当成第 1080 集；
//   2. 体积上下限只对 `enclosureLength > 0` 的条目生效（未知大小放行）；
//   3. 归一番名按 Unicode 字符而非 UTF-16 码元取前 24 个。
// 判定顺序固定为 包含 → 排除 → 体积下限 → 体积上限 → 剧集去重，先命中先返回：
// 一个条目只有一个原因。
//
// 引擎只产出稳定原因码，面向用户的文案一律经 reasonKey() 映射到 i18n 键——原码永不上屏
// （与 GPUI `crates/rss/src/view.rs` 一致：未知码与首轮播种跳过不显示原因行）。

import type { RssItemDto, RssSourceDto } from '../../lib/rpc'

/** 条目被过滤掉的原因（引擎 RejectReason::code 的完整值域）。 */
export type RssRejectReason = 'not_included' | 'excluded' | 'too_small' | 'too_large' | 'dup_episode'

/** 条目上可展示的原因码：过滤原因 + 引擎写入的种子抓取失败码。 */
export type RssReason = RssRejectReason | 'torrent_fetch_failed'

const REASON_KEYS: Record<RssReason, string> = {
  not_included: 'rssReasonNotIncluded',
  excluded: 'rssReasonExcluded',
  too_small: 'rssReasonTooSmall',
  too_large: 'rssReasonTooLarge',
  dup_episode: 'rssReasonDupEpisode',
  torrent_fetch_failed: 'rssReasonTorrentFetchFailed',
}

/** 原因码 → i18n 键；空码 / 未知码 / `seed_skipped` 返回 null（不显示原因行）。 */
export function reasonKey(code: string): string | null {
  return (REASON_KEYS as Record<string, string | undefined>)[code] ?? null
}

// ---------------------------------------------------------------------------
// Unicode 词边界
// ---------------------------------------------------------------------------

// Rust regex 的 `\b` 是 Unicode 感知的（`\w` 含 CJK 等字母），JS 的 `\b` 只认 ASCII。
// 直接照抄 `\b` 会让「番名 - 02话」这类标题在两端分叉（JS 认边界、Rust 不认），
// 故用前后瞻手工还原 Rust `\w` 的定义。
const WORD_CHAR = '[\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}]'
const NOT_AFTER_WORD = `(?<!${WORD_CHAR})`
const NOT_BEFORE_WORD = `(?!${WORD_CHAR})`

// ---------------------------------------------------------------------------
// 体积字面量
// ---------------------------------------------------------------------------

const SIZE_RE = /^([0-9]+(?:\.[0-9]+)?)\s*([kmgt]?)b?$/i
const SIZE_MULT: Record<string, number> = {
  '': 1,
  K: 1024,
  M: 1024 ** 2,
  G: 1024 ** 3,
  T: 1024 ** 4,
}
/** 引擎的溢出上界写作 `i64::MAX as f64`，该转换落到 2^63——此处用同一个可精确
 *  表示的值，避免写 i64::MAX 字面量反而被 f64 舍入成别的数。 */
const I64_MAX_AS_F64 = 2 ** 63

/** 解析体积字面量为字节数：`200M` / `2G` / `1.5 GB` / `1024`（1024 进制）。
 *  空串与无法解析的输入统一返回 null = 不限。 */
export function parseSize(input: string): number | null {
  const text = input.trim()
  if (!text) return null
  const caps = SIZE_RE.exec(text)
  if (!caps) return null
  const value = Number.parseFloat(caps[1])
  const bytes = value * (SIZE_MULT[caps[2].toUpperCase()] ?? 1)
  if (!Number.isFinite(bytes) || bytes < 0 || bytes > I64_MAX_AS_F64) return null
  return Math.trunc(bytes)
}

/** 反向格式化：字节数 → 体积字面量（0 = 空串 = 不限）。与 parseSize 构成往返，
 *  供对话框把 sizeMinBytes/sizeMaxBytes 回填进文本框。 */
export function formatSize(bytes: number): string {
  if (bytes <= 0) return ''
  const units: [number, string][] = [
    [1024 ** 4, 'T'],
    [1024 ** 3, 'G'],
    [1024 ** 2, 'M'],
    [1024, 'K'],
  ]
  for (const [scale, suffix] of units) {
    if (bytes % scale === 0) return `${bytes / scale}${suffix}`
  }
  return String(bytes)
}

// ---------------------------------------------------------------------------
// 智能剧集去重
// ---------------------------------------------------------------------------

const SEASON_EP = /s(\d+)e(\d+)/giu
// 季号限 1-2 位、集号限 1-3 位：`1920x1080` 这类分辨率不再被误判。
const CROSS_EP = new RegExp(`${NOT_AFTER_WORD}(\\d{1,2})x(\\d{1,3})${NOT_BEFORE_WORD}`, 'gu')
const DASH_EP = new RegExp(`-\\s*(\\d{2,3})${NOT_BEFORE_WORD}`, 'gu')
// `2025-09-27` 里的 `-09` / `-27` 是日期分量，不是集号：`-` 紧贴 YYYY 或 YYYY-MM 时跳过。
const DATE_TAIL = /\d{4}(?:-\d{2})?$/
const CJK_EP = /第\s*(\d+)\s*[话話集]/gu
/** Rust 侧集号解析为 u32，溢出即换下一个匹配器——此处照同样的门槛放行。 */
const U32_MAX = 4_294_967_295

const EPISODE_MATCHERS: [RegExp, number][] = [
  [SEASON_EP, 2],
  [CROSS_EP, 2],
  [DASH_EP, 1],
  [CJK_EP, 1],
]

const BRACKETS = /\[[^\]]*\]/gu
const EP_TOKENS = new RegExp(
  `s\\d+e\\d+|第\\s*\\d+\\s*[话話集]|-\\s*\\d{2,3}${NOT_BEFORE_WORD}|${NOT_AFTER_WORD}\\d{1,2}x\\d{1,3}${NOT_BEFORE_WORD}`,
  'giu',
)
const NOISE = /[\s/～~]+/gu

/** 归一番名：去掉 `[...]` 字幕组/规格标签 → 去掉集号片段 → 去掉空白与 `/`、`～`、`~`
 *  → 取前 24 个 Unicode 字符。 */
export function normalizedSeries(title: string): string {
  let text = title
  for (const re of [BRACKETS, EP_TOKENS, NOISE]) text = text.replace(re, '')
  return Array.from(text).slice(0, 24).join('')
}

/** 从标题提取「番名归一键 + 集号」，识别 `S01E02` / `1x02` / `- 02` / `第02话`。
 *  识别失败返回 null = 放行（宁可重复不可漏下）。 */
export function episodeKey(title: string): string | null {
  for (const [re, group] of EPISODE_MATCHERS) {
    for (const caps of title.matchAll(re)) {
      if (re === DASH_EP && DATE_TAIL.test(title.slice(0, caps.index))) continue
      const episode = Number.parseInt(caps[group], 10)
      if (!Number.isFinite(episode) || episode > U32_MAX) continue
      return `${normalizedSeries(title)}#${episode}`
    }
  }
  return null
}

// ---------------------------------------------------------------------------
// 规则编译与判定
// ---------------------------------------------------------------------------

/** 一条订阅的过滤规则（RssSourceDto 的投影）。 */
export interface RssFilterRule {
  include: string
  exclude: string
  useRegex: boolean
  smartEpisode: boolean
  /** 体积下限（字节，0 = 不限）。 */
  sizeMinBytes: number
  /** 体积上限（字节，0 = 不限）。 */
  sizeMaxBytes: number
}

type Matcher =
  /** 空表达式或非法正则：恒真。 */
  | { kind: 'any' }
  | { kind: 'regex'; re: RegExp }
  /** 外层 = `|` 分隔的或项，内层 = 空格分隔的与项（全小写）。 */
  | { kind: 'keywords'; alts: string[][] }

// 用户正则：引擎用 Rust `regex`（`(?i)` 前缀、Unicode 感知、无环视/反向引用），
// 预览用 JS RegExp。这里把 Rust 语义翻译过去：Rust 会编译失败的构造（环视、反向
// 引用）同样判无效；`\w` `\b` `\d` 按 Unicode 还原；`(?P<name>` 改写为 JS 命名组；
// 开头的 `(?ims-)` 标志组折算进 RegExp flags。返回 null = 引擎同样会编译失败
// （或无法在 JS 中等价表达），调用方按「非法正则放行」处理。
const WORD_INNER = '\\p{Alphabetic}\\p{M}\\p{Nd}\\p{Pc}\\p{Join_Control}'
const WORD_BOUNDARY = `(?:(?<=${WORD_CHAR})(?!${WORD_CHAR})|(?<!${WORD_CHAR})(?=${WORD_CHAR}))`
const NOT_WORD_BOUNDARY = `(?:(?<=${WORD_CHAR})(?=${WORD_CHAR})|(?<!${WORD_CHAR})(?!${WORD_CHAR}))`
// Rust 的 `\p{X}` 依次按二元属性 / 通用类别 / 脚本解析；JS u 模式不认裸脚本名，
// 故先试原名，再试 `Script=X`；都不合法即 Rust 同样会拒绝。
function resolveUnicodeProperty(name: string): string | null {
  const trimmed = name.trim()
  if (trimmed === '') return null
  const titled = trimmed.charAt(0).toUpperCase() + trimmed.slice(1).toLowerCase()
  for (const candidate of [trimmed, `Script=${trimmed}`, `Script=${titled}`]) {
    try {
      new RegExp(`\\p{${candidate}}`, 'u')
      return candidate
    } catch {
      // 换下一个候选
    }
  }
  return null
}


export function compileEngineRegex(source: string): RegExp | null {
  const flags = new Set(['i', 'u'])
  let i = 0
  const lead = /^\(\?([a-zA-Z-]+)\)/.exec(source)
  if (lead) {
    let on = true
    for (const c of lead[1]) {
      if (c === '-') on = false
      else if (c === 'i' || c === 's' || c === 'm') {
        if (on) flags.add(c)
        else flags.delete(c)
      } else return null
    }
    i = lead[0].length
  }
  let out = ''
  let inClass = false
  while (i < source.length) {
    const ch = source[i]
    if (ch === '\\') {
      const next = source[i + 1]
      if (next === undefined) return null
      i += 2
      if (!inClass && ((next >= '1' && next <= '9') || (next === 'k' && source[i] === '<'))) return null
      if (next === 'p' || next === 'P') {
        let name: string
        if (source[i] === '{') {
          const end = source.indexOf('}', i)
          if (end < 0) return null
          name = source.slice(i + 1, end)
          i = end + 1
        } else if (source[i] !== undefined) {
          name = source[i]
          i += 1
        } else return null
        const resolved = resolveUnicodeProperty(name)
        if (resolved === null) return null
        out += `\\${next}{${resolved}}`
        continue
      }
      if (next === 'b' && !inClass) out += WORD_BOUNDARY
      else if (next === 'B' && !inClass) out += NOT_WORD_BOUNDARY
      else if (next === 'w') out += inClass ? WORD_INNER : `[${WORD_INNER}]`
      else if (next === 'W' && !inClass) out += `[^${WORD_INNER}]`
      else if (next === 'd') out += '\\p{Nd}'
      else if (next === 'D') out += '\\P{Nd}'
      else out += ch + next
      continue
    }
    if (inClass) {
      if (ch === ']') inClass = false
      out += ch
      i += 1
      continue
    }
    if (ch === '[') {
      inClass = true
      out += ch
      i += 1
      continue
    }
    if (ch === '(' && source[i + 1] === '?') {
      const rest = source.slice(i + 2, i + 4)
      if (rest[0] === '=' || rest[0] === '!' || rest === '<=' || rest === '<!') return null
      if (rest[0] === 'P' && rest[1] === '<') {
        out += '(?<'
        i += 4
        continue
      }
      // 非开头的内联标志（`(?i)` / `(?i:`）无法在 JS 中等价表达。
      if (/^[a-zA-Z-]$/.test(rest[0] ?? '')) return null
    }
    out += ch
    i += 1
  }
  try {
    return new RegExp(out, [...flags].join(''))
  } catch {
    return null
  }
}

function buildMatcher(expr: string, useRegex: boolean): Matcher {
  const text = expr.trim()
  if (!text) return { kind: 'any' }
  if (useRegex) {
    // 非法正则一律放行：用户写错时宁可多下，也不静默漏下（qBittorrent
    // episodeFilter 写错即静默失配是明确的反面教训）。
    const re = compileEngineRegex(text)
    return re ? { kind: 'regex', re } : { kind: 'any' }
  }
  return {
    kind: 'keywords',
    alts: text.split('|').map((alt) => alt.split(/\s+/).filter(Boolean).map((w) => w.toLowerCase())),
  }
}

/** `lowered` 为调用方预先小写化的标题（避免逐 alt 重复分配）。 */
function matches(matcher: Matcher, title: string, lowered: string): boolean {
  switch (matcher.kind) {
    case 'any':
      return true
    case 'regex':
      return matcher.re.test(title)
    // 空的与项集合视为命中（`"a||b"` 中的空段）。
    case 'keywords':
      return matcher.alts.some((words) => words.every((w) => lowered.includes(w)))
  }
}

/** 预编译后的规则——正则只编译一次，供一整轮条目复用。 */
export interface CompiledRssRule {
  include: Matcher
  exclude: Matcher
  smartEpisode: boolean
  sizeMinBytes: number
  sizeMaxBytes: number
}

export function compileRule(rule: RssFilterRule): CompiledRssRule {
  return {
    include: buildMatcher(rule.include, rule.useRegex),
    exclude: buildMatcher(rule.exclude, rule.useRegex),
    smartEpisode: rule.smartEpisode,
    sizeMinBytes: Math.max(0, rule.sizeMinBytes),
    sizeMaxBytes: Math.max(0, rule.sizeMaxBytes),
  }
}

/** 单条目的判定结论。 */
export type RssVerdict =
  | { accepted: true; episodeKey: string }
  | { accepted: false; reason: RssRejectReason; episodeKey: string }

/**
 * 判定一个条目。`size <= 0` = 未知大小，跳过体积判定；`seen` 是**同源**已占用的
 * 剧集键集合，命中 Accept 且识别出剧集键时就地登记，使同一轮内的后续同集条目
 * 被判为重复。
 */
export function evaluate(rule: CompiledRssRule, title: string, size: number, seen: Set<string>): RssVerdict {
  const lowered = title.toLowerCase()
  if (!matches(rule.include, title, lowered)) {
    return { accepted: false, reason: 'not_included', episodeKey: '' }
  }
  if (rule.exclude.kind !== 'any' && matches(rule.exclude, title, lowered)) {
    return { accepted: false, reason: 'excluded', episodeKey: '' }
  }
  if (size > 0) {
    if (rule.sizeMinBytes > 0 && size < rule.sizeMinBytes) {
      return { accepted: false, reason: 'too_small', episodeKey: '' }
    }
    if (rule.sizeMaxBytes > 0 && size > rule.sizeMaxBytes) {
      return { accepted: false, reason: 'too_large', episodeKey: '' }
    }
  }
  let key = ''
  if (rule.smartEpisode) {
    const k = episodeKey(title)
    if (k !== null) {
      if (seen.has(k)) return { accepted: false, reason: 'dup_episode', episodeKey: k }
      seen.add(k)
      key = k
    }
  }
  return { accepted: true, episodeKey: key }
}

/** 一条预览结果：条目 + 判定。 */
export interface RssPreviewRow {
  item: RssItemDto
  verdict: RssVerdict
}

/**
 * 对一批已缓存条目整轮试跑规则。去重集合每次从空开始（不播种历史已下条目），
 * 因此预览回答的是「若这批条目现在重新抓一遍会怎样」——与引擎单轮内的语义一致。
 */
export function previewRule(rule: RssFilterRule, items: RssItemDto[]): RssPreviewRow[] {
  const compiled = compileRule(rule)
  const seen = new Set<string>()
  return items.map((item) => ({ item, verdict: evaluate(compiled, item.title, item.enclosureLength, seen) }))
}

/** 订阅源 → 过滤规则投影（对话框未保存时用表单值另行构造）。 */
export function ruleOf(source: RssSourceDto): RssFilterRule {
  return {
    include: source.includePattern,
    exclude: source.excludePattern,
    useRegex: source.useRegex,
    smartEpisode: source.smartEpisode,
    sizeMinBytes: source.sizeMinBytes,
    sizeMaxBytes: source.sizeMaxBytes,
  }
}

/**
 * 订阅显示名：未命名时退到 feed **主机名**而不是整条 URL——真实 feed 链接常带
 * token/passkey，铺在侧边栏既顶掉行内其它元素，也把凭证摊在屏幕上。
 * URL 解析不了（用户手输了半截）才退回原串。
 *
 * 侧边栏 / 条目流头部 / 管理对话框标题 / 删除确认文案共用这一处，避免同一条订阅
 * 在四个地方叫四个名字。
 */
export function sourceDisplayName(source: RssSourceDto): string {
  if (source.name) return source.name
  try {
    return new URL(source.url).host || source.url
  } catch {
    return source.url
  }
}
