// bun:test 的最小类型声明（未引入 @types/bun；仅覆盖本项目单测用到的 API）。
declare module 'bun:test' {
  interface Matchers {
    toBe(expected: unknown): void
    toEqual(expected: unknown): void
    toBeNull(): void
    toBeUndefined(): void
    toBeTruthy(): void
    toBeFalsy(): void
    toThrow(expected?: unknown): void
    not: Matchers
  }
  export function describe(name: string, body: () => void): void
  export function test(name: string, body: () => void | Promise<void>): void
  export function it(name: string, body: () => void | Promise<void>): void
  export function expect(actual: unknown): Matchers
}
