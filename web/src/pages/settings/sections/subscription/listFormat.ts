// 列表型配置键的存储格式（GPUI `subscription::ListFormat`）。

export type ListFormat = 'lines' | 'comma'

/** 存储值中的非空条目（lines 按行；comma 按逗号/空白，兼容换行旧值）。 */
export function listEntries(format: ListFormat, stored: string): string[] {
  const parts = format === 'lines' ? stored.split(/[\n\r]/) : stored.split(/[,\s]+/)
  return parts.map((part) => part.trim()).filter((part) => part !== '')
}

/** 条目 → 存储值；comma 大小写不敏感去重，lines 原样按行拼接。 */
export function listToStored(format: ListFormat, entries: readonly string[]): string {
  if (format === 'lines') return entries.join('\n')
  const seen = new Set<string>()
  return entries
    .filter((entry) => {
      const key = entry.toLowerCase()
      if (seen.has(key)) return false
      seen.add(key)
      return true
    })
    .join(',')
}

/** Unix 秒 → 本地时间 `YYYY-MM-DD HH:MM`。 */
export function formatUnix(secs: number): string {
  const d = new Date(secs * 1000)
  if (Number.isNaN(d.getTime())) return ''
  const p = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}`
}
