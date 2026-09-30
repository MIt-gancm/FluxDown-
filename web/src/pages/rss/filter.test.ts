// 前端过滤副本与 native/engine/src/rss/filter.rs 的一致性（用例取自 Rust 单测）。
import { describe, expect, test } from 'bun:test'
import { compileRule, episodeKey, evaluate, formatSize, parseSize } from './filter'
import type { RssFilterRule, RssVerdict } from './filter'

const MB = 1024 * 1024
const rule = (over: Partial<RssFilterRule> = {}) =>
  compileRule({ include: '', exclude: '', useRegex: false, smartEpisode: false, sizeMinBytes: 0, sizeMaxBytes: 0, ...over })
const verdictOf = (v: RssVerdict) => (v.accepted ? 'accepted' : v.reason)

describe('rss filter parity with engine', () => {
  test('体积字面量与往返', () => {
    expect(parseSize('200 mb')).toBe(200 * MB)
    expect(parseSize('1.5G')).toBe(1536 * MB)
    for (const bad of ['', '   ', 'abc', '12X', '-5M']) expect(parseSize(bad)).toBeNull()
    for (const bytes of [1024, 200 * MB, 3 * 1024 * MB, 1_234_567]) expect(parseSize(formatSize(bytes))).toBe(bytes)
  })

  test('体积上下限只对已知大小生效', () => {
    const r = rule({ sizeMinBytes: 200 * MB, sizeMaxBytes: 2048 * MB })
    const seen = new Set<string>()
    expect(verdictOf(evaluate(r, 'ok', 500 * MB, seen))).toBe('accepted')
    expect(verdictOf(evaluate(r, 'small', 10 * MB, seen))).toBe('too_small')
    expect(verdictOf(evaluate(r, 'huge', 40 * 1024 * MB, seen))).toBe('too_large')
    expect(verdictOf(evaluate(r, 'unknown', 0, seen))).toBe('accepted')
  })

  test('剧集键：四种格式、跨字幕组归一、分辨率不当集号', () => {
    for (const title of ['Show S01E02 1080p', 'Show 1x02 1080p', '[ANi] Show - 02 [1080P]', '[字幕组] 番名 第02话 [1080P]']) {
      expect(episodeKey(title)).not.toBeNull()
    }
    const ani = episodeKey('[ANi] 幼女战记 2 - 02 [1080P][Baha][WEB-DL][AAC AVC][CHT][MP4]')
    expect(ani).toBe(episodeKey('[桜都字幕组] 幼女战记 2 - 02 [720P][简体内嵌]'))
    expect(ani?.endsWith('#2')).toBe(true)
    expect(episodeKey('Some Movie 1920x1080 BluRay')).toBeNull()
    expect(episodeKey('Clip 3840x2160')).toBeNull()
    expect(episodeKey('[VCB-Studio] 紫罗兰永恒花园 [Ma10p_1080p][合集]')).toBeNull()
  })

  test('智能去重：先到先得，默认关闭，识别失败放行', () => {
    const on = rule({ smartEpisode: true })
    const seen = new Set<string>()
    expect(verdictOf(evaluate(on, '[ANi] 幼女战记 2 - 02 [1080P]', 0, seen))).toBe('accepted')
    expect(verdictOf(evaluate(on, '[桜都字幕组] 幼女战记 2 - 02 [720P]', 0, seen))).toBe('dup_episode')
    expect(verdictOf(evaluate(on, '[ANi] 幼女战记 2 - 03 [1080P]', 0, seen))).toBe('accepted')
    const off = new Set<string>()
    expect(verdictOf(evaluate(rule(), '[A] X - 02', 0, off))).toBe('accepted')
    expect(verdictOf(evaluate(rule(), '[B] X - 02', 0, off))).toBe('accepted')
    expect(off.size).toBe(0)
    const none = new Set<string>()
    expect(verdictOf(evaluate(on, '[VCB] 合集 A', 0, none))).toBe('accepted')
    expect(verdictOf(evaluate(on, '[VCB] 合集 A', 0, none))).toBe('accepted')
  })

  test('智能剧集：ISO 日期分量不当集号（与 Rust 同例）', () => {
    expect(episodeKey('Daily Show 2025-09-27 1080p')).toBeNull()
    expect(episodeKey('Daily Show 2025-09 1080p')).toBeNull()
    expect(episodeKey('Daily Show 2025-09-27 - 12 1080p')?.endsWith('#12')).toBe(true)
  })

  test('用户正则：Rust 不支持的构造判无效（放行），Unicode 语义对齐', () => {
    // 环视：Rust 编译失败 → Matcher::Any，全部放行。
    expect(verdictOf(evaluate(rule({ include: '^(?!.*720p).*1080p', useRegex: true }), 'x 720p 1080p', 0, new Set()))).toBe('accepted')
    expect(verdictOf(evaluate(rule({ include: '(a)\\1', useRegex: true }), 'zzz', 0, new Set()))).toBe('accepted')
    // 开头内联标志：Rust 生效。
    expect(verdictOf(evaluate(rule({ exclude: '(?-i)RAW', useRegex: true }), 'a raw b', 0, new Set()))).toBe('accepted')
    expect(verdictOf(evaluate(rule({ exclude: '(?-i)RAW', useRegex: true }), 'a RAW b', 0, new Set()))).toBe('excluded')
    // \p{Han}：Rust 按字符类匹配。
    expect(verdictOf(evaluate(rule({ include: '\\p{Han}', useRegex: true }), '幼女战记', 0, new Set()))).toBe('accepted')
    expect(verdictOf(evaluate(rule({ include: '\\p{Han}', useRegex: true }), 'abc', 0, new Set()))).toBe('not_included')
    // \b 是 Unicode 感知的：「记」与 S 之间不构成边界。
    const b = rule({ include: '\\bS01E02\\b', useRegex: true })
    expect(verdictOf(evaluate(b, '[X]幼女战记S01E02[1080P]', 0, new Set()))).toBe('not_included')
    expect(verdictOf(evaluate(b, '[X] S01E02 [1080P]', 0, new Set()))).toBe('accepted')
  })
})
