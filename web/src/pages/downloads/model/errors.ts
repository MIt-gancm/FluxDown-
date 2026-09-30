// 下载页统一的 RPC 错误 toast：RpcError 按稳定 appCode 取本地化文案，
// 不把传输层的英文诊断串（如 `request timed out: daemon.task.pause`）直接上屏。

import { t } from '../../../i18n'
import { toast } from '../../../ui'
import { rpcErrorText } from '../../settings/kit/errors'

export function toastRpcError(error: unknown): void {
  toast.error(rpcErrorText(error, t))
}
