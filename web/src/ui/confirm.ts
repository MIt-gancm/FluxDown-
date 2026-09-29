// 命令式确认框：`if (await confirmDialog({...})) ...`。宿主组件 ConfirmHost 由 AppProviders 挂一次。

import type { DialogIntent } from './Dialog'

export interface ConfirmOptions {
  title: string
  description?: string
  okLabel?: string
  cancelLabel?: string
  intent?: DialogIntent
}

export interface Pending extends ConfirmOptions {
  resolve: (ok: boolean) => void
}

let pending: Pending | null = null
const listeners = new Set<() => void>()

function publish(next: Pending | null) {
  pending = next
  for (const listener of listeners) listener()
}

export function confirmDialog(options: ConfirmOptions): Promise<boolean> {
  // 同时只有一个确认框：新请求到来时把旧的当作取消。
  pending?.resolve(false)
  return new Promise<boolean>((resolve) => publish({ ...options, resolve }))
}

export function settleConfirm(ok: boolean) {
  const current = pending
  publish(null)
  current?.resolve(ok)
}

export function subscribeConfirm(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}
export const readConfirm = (): Pending | null => pending

