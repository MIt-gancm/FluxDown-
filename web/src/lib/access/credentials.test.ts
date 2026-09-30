import { beforeAll, describe, expect, test } from 'bun:test'
import type * as Credentials from './credentials'

class MemoryStorage {
  private readonly data = new Map<string, string>()
  getItem(key: string): string | null {
    return this.data.get(key) ?? null
  }
  setItem(key: string, value: string): void {
    this.data.set(key, value)
  }
  removeItem(key: string): void {
    this.data.delete(key)
  }
}

const session = new MemoryStorage()
const local = new MemoryStorage()
let credentials: typeof Credentials

beforeAll(async () => {
  Object.assign(globalThis, { sessionStorage: session, localStorage: local })
  // 模块加载时即读取 window/存储，必须先装好桩再动态加载。
  credentials = await import('./credentials')
})

describe('credentials', () => {
  test('updateStoredToken 保留原存储位置；未登录不写入', () => {
    credentials.clearToken()
    credentials.updateStoredToken('x')
    expect(credentials.getToken()).toBe('')

    credentials.saveToken('old', true)
    credentials.updateStoredToken('new')
    expect(local.getItem('fluxdown.web.token')).toBe('new')
    expect(session.getItem('fluxdown.web.token')).toBeNull()

    credentials.saveToken('old', false)
    credentials.updateStoredToken('new2')
    expect(session.getItem('fluxdown.web.token')).toBe('new2')
    expect(local.getItem('fluxdown.web.token')).toBeNull()
  })

  test('clearToken 清除两处存储', () => {
    credentials.saveToken('a', true)
    credentials.clearToken()
    expect(credentials.isAuthenticated()).toBe(false)
  })
})
