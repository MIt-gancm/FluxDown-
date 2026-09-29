// RPC 错误：JSON-RPC 错误对象 + 稳定应用错误详情（RpcErrorData），或传输层失败。

import type { ApplicationErrorCode, ErrorReason, RpcErrorData } from './protocol'

export interface RpcErrorInit {
  /** JSON-RPC code（应用错误恒为 -32000；传输层失败用本地约定值）。 */
  code: number
  message: string
  data?: RpcErrorData
  /** 请求所属方法名，便于日志。 */
  method?: string
}

/** 传输层失败（未连接、超时、断线）使用的 JSON-RPC code，仅本地使用。 */
export const TRANSPORT_ERROR_CODE = -32001

export class RpcError extends Error {
  readonly code: number
  readonly data: RpcErrorData | undefined
  readonly method: string | undefined

  constructor(init: RpcErrorInit) {
    super(init.message)
    this.name = 'RpcError'
    this.code = init.code
    this.data = init.data
    this.method = init.method
  }

  /** 稳定应用错误码；传输层失败合成为 `unavailable` / `timeout`。 */
  get appCode(): ApplicationErrorCode | undefined {
    return this.data?.code
  }

  get retryable(): boolean {
    return this.data?.retryable ?? false
  }

  get reason(): ErrorReason | undefined {
    return this.data?.reason ?? undefined
  }

  /** `daemon.config.patch` 冲突时服务端回带的最新 revision。 */
  get revision(): number | undefined {
    return this.data?.revision ?? undefined
  }

  get field(): string | undefined {
    return this.data?.field ?? undefined
  }

  is(code: ApplicationErrorCode): boolean {
    return this.data?.code === code
  }
}

export function transportError(message: string, appCode: 'unavailable' | 'timeout', method?: string): RpcError {
  return new RpcError({
    code: TRANSPORT_ERROR_CODE,
    message,
    data: { code: appCode, retryable: true },
    method,
  })
}

/** 任意抛出值 → 可展示文本（`{e:#}` 风格：优先 Error.message）。 */
export function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return String(error)
}
