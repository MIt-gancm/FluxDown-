// Webhook 页：活动栏路由页（非设置分类）。对应 GPUI `crates/settings/src/webhook_view.rs`：
// 页面标题（webhookNavTitle + webhookEmptyDesc）+ hairline，下方滚动区两个分组——
// 端点（配置键 `webhook.endpoints`）与投递记录。

import { useCallback, useState } from 'react'
import { useT } from '../../i18n'
import { useAgent, useConnection } from '../../lib/rpc'
import { DeliveryLogGroup } from './DeliveryLogGroup'
import { EndpointsGroup } from './EndpointsGroup'
import { writeErrorText } from './write'
import type { RunWrite } from './write'

const selectDaemonConnected = (snapshot: { daemonConnected: boolean }) => snapshot.daemonConnected

export function WebhooksPage() {
  const t = useT()
  const phase = useConnection().phase
  const daemonConnected = useAgent(selectDaemonConnected, false)
  const [writeError, setWriteError] = useState<unknown>(null)

  // 未连上 agent / daemon：数据只读（GPUI `daemon_connected()`）。
  const disconnected = phase !== 'ready' || !daemonConnected

  const runWrite: RunWrite = useCallback(async (operation) => {
    setWriteError(null)
    try {
      await operation()
      return true
    } catch (error) {
      setWriteError(error)
      return false
    }
  }, [])

  const feedback = disconnected ? t('localServiceDisconnected') : writeError === null ? null : writeErrorText(writeError, t)

  return (
    <div className="flex h-full min-h-0 min-w-0 flex-col bg-surface text-foreground">
      <header className="flex-none border-b border-hairline px-6 pt-4 pb-3 mobile:px-4">
        <h1 className="text-title font-semibold text-foreground">{t('webhookNavTitle')}</h1>
        <p className="mt-0.5 text-xs text-muted-foreground">{t('webhookEmptyDesc')}</p>
      </header>
      {feedback ? (
        <div role="status" className="flex-none px-6 pt-3 text-xs text-destructive mobile:px-4">
          {feedback}
        </div>
      ) : null}
      <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-6 pt-5 pb-6 mobile:px-4 mobile:pt-4">
        <div className="mx-auto flex w-full min-w-0 max-w-4xl flex-col gap-4">
          <EndpointsGroup disabled={disconnected} runWrite={runWrite} />
          <DeliveryLogGroup disabled={disconnected} runWrite={runWrite} />
        </div>
      </div>
    </div>
  )
}
