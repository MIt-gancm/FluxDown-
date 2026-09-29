// 移植 crates/downloads/src/pages/task_detail_activity.rs：历史页与实时事件按源端 ID 合并的持久游标。

import type { TaskActivityDto, TaskActivityPage, TaskActivityQuery } from '../../../lib/rpc'

type Fetch = 'latest' | 'after' | 'before'

export interface Ticket {
  generation: number
  serial: number
  fetch: Fetch
  query: TaskActivityQuery
}

export class ActivityFeed {
  private generation = 0
  private serial = 0
  private map = new Map<number, TaskActivityDto>()
  private cursor = 0
  private isLoaded = false
  private older = false
  private oldest: number | null = null
  private newest: number | null = null
  private isTruncated = false
  private gap = false
  private pending: Fetch | null = 'latest'
  private inFlight: Ticket | null = null
  private failedFetch: Fetch | null = null
  /** 事件可能先于页 RPC 的结果到达；页的游标只能由查询结果推进。 */
  private liveDuringFetch = 0

  constructor(private taskId: string) {}

  private reset(): void {
    this.serial = 0
    this.map = new Map()
    this.cursor = 0
    this.isLoaded = false
    this.older = false
    this.oldest = null
    this.newest = null
    this.isTruncated = false
    this.gap = false
    this.pending = 'latest'
    this.inFlight = null
    this.failedFetch = null
    this.liveDuringFetch = 0
  }

  /** 升序（id 小 → 大）。 */
  entries(): TaskActivityDto[] {
    return [...this.map.values()].sort((a, b) => a.id - b.id)
  }
  isLoading = (): boolean => this.inFlight !== null
  loaded = (): boolean => this.isLoaded
  failed = (): boolean => this.failedFetch !== null
  hasOlder = (): boolean => this.isLoaded && this.older
  retainedRange = (): { oldest: number | null; newest: number | null; truncated: boolean } => ({
    oldest: this.oldest,
    newest: this.newest,
    truncated: this.isTruncated,
  })
  hasJournalGap = (): boolean => this.gap

  private switchTask(): void {
    this.generation += 1
    this.reset()
  }

  suspend(): void {
    this.generation += 1
    this.inFlight = null
    this.liveDuringFetch = 0
  }

  reconnect(): void {
    this.suspend()
    this.pending = this.isLoaded ? 'after' : 'latest'
    this.failedFetch = null
  }

  loadOlder(): void {
    if (this.older && this.inFlight === null && this.pending === null) {
      this.pending = 'before'
      this.failedFetch = null
    }
  }

  retry(): void {
    if (this.failedFetch !== null) {
      this.pending = this.failedFetch
      this.failedFetch = null
    }
  }

  add(entry: TaskActivityDto): boolean {
    if (entry.taskId !== this.taskId || entry.id <= 0) return false
    if (entry.kind === 'journal_overflow') this.gap = true
    const changed = !this.map.has(entry.id)
    this.map.set(entry.id, entry)
    if (this.inFlight !== null && this.inFlight.fetch !== 'before') {
      this.liveDuringFetch = Math.max(this.liveDuringFetch, entry.id)
    }
    return changed
  }

  begin(): Ticket | null {
    if (this.inFlight !== null || this.failedFetch !== null) return null
    const fetch = this.pending
    if (fetch === null) return null
    this.pending = null
    let beforeId: number | null = null
    if (fetch === 'before') {
      const first = this.entries()[0]
      if (!first) {
        this.older = false
        return null
      }
      beforeId = first.id
    }
    this.serial += 1
    const ticket: Ticket = {
      generation: this.generation,
      serial: this.serial,
      fetch,
      query: {
        taskId: this.taskId,
        beforeId,
        afterId: fetch === 'after' ? this.cursor : null,
        limit: 0,
      },
    }
    this.liveDuringFetch = 0
    this.inFlight = ticket
    return ticket
  }

  /** 只接受本任务本世代的结果；查询期间到达的实时事件永不被页覆盖。 */
  finish(ticket: Ticket, page: TaskActivityPage | null): boolean {
    const cur = this.inFlight
    if (!cur || cur.generation !== ticket.generation || cur.serial !== ticket.serial || cur.query.taskId !== ticket.query.taskId) {
      return false
    }
    this.inFlight = null
    if (page === null) {
      this.failedFetch = ticket.fetch
      return true
    }
    this.failedFetch = null
    if (ticket.fetch === 'after' && this.cursor > 0 && (page.newestId === null || page.newestId < this.cursor)) {
      // 数据库被重建或该任务的保留记录全部过期；旧游标不属于新历史。
      this.switchTask()
      return true
    }
    this.oldest = page.oldestId
    this.newest = page.newestId
    this.isTruncated ||= page.truncated
    if (this.oldest !== null) {
      const oldest = this.oldest
      const first = this.entries()[0]
      if (first && first.id < oldest) this.isTruncated = true
      for (const id of [...this.map.keys()]) if (id < oldest) this.map.delete(id)
    }
    const lastPageId = page.entries.length > 0 ? page.entries[page.entries.length - 1]!.id : 0
    for (const entry of page.entries) {
      if (entry.taskId === this.taskId && entry.id > 0) {
        if (entry.kind === 'journal_overflow') this.gap = true
        this.map.set(entry.id, entry)
      }
    }
    switch (ticket.fetch) {
      case 'latest':
        this.isLoaded = true
        this.older = page.hasMore
        this.cursor = Math.max(this.cursor, lastPageId)
        if (this.liveDuringFetch > this.cursor) this.pending = 'after'
        break
      case 'after':
        // 不能以实时事件的最大 ID 推进游标：中间尚未取回的记录会被跳过。
        this.cursor = Math.max(this.cursor, lastPageId)
        if (page.hasMore && lastPageId <= (ticket.query.afterId ?? 0)) this.failedFetch = 'after'
        else if (page.hasMore || this.liveDuringFetch > this.cursor) this.pending = 'after'
        break
      case 'before':
        this.older = page.hasMore
        break
    }
    this.liveDuringFetch = 0
    return true
  }
}
