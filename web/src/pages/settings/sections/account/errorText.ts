// RPC 错误 → 账户页可展示的 i18n 键。agent 只回传稳定错误码（服务端 message 不透传），
// 通用映射与 GPUI `crates/account/src/lib.rs::error_text` 一致；登录/注册/验证码场景按上下文细化，
// 使用 Flutter/GPUI 共用的 `accountError*` 文案。

import { RpcError } from '../../../../lib/rpc'

export type AccountErrorContext = 'generic' | 'login' | 'register' | 'code'

function genericKey(error: RpcError): string {
  switch (error.appCode) {
    case 'unavailable':
    case 'timeout':
      return 'localServiceDisconnected'
    case 'invalidArgument':
    case 'notFound':
      return 'localServiceInvalidArgument'
    case 'conflict':
      return 'localServiceConflict'
    case 'unsupported':
      return 'settingsUnsupportedOnPlatform'
    default:
      return 'localServiceActionFailed'
  }
}

export function accountErrorKey(error: unknown, context: AccountErrorContext = 'generic'): string {
  if (!(error instanceof RpcError)) return 'accountErrorUnknown'
  const code = error.appCode
  if (context === 'login') {
    if (code === 'unauthorized') return 'accountErrorInvalidCredentials'
    if (code === 'invalidArgument') return 'accountErrorValidation'
    if (code === 'conflict') return 'accountErrorRegistrationIncomplete'
    if (code === 'unavailable' && error.retryable) return 'accountErrorNetwork'
  } else if (context === 'register') {
    if (code === 'conflict') return 'accountErrorEmailTaken'
    if (code === 'invalidArgument') return 'accountErrorValidation'
    if (code === 'unsupported') return 'accountErrorRegistrationClosed'
    if (code === 'unavailable' && error.retryable) return 'accountErrorNetwork'
  } else if (context === 'code') {
    if (code === 'unauthorized' || code === 'invalidArgument' || code === 'notFound') return 'accountErrorInvalidCode'
    if (code === 'unavailable' && error.retryable) return 'accountErrorNetwork'
  }
  return genericKey(error)
}
