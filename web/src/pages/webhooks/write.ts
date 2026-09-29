// 配置写回的共享类型与错误文案。

import { RpcError, errorMessage } from '../../lib/rpc'
import type { TFunction } from '../../i18n'

/** 配置写回入口：成功返回 `true`，失败由页面顶部反馈条展示并返回 `false`。 */
export type RunWrite = (operation: () => Promise<void>) => Promise<boolean>

/** 写回失败 → 反馈文案（与 GPUI `SettingsErrorKind::i18n_key` 一致；校验失败附带具体原因）。 */
export function writeErrorText(error: unknown, t: TFunction): string {
  const code = error instanceof RpcError ? error.appCode : undefined
  switch (code) {
    case 'unavailable':
      return t('localServiceDisconnected')
    case 'conflict':
      return t('localServiceConflict')
    case 'invalidArgument': {
      const detail = errorMessage(error)
      return detail ? `${t('localServiceInvalidArgument')}: ${detail}` : t('localServiceInvalidArgument')
    }
    default:
      return t('localServiceActionFailed')
  }
}
