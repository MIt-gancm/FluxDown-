// agent 自有 DTO（native/protocol/src/agent.rs）：FluxCloud 账号、网关、同步、跨设备任务、捕获、诊断等。
// 全部为服务端输出，字段均必带（`Option<T>` → `T | null`）。

import type { JsonValue } from './common';

// ── 账号 ──

export type CloudUserStatus = 'active' | 'disabled' | 'pending';

/** FluxCloud 用户公开资料。 */
export interface CloudUser {
  id: string;
  email: string;
  nickname: string;
  plan: string;
  status: CloudUserStatus;
  createdAt: string;
  lastLoginAt: string | null;
  /** Origin ID（数字身份号）。 */
  originId: number | null;
  originIdChanged: boolean;
  membershipOrdinal: number | null;
}

/** 套餐权益集合：前向兼容，未知键原样保留。 */
export type Entitlements = Record<string, JsonValue>;

export interface CloudPlanCampaignStage {
  label: string;
  priceMinor: number;
  quota: number | null;
}

export interface CloudPlanCampaign {
  name: string;
  endAt: string | null;
  stages: CloudPlanCampaignStage[];
  soldTotal: number;
  stageSold: number[];
  currentStageIndex: number;
  effectivePriceMinor: number;
}

/** 套餐（`agent.plan.list` 元素）。金额单位为「分」（`*Minor`）。 */
export interface CloudPlan {
  code: string;
  name: string;
  description: string;
  badge: string | null;
  icon: string;
  color: string;
  badgeStyle: string;
  badgeColor: string;
  badgeNumbered: boolean;
  badgeNumberDigits: number;
  priceMinor: number;
  currency: string;
  highlights: string[];
  /** wire 键名为 `entitlements`。 */
  entitlements: Record<string, JsonValue>;
  sort: number;
  campaign: CloudPlanCampaign | null;
  purchasable: boolean;
}

/** 当前账户资料：用户字段平铺在顶层，与 `entitlements` / `currentPlan` 同级。 */
export type CloudProfile = CloudUser & {
  entitlements: Entitlements;
  currentPlan: CloudPlan | null;
  purchaseCreditMinor: number;
};

export interface OriginIdCheckResult {
  available: boolean;
  reason: string | null;
}

/** 受信任设备公开投影。 */
export interface CloudDevice {
  id: string;
  deviceId: string;
  name: string;
  platform: string | null;
  createdAt: string;
  lastSeenAt: string;
  lastIp: string | null;
  appVersion: string | null;
  isOnline: boolean;
  isCurrent: boolean;
}

/** 无 token 的本地会话视图。 */
export interface AgentSessionDto {
  user: CloudUser;
  entitlements: Entitlements;
  currentPlan: CloudPlan | null;
  device: CloudDevice;
}

/**
 * 登录/注册类方法的结果（`status` 内部标记）。
 * `agent.auth.register` / `sendCode` 路径没有 token 时同样返回 `deviceVerificationRequired`
 * （此时 `ttlSeconds` 是验证码有效期）。
 */
export type AgentLoginResult =
  | { status: 'ok'; session: AgentSessionDto }
  | {
      status: 'deviceVerificationRequired';
      ttlSeconds: number;
      willReplaceDevices: boolean;
    };

// ── 订单 / 推介 ──

export interface CloudOrder {
  orderNo: string;
  planCode: string;
  planName: string;
  /** 默认 `pending`。 */
  status: string;
  amountMinor: number;
  listPriceMinor: number;
  creditMinor: number;
  upgradeFromPlan: string | null;
  currency: string;
  campaignName: string | null;
  stageLabel: string | null;
  referralCode: string | null;
  referralDiscountMinor: number;
  /** 微信 Native 支付二维码链接。 */
  codeUrl: string | null;
  createdAt: string;
  paidAt: string | null;
  expiresAt: string;
}

export interface CloudReferralRule {
  planCode: string;
  planName: string;
  priceMinor: number;
  discountMinor: number;
  rewardPercent: number;
}

export interface CloudReferralSummary {
  enabled: boolean;
  description: string;
  rewardEnabled: boolean;
  contact: string;
  invitedCount: number;
  pendingRewardMinor: number;
  paidRewardMinor: number;
  totalRewardMinor: number;
  rules: CloudReferralRule[];
}

export interface CloudReferralCode {
  id: string;
  code: string;
  paidOrderCount: number;
  rewardMinor: number;
  createdAt: string;
}

export interface CloudReferralCodesResult {
  total: number;
  items: CloudReferralCode[];
}

export interface CloudReferralRecord {
  id: string;
  buyerLabel: string;
  orderAmountMinor: number;
  rewardMinor: number;
  rewardPercent: number;
  status: string;
  createdAt: string;
  paidAt: string | null;
  referralCode: string | null;
}

export interface CloudReferralRecordsResult {
  total: number;
  items: CloudReferralRecord[];
}

export interface CloudReferralValidateResult {
  valid: boolean;
  discountMinor: number;
  reason: string | null;
}

// ── 跨设备任务 ──

