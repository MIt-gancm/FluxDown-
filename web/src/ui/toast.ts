// 全局 toast：模块级队列 + 单个宿主组件（AppProviders 挂一次）。
// 任何位置（含非组件代码：RPC 回调、事件处理）都可 `toast.error(...)`。
// 语义：toast 只承载「已发生、无需回应」的一次性反馈；需要用户决策请用 confirmDialog。

import { t } from '../i18n'
import { errorMessage } from '../lib/rpc/error'

export type ToastKind = 'info' | 'success' | 'warning' | 'error'

export interface ToastItem {
  id: number
  text: string
  kind: ToastKind
}

const MAX_VISIBLE = 3
/** 报错停留更久：用户要读完文本才知道下一步做什么。 */
const DURATION_MS: Record<ToastKind, number> = { info: 3000, success: 3000, warning: 4500, error: 5000 }

let items: readonly ToastItem[] = []
let nextId = 1
const listeners = new Set<() => void>()
const timers = new Map<number, ReturnType<typeof setTimeout>>()

function publish(next: readonly ToastItem[]) {
  items = next
  for (const listener of listeners) listener()
}

export function dismissToast(id: number) {
  const timer = timers.get(id)
  if (timer !== undefined) clearTimeout(timer)
  timers.delete(id)
  publish(items.filter((item) => item.id !== id))
}

function push(text: string, kind: ToastKind): void {
  const id = nextId++
  let next = [...items, { id, text, kind }]
  while (next.length > MAX_VISIBLE) {
    const dropped = next[0]
    if (dropped) {
      const timer = timers.get(dropped.id)
      if (timer !== undefined) clearTimeout(timer)
      timers.delete(dropped.id)
    }
    next = next.slice(1)
  }
  publish(next)
  timers.set(
    id,
    setTimeout(() => dismissToast(id), DURATION_MS[kind]),
  )
}

export const toast = {
  info: (text: string) => push(text, 'info'),
  success: (text: string) => push(text, 'success'),
  warning: (text: string) => push(text, 'warning'),
  /** 传 Error / RpcError / 字符串；可加前缀文案（如「保存失败」）。 */
  error: (error: unknown, prefix?: string) => {
    const detail = errorMessage(error)
    push(prefix ? `${prefix}: ${detail}` : detail, 'error')
  },
  /** 用 i18n 键弹提示。 */
  key: (key: string, kind: ToastKind = 'info', params?: Record<string, string | number>) => push(t(key, params), kind),
}

export function subscribeToasts(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

export const readToasts = (): readonly ToastItem[] => items
