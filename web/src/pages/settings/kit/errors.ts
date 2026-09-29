// RpcError → 可展示文案（对应 GPUI 设置页错误映射：按稳定错误码取 i18n 键）。

import type { TFunction } from '../../../i18n'
import { RpcError, errorMessage } from '../../../lib/rpc'
import { SETTINGS_ERROR_KEYS } from './writeStore'
import type { SettingsErrorKind } from './writeStore'

function kindOfCode(error: RpcError): SettingsErrorKind {
  switch (error.appCode) {
    case 'unavailable':
    case 'timeout':
      return 'disconnected'
    case 'conflict':
      return 'conflict'
    case 'invalidArgument':
      return 'invalidArgument'
    default:
      return 'failed'
  }
}

/**
 * 动作失败文案：`localService*` 通用键；invalidArgument 追加服务端细节。
 * 非 RpcError（如上传/网络异常）取其 message。
 */
export function rpcErrorText(error: unknown, t: TFunction): string {
  if (!(error instanceof RpcError)) return errorMessage(error)
  const kind = kindOfCode(error)
  const base = t(SETTINGS_ERROR_KEYS[kind])
  return kind === 'invalidArgument' && error.message ? `${base}: ${error.message}` : base
}
