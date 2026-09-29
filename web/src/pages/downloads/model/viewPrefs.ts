// 下载页视图偏好（移植 crates/downloads/src/model/view_prefs.rs）。
// 全局单套、设备本地（`sync:false`），偏好键 `desktop.downloads.view`，JSON 形状与 GPUI 一致（snake_case）。

import { SMART_RANK } from './task'
import type { DownloadTaskView, TaskState } from './task'

export const VIEW_PREFS_KEY = 'desktop.downloads.view'

export type ViewDensity = 'comfortable' | 'compact'
export type ViewGroupBy = 'none' | 'status' | 'date' | 'type' | 'queue' | 'site' | 'group'
export type ViewSortKey = 'smart' | 'created' | 'name' | 'size' | 'progress' | 'speed'
export type SortDir = 'asc' | 'desc'
export type DetailPlacement = 'bottom' | 'right'

export const GROUP_BY_OPTIONS: readonly ViewGroupBy[] = ['none', 'status', 'date', 'type', 'queue', 'site', 'group']
export const SORT_KEY_OPTIONS: readonly ViewSortKey[] = ['smart', 'created', 'name', 'size', 'progress', 'speed']

export type ColumnKind =
  | 'file_name'
  | 'progress'
  | 'size'
  | 'speed'
  | 'eta'
  | 'status'
  | 'created'
  | 'protocol'
  | 'source'
  | 'queue'

/** 默认列顺序（`columns` 偏好为空或缺列时按此补齐）。 */
export const ALL_COLUMNS: readonly ColumnKind[] = [
  'file_name',
  'size',
  'progress',
  'status',
  'created',
  'speed',
  'eta',
  'protocol',
  'source',
  'queue',
]

/** 列名 i18n 键。 */
export const COLUMN_LABEL_KEY: Record<ColumnKind, string> = {
  file_name: 'colFileName',
  progress: 'colProgress',
  size: 'colSize',
  speed: 'colSpeed',
  eta: 'colEta',
  status: 'colStatus',
  created: 'colCreated',
  protocol: 'colProtocol',
  source: 'colSource',
  queue: 'colQueue',
}

const DEFAULT_VISIBLE = new Set<ColumnKind>(['file_name', 'size', 'progress', 'status', 'created'])

const DEFAULT_WIDTH: Record<ColumnKind, number> = {
  file_name: 240,
  progress: 150,
  size: 84,
  speed: 90,
  eta: 84,
  status: 140,
  created: 150,
  protocol: 64,
  source: 148,
  queue: 88,
}

export const MIN_WIDTH: Record<ColumnKind, number> = {
  file_name: 160,
  progress: 110,
  size: 64,
  speed: 72,
  eta: 64,
  status: 84,
  created: 140,
  protocol: 56,
  source: 96,
  queue: 72,
}

export const MAX_COLUMN_WIDTH = 480
export const FILE_NAME_MAX_WIDTH = 1600

/** 数字列：单元格与表头右对齐。 */
export const NUMERIC_COLUMNS = new Set<ColumnKind>(['size', 'speed', 'eta', 'created'])

/** 点击表头切换到的排序键；「状态」列回到智能排序。 */
export const COLUMN_SORT_KEY: Partial<Record<ColumnKind, ViewSortKey>> = {
  file_name: 'name',
  progress: 'progress',
  size: 'size',
  speed: 'speed',
  created: 'created',
  status: 'smart',
}

export interface ColumnPref {
  key: ColumnKind
  visible: boolean
  width: number
}

export interface ViewPrefs {
  density: ViewDensity
  group_by: ViewGroupBy
  sort_key: ViewSortKey
  sort_dir: SortDir
  /** 空 = 使用默认列集。 */
  columns: ColumnPref[]
  /** 文件名列拖过后的固定宽度；null = 吸收表格剩余宽度。 */
  file_name_width: number | null
  detail_placement: DetailPlacement
  detail_open: boolean
  detail_size: number
  sidebar_width: number
  collapsed_groups: string[]
}

export const DEFAULT_SIDEBAR_WIDTH = 200
export const SIDEBAR_WIDTH_RANGE = [176, 300] as const
export const DETAIL_SIZE_RANGE: Record<DetailPlacement, readonly [number, number]> = {
  bottom: [160, 480],
  right: [220, 560],
}

export function defaultViewPrefs(): ViewPrefs {
  return {
    density: 'comfortable',
    group_by: 'none',
    sort_key: 'smart',
    sort_dir: 'desc',
    columns: [],
    file_name_width: null,
    detail_placement: 'bottom',
    detail_open: false,
    detail_size: 260,
    sidebar_width: DEFAULT_SIDEBAR_WIDTH,
    collapsed_groups: [],
  }
}

function oneOf<T extends string>(values: readonly T[], raw: unknown, fallback: T): T {
  return typeof raw === 'string' && (values as readonly string[]).includes(raw) ? (raw as T) : fallback
}

function finite(raw: unknown, fallback: number): number {
  return typeof raw === 'number' && Number.isFinite(raw) ? raw : fallback
}

