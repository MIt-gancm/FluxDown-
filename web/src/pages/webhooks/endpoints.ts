// Webhook 端点模型与配置读写：daemon 配置键 `webhook.endpoints`（JSON 数组字符串），
// 与 `engine::webhook::EndpointSpec` 同 wire 形状（camelCase）。

import { RpcError, rpc, rpcStore } from '../../lib/rpc'
import type { WebhookDeliveryDto } from '../../lib/rpc'

export const ENDPOINTS_KEY = 'webhook.endpoints'

/** 与 GPUI `MAX_CONFLICT_RETRIES` 一致：修订冲突时的自动重试上限。 */
const MAX_CONFLICT_RETRIES = 3

export interface EndpointSpec {
  id: string
  name: string
  preset: string
  url: string
  enabled: boolean
  events: string[]
  queueId: string
  headers: Record<string, string>
  bodyTemplate: string
  signSecret: string
  allowHttp: boolean
  useProxy: boolean
}

/** 事件 wire 名（与引擎 `WebhookEventKind::wire()` 逐字一致）及其 i18n 键。 */
export const WEBHOOK_EVENTS = [
  { wire: 'task.created', labelKey: 'webhookEventCreated' },
  { wire: 'task.started', labelKey: 'webhookEventStarted' },
  { wire: 'task.completed', labelKey: 'webhookEventCompleted' },
  { wire: 'task.failed', labelKey: 'webhookEventFailed' },
  { wire: 'task.paused', labelKey: 'webhookEventPaused' },
  { wire: 'queue.drained', labelKey: 'webhookEventQueueDrained' },
] as const

const str = (value: unknown): string => (typeof value === 'string' ? value : '')
const bool = (value: unknown, fallback: boolean): boolean => (typeof value === 'boolean' ? value : fallback)

function parseEndpoint(raw: unknown): EndpointSpec | null {
  if (typeof raw !== 'object' || raw === null || Array.isArray(raw)) return null
  const item = raw as Record<string, unknown>
  const headers: Record<string, string> = {}
  if (typeof item.headers === 'object' && item.headers !== null && !Array.isArray(item.headers)) {
    for (const [key, value] of Object.entries(item.headers)) {
      if (typeof value === 'string') headers[key] = value
    }
  }
  return {
    id: str(item.id),
    name: str(item.name),
    preset: str(item.preset),
    url: str(item.url),
    // serde `default_true`：缺省启用。
    enabled: bool(item.enabled, true),
    events: Array.isArray(item.events) ? item.events.filter((event): event is string => typeof event === 'string') : [],
    queueId: str(item.queueId),
    headers,
    bodyTemplate: str(item.bodyTemplate),
    signSecret: str(item.signSecret),
    allowHttp: bool(item.allowHttp, false),
    useProxy: bool(item.useProxy, false),
  }
}

/** 配置串 → 端点列表；空串 / 非法 JSON 视为空列表（与 GPUI `read_endpoints` 一致）。 */
export function parseEndpoints(raw: string | undefined): EndpointSpec[] {
  if (!raw || raw.trim() === '') return []
  try {
    const parsed: unknown = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.map(parseEndpoint).filter((entry): entry is EndpointSpec => entry !== null)
  } catch {
    return []
  }
}

/**
 * 读-改-写端点列表。`mutate` 返回 `null` 表示无需写入。
 * 每次都基于最新快照重新计算，`expectedRevision` 冲突时重新 `config.get` 后再算一遍
 * （最多 3 次），避免覆盖别处（另一浏览器 / GPUI 客户端）的并发修改。
 * 写入串行化：连续操作不会互相制造冲突。
 */
let writeChain: Promise<unknown> = Promise.resolve()

export function patchEndpoints(mutate: (list: EndpointSpec[]) => EndpointSpec[] | null): Promise<void> {
  const run = async (): Promise<void> => {
    let config = rpcStore.peek().snapshot?.daemon.config
    for (let attempt = 0; ; attempt += 1) {
      if (!config) config = await rpc.daemon.config.get()
      const next = mutate(parseEndpoints(config.values[ENDPOINTS_KEY]))
      if (next === null) return
      try {
        await rpc.daemon.config.patch({
          expectedRevision: config.revision,
          values: { [ENDPOINTS_KEY]: JSON.stringify(next) },
        })
        return
      } catch (error) {
        if (error instanceof RpcError && error.is('conflict') && attempt < MAX_CONFLICT_RETRIES) {
          config = await rpc.daemon.config.get()
          continue
        }
        throw error
      }
    }
  }
  const result = writeChain.then(run, run)
  writeChain = result.catch(() => undefined)
  return result
}

/** 新增或按 id 覆盖。 */
export const upsertEndpoint = (draft: EndpointSpec) =>
  patchEndpoints((list) => {
    const index = list.findIndex((entry) => entry.id === draft.id)
    if (index < 0) return [...list, draft]
    return list.map((entry, i) => (i === index ? draft : entry))
  })

export const setEndpointEnabled = (id: string, enabled: boolean) =>
  patchEndpoints((list) => {
    if (!list.some((entry) => entry.id === id)) return null
    return list.map((entry) => (entry.id === id ? { ...entry, enabled } : entry))
  })

export const removeEndpoint = (id: string) => patchEndpoints((list) => list.filter((entry) => entry.id !== id))

/** 端点的最近一次投递（按时间戳最大）。 */
export function latestDelivery(deliveries: readonly WebhookDeliveryDto[], endpointId: string): WebhookDeliveryDto | undefined {
  let latest: WebhookDeliveryDto | undefined
  for (const delivery of deliveries) {
    if (delivery.endpointId !== endpointId) continue
    if (!latest || delivery.timestampMs > latest.timestampMs) latest = delivery
  }
  return latest
}

export const NO_DELIVERIES: readonly WebhookDeliveryDto[] = []
export const selectDeliveries = (daemon: { webhookDeliveries: WebhookDeliveryDto[] }) => daemon.webhookDeliveries