/** 远端任务状态；未识别的值 Rust 端回退为 `pending`。 */
export type RemoteTaskStatus =
  | 'accepted'
  | 'downloading'
  | 'paused'
  | 'completed'
  | 'failed'
  | 'canceled'
  | 'pending';

export interface RemoteTaskDto {
  id: string;
  fromDevice: string;
  toDevice: string;
  url: string;
  saveDir: string | null;
  fileName: string;
  status: RemoteTaskStatus;
  totalBytes: number | null;
  downloadedBytes: number;
  speed: number;
  /** 0..1。 */
  progress: number;
  error: string | null;
  createdAt: string;
  updatedAt: string;
}

/** 已配对设备（P2P 互联）对外视图。`platform` 缺失时字段省略。 */
export interface LinkDeviceInfo {
  fingerprint: string;
  name: string;
  platform?: string;
  online: boolean;
  pairedAt: number;
  lastSeenAt: number;
}

// ── 网关 / 外壳 / 电源 / 同步 / 偏好 ──

/** UI Gateway 运行状态；永远不携带 token 文本（用 revealToken 获取）。 */
export interface GatewayStatusDto {
  takeoverEnabled: boolean;
  jsonrpcEnabled: boolean;
  apiEnabled: boolean;
  mcpEnabled: boolean;
  corsEnabled: boolean;
  userTokenConfigured: boolean;
  /** 当前监听端口（只读，默认 17800）。 */
  port: number;
  /** 是否对局域网开放兼容 API；修改后下次 agent 启动生效。 */
  lanEnabled: boolean;
}

export type TrayUnavailableReason = 'notBuilt' | 'noDisplay' | 'noHost' | 'initFailed';

/** agent 系统外壳状态。 */
export interface ShellStatusDto {
  trayAvailable: boolean;
  trayUnavailableReason: TrayUnavailableReason | null;
  /** true：关闭全部 UI 后 agent + daemon 继续驻留。 */
  resident: boolean;
}

/** 完成后关机状态。 */
export interface PowerStatusDto {
  /** 已排定的延迟（秒）；null = 未启用。 */
  armedDelaySecs: number | null;
  /** 全部任务结束、开始倒计时后的剩余秒数；等待任务完成阶段为 null。 */
  countdownRemainingSecs: number | null;
}

export interface SyncStatusDto {
  enabled: boolean;
  revision: number;
  dirtyKeys: string[];
  lastError: string | null;
}

/**
 * agent 自有偏好及其原子版本。`values` 的值类型随键而异（布尔/整数/浮点/字符串，
 * `custom_categories` 可为 JSON 字符串或数组）。
 */
export interface AgentPreferencesDto {
  revision: number;
  values: Record<string, JsonValue>;
}

/** 偏好键：自定义分类列表。 */
export const CUSTOM_CATEGORIES_PREF_KEY = 'custom_categories';

/** 自定义分类（与 Flutter `custom_categories` 同 JSON 形状）。 */
export interface CustomCategoryDto {
  id: string;
  name: string;
  icon: string;
  /** `extension` | `regex`。 */
  matchMode: string;
  extensions: string[];
  regexPattern: string;
  position: number;
  visible: boolean;
  isBuiltin: boolean;
  builtinType: string | null;
  saveDir: string;
}

// ── 外部捕获 ──

/** 等待官方 UI 确认的外部捕获（不含 cookie/header 原文，仅摘要）。 */
export interface PendingCaptureDto {
  transactionId: string;
  url: string;
  fileName: string;
  createdAtUnixMs: number;
  /** 捕获方声明的文件大小（字节，0 = 未知）。 */
  fileSize: number;
  referrer: string;
  /** 空 = 未指定。 */
  saveDir: string;
  hasCookies: boolean;
  /** 携带的请求头名（不含值）。 */
  headerNames: string[];
}

// ── 诊断 / 更新 ──

export type DiagnosticLevel = 'info' | 'ok' | 'warn' | 'error';

export interface DiagnosticRepairParams {
  /** `reregister` / `use_this_install` / `enable_service` / `register` / `open_log_dir` / `refreshTrackers` / `refreshEd2kServers`。 */
  action: string;
  target?: string;
}

export interface DiagnosticCheckDto {
  /** 稳定 ID，UI 据此选择标题文案与修复动作。 */
  id: string;
  target: string;
  level: DiagnosticLevel;
  detail: string;
  /** 既有 hint code（如 `HINT_CHECK_DISK`），无则为空。 */
  hint: string;
  /** 可用的就地修复动作；无则为 null。 */
  repair: DiagnosticRepairParams | null;
}

export interface DiagnosticsReportDto {
  generatedAtUnixMs: number;
  appVersion: string;
  platform: string;
  agentDataDir: string;
  daemonConnected: boolean;
  checks: DiagnosticCheckDto[];
}

export interface LogPathsDto {
  agentLogDir: string;
  /** daemon 不可达时为空串。 */
  daemonLogDir: string;
}

export interface ReleaseNoteDto {
  version: string;
  publishedAt: string;
  body: string;
}

export interface UpdateCheckResultDto {
  channel: string;
  currentVersion: string;
  latestVersion: string;
  hasUpdate: boolean;
  downloadUrl: string;
  releasePageUrl: string;
  notes: ReleaseNoteDto[];
}
