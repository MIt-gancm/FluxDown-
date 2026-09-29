// Webhook 任务事件推送 DTO。

import type { JsonValue } from './common';

/** 一条投递记录（新的在前，最多 100 条，内存环形缓冲不落盘）。 */
export interface WebhookDeliveryDto {
  deliveryId: string;
  /** Unix 毫秒。 */
  timestampMs: number;
  /** 事件 wire 名（`task.completed` 等）。 */
  event: string;
  endpointId: string;
  endpointName: string;
  url: string;
  /** 请求头摘录，每行 `K: V`；鉴权类值已掩码。 */
  requestHeaders: string;
  requestBody: string;
  /** HTTP 状态码；0 = 未拿到响应。 */
  statusCode: number;
  responseBody: string;
  latencyMs: number;
  attempts: number;
  success: boolean;
  error: string;
}

/** 服务预设元数据（前端只做占位符替换预览）。 */
export interface WebhookPresetDto {
  id: string;
  label: string;
  urlPlaceholder: string;
  defaultTemplate: string;
  contentType: string;
}

export interface WebhookDeliveriesResponse {
  deliveries: WebhookDeliveryDto[];
  presets: WebhookPresetDto[];
  /** 可用占位符（`{task.fileName}` 等）。 */
  variables: string[];
}

/**
 * 测试投递参数：直接内嵌端点草稿对象（无需先保存），
 * schema 同配置键 `webhook.endpoints` 数组元素（服务端展平为顶层字段）。
 */
export type WebhookEndpointDraft = Record<string, JsonValue>;

export interface WebhookTestResponse {
  success: boolean;
  /** HTTP 状态码；0 = 未拿到响应。 */
  statusCode: number;
  latencyMs: number;
  /** 成功为空。 */
  error: string;
}

export interface WebhookSimulateResponse {
  /** 投出的端点数；0 = 没有端点订阅 `task.completed`。 */
  dispatched: number;
}
