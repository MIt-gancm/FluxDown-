// 访问密钥存储：「记住此设备」→ localStorage，否则 sessionStorage。同源部署，不存 base URL。

import { useSyncExternalStore } from 'react'

const TOKEN_KEY = 'fluxdown.web.token'

type Listener = () => void
const listeners = new Set<Listener>()

function emit() {
  for (const listener of listeners) listener()
}

export function getToken(): string {
  return sessionStorage.getItem(TOKEN_KEY) ?? localStorage.getItem(TOKEN_KEY) ?? ''
}

export function isAuthenticated(): boolean {
  return getToken() !== ''
}

export function saveToken(token: string, remember: boolean): void {
  clearStorage()
  ;(remember ? localStorage : sessionStorage).setItem(TOKEN_KEY, token)
  emit()
}

/** 密钥在服务端被改后就地替换，保留原「记住此设备」选择；未登录不写入。 */
export function updateStoredToken(token: string): void {
  const store = sessionStorage.getItem(TOKEN_KEY) !== null ? sessionStorage : localStorage
  if (store.getItem(TOKEN_KEY) === null) return
  store.setItem(TOKEN_KEY, token)
  emit()
}

function clearStorage() {
  sessionStorage.removeItem(TOKEN_KEY)
  localStorage.removeItem(TOKEN_KEY)
}

export function clearToken(): void {
  clearStorage()
  emit()
}

function subscribe(listener: Listener) {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** 响应式登录态。 */
export function useIsAuthenticated(): boolean {
  return useSyncExternalStore(subscribe, isAuthenticated, isAuthenticated)
}
