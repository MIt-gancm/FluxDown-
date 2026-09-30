// 统一的 RPC 错误 toast：各页面的动作失败都经此本地化后上屏。

import { t } from '../i18n'
import { toast } from '../ui'
import { rpcErrorText } from './rpcErrorText'

export function toastRpcError(error: unknown): void {
  toast.error(rpcErrorText(error, t))
}