/** 反序列化；偏好为字符串（JSON 文本）或对象都接受，逐字段容错，非对象返回默认值。 */
export function parseViewPrefs(value: unknown): ViewPrefs {
  const defaults = defaultViewPrefs()
  let raw: unknown = value
  if (typeof raw === 'string') {
    try {
      raw = JSON.parse(raw)
    } catch {
      return defaults
    }
  }
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return defaults
  const json = raw as Record<string, unknown>
  const columns: ColumnPref[] = Array.isArray(json.columns)
    ? json.columns.flatMap((item): ColumnPref[] => {
        if (typeof item !== 'object' || item === null) return []
        const entry = item as Record<string, unknown>
        if (typeof entry.key !== 'string' || !(ALL_COLUMNS as readonly string[]).includes(entry.key)) return []
        return [{ key: entry.key as ColumnKind, visible: entry.visible !== false, width: finite(entry.width, 0) }]
      })
    : []
  return {
    density: oneOf(['comfortable', 'compact'], json.density, defaults.density),
    group_by: oneOf(GROUP_BY_OPTIONS, json.group_by, defaults.group_by),
    sort_key: oneOf(SORT_KEY_OPTIONS, json.sort_key, defaults.sort_key),
    sort_dir: oneOf(['asc', 'desc'], json.sort_dir, defaults.sort_dir),
    columns,
    file_name_width: typeof json.file_name_width === 'number' ? json.file_name_width : null,
    detail_placement: oneOf(['bottom', 'right'], json.detail_placement, defaults.detail_placement),
    detail_open: json.detail_open === true,
    detail_size: finite(json.detail_size, defaults.detail_size),
    sidebar_width: finite(json.sidebar_width, defaults.sidebar_width),
    collapsed_groups: Array.isArray(json.collapsed_groups)
      ? json.collapsed_groups.filter((item): item is string => typeof item === 'string')
      : [],
  }
}

/** 解析后的列配置（补齐缺列、至少一列可见）——GPUI `apply_column_prefs`。 */
export interface ResolvedColumn {
  kind: ColumnKind
  width: number
  visible: boolean
}

export function resolveColumns(prefs: readonly ColumnPref[]): ResolvedColumn[] {
  const make = (kind: ColumnKind): ResolvedColumn => ({
    kind,
    width: DEFAULT_WIDTH[kind],
    visible: DEFAULT_VISIBLE.has(kind),
  })
  if (prefs.length === 0) return ALL_COLUMNS.map(make)
  const columns = prefs.map((pref) => {
    const column = make(pref.key)
    column.visible = pref.visible
    if (pref.width >= MIN_WIDTH[pref.key]) column.width = pref.width
    return column
  })
  for (const kind of ALL_COLUMNS) {
    if (!columns.some((column) => column.kind === kind)) columns.push(make(kind))
  }
  if (!columns.some((column) => column.visible)) columns[0] = { ...(columns[0] as ResolvedColumn), visible: true }
  return columns
}

export function toColumnPrefs(columns: readonly ResolvedColumn[]): ColumnPref[] {
  return columns.map((column) => ({ key: column.kind, visible: column.visible, width: column.width }))
}

/** 首次切到该列排序时的方向：名称 A→Z，数值类从大到新。 */
export function defaultSortDir(key: ViewSortKey): SortDir {
  return key === 'name' ? 'asc' : 'desc'
}

/** 排序比较：`smart` = 状态优先级再创建时间倒序；其他键按 `sort_dir`。 */
export function compareViews(prefs: ViewPrefs, left: DownloadTaskView, right: DownloadTaskView): number {
  const tie = left.key < right.key ? -1 : left.key > right.key ? 1 : 0
  if (prefs.sort_key === 'smart') {
    return (
      SMART_RANK[left.state] - SMART_RANK[right.state] || right.createdAtSecs - left.createdAtSecs || tie
    )
  }
  let order = 0
  switch (prefs.sort_key) {
    case 'created':
      order = left.createdAtSecs - right.createdAtSecs
      break
    case 'name': {
      const a = left.name.toLowerCase()
      const b = right.name.toLowerCase()
      order = a < b ? -1 : a > b ? 1 : 0
      break
    }
    case 'size':
      order = left.sizeBytes - right.sizeBytes
      break
    case 'progress':
      order = left.progress - right.progress
      break
    case 'speed':
      order = (left.speed ?? 0) - (right.speed ?? 0)
      break
  }
  return (prefs.sort_dir === 'asc' ? order : -order) || tie
}

/** 日期分组桶（本地日期）。顺序即分组顺序。 */
export type DateBucket = 'today' | 'yesterday' | 'this_week' | 'this_month' | 'older'
export const DATE_BUCKET_ORDER: Record<DateBucket, number> = {
  today: 0,
  yesterday: 1,
  this_week: 2,
  this_month: 3,
  older: 4,
}

function isoWeekKey(date: Date): string {
  // ISO 周：以周四所在年份 + 周序号标识。
  const d = new Date(Date.UTC(date.getFullYear(), date.getMonth(), date.getDate()))
  const dayNum = d.getUTCDay() || 7
  d.setUTCDate(d.getUTCDate() + 4 - dayNum)
  const yearStart = Date.UTC(d.getUTCFullYear(), 0, 1)
  const week = Math.ceil(((d.getTime() - yearStart) / 86_400_000 + 1) / 7)
  return `${d.getUTCFullYear()}-${week}`
}

export function dateBucketOf(createdAtSecs: number, now: Date = new Date()): DateBucket {
  if (!createdAtSecs || createdAtSecs <= 0) return 'older'
  const created = new Date(createdAtSecs * 1000)
  const startOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime()
  const today = startOf(now)
  const day = startOf(created)
  if (day >= today) return 'today'
  const yesterday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime()
  if (day === yesterday) return 'yesterday'
  if (isoWeekKey(created) === isoWeekKey(now)) return 'this_week'
  if (created.getMonth() === now.getMonth() && created.getFullYear() === now.getFullYear()) return 'this_month'
  return 'older'
}

/** 状态分组键（稳定，用于折叠记忆）。 */
export const stateGroupKey = (state: TaskState): string => state

/** 搜索：名称 / url / 站点（小写包含）。 */
export function matchesQuery(view: DownloadTaskView, query: string): boolean {
  if (query === '') return true
  return (
    view.name.toLowerCase().includes(query) ||
    view.url.toLowerCase().includes(query) ||
    view.site.toLowerCase().includes(query)
  )
}
