// 端点分组：列表（读取 daemon 配置 `webhook.endpoints`）+ 新增按钮 + 测试结果。

import { Webhook } from 'lucide-react'
import { useMemo, useState } from 'react'
import { useT } from '../../i18n'
import { cn } from '../../lib/cn'
import { errorMessage, rpc, useConfigValue, useDaemon } from '../../lib/rpc'
import { Button, EmptyState, confirmDialog } from '../../ui'
import { EndpointDialog } from './EndpointDialog'
import { EndpointRow } from './EndpointRow'
import { ENDPOINTS_KEY, NO_DELIVERIES, parseEndpoints, removeEndpoint, selectDeliveries, setEndpointEnabled } from './endpoints'
import type { EndpointSpec } from './endpoints'
import { WebhookGroup } from './WebhookGroup'
import type { RunWrite } from './write'

interface TestReport {
  success: boolean
  text: string
}

export function EndpointsGroup({ disabled, runWrite }: { disabled: boolean; runWrite: RunWrite }) {
  const t = useT()
  const raw = useConfigValue(ENDPOINTS_KEY)
  const endpoints = useMemo(() => parseEndpoints(raw), [raw])
  const deliveries = useDaemon(selectDeliveries, NO_DELIVERIES)

  // `null` = 关闭；`'new'` = 新增；否则编辑该端点。
  const [editing, setEditing] = useState<EndpointSpec | 'new' | null>(null)
  const [testingId, setTestingId] = useState<string | null>(null)
  const [testReport, setTestReport] = useState<TestReport | null>(null)

  const test = async (endpoint: EndpointSpec) => {
    if (testingId !== null) return
    setTestingId(endpoint.id)
    try {
      const response = await rpc.daemon.webhook.test({ ...endpoint })
      setTestReport(
        response.success
          ? { success: true, text: t('webhookTestOk', { status: response.statusCode, ms: response.latencyMs }) }
          : { success: false, text: t('webhookTestFail', { error: response.error }) },
      )
    } catch (error) {
      setTestReport({ success: false, text: t('webhookTestFail', { error: errorMessage(error) }) })
    } finally {
      setTestingId(null)
    }
  }

  const remove = async (endpoint: EndpointSpec) => {
    const ok = await confirmDialog({
      title: t('webhookRowDeleteConfirm'),
      description: endpoint.name,
      intent: 'destructive',
      okLabel: t('webhookRowDelete'),
    })
    if (ok) await runWrite(() => removeEndpoint(endpoint.id))
  }

  return (
    <WebhookGroup title={t('notifyGroupWebhook')} subtitle={t('webhookSemantics')}>
      <div className="flex w-full flex-col gap-0.5">
        {endpoints.length === 0 ? (
          <EmptyState icon={Webhook} title={t('webhookEmptyTitle')} description={t('webhookEmptyDesc')} />
        ) : null}
        {endpoints.map((endpoint) => (
          <EndpointRow
            key={endpoint.id}
            endpoint={endpoint}
            deliveries={deliveries}
            disabled={disabled}
            testing={testingId === endpoint.id}
            testBusy={testingId !== null}
            onToggle={(enabled) => void runWrite(() => setEndpointEnabled(endpoint.id, enabled))}
            onEdit={() => setEditing(endpoint)}
            onTest={() => void test(endpoint)}
            onDelete={() => void remove(endpoint)}
          />
        ))}
        {testReport ? (
          <div className={cn('px-2 pt-1 text-xs', testReport.success ? 'text-success' : 'text-destructive')}>{testReport.text}</div>
        ) : null}
        <div className="flex w-full justify-end pt-2">
          <Button variant="primary" disabled={disabled} onClick={() => setEditing('new')}>
            {t('webhookAddEndpoint')}
          </Button>
        </div>
      </div>
      {editing !== null ? (
        <EndpointDialog
          key={editing === 'new' ? 'new' : editing.id}
          existing={editing === 'new' ? null : editing}
          runWrite={runWrite}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </WebhookGroup>
  )
}
