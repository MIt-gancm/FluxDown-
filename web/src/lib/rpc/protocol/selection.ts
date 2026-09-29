// 交互选择（HLS 画质 / BT 文件 / 插件变体）。
// 流程：`daemon.selection.subscribe` → 收到 `selectionPending` 事件 → `daemon.selection.resolve`；
// 超过 `deadlineUnixMs` 服务端按 `defaultChoice` 自动决定。

export interface HlsQualityOptionDto {
  /** 用于 `SelectionOutcome.hls.index`。 */
  index: number;
  bandwidth: number;
  width: number;
  height: number;
}

export interface BtFileDto {
  index: number;
  path: string;
  size: number;
}

export interface ResolveVariantOptionDto {
  index: number;
  label: string;
  container: string;
  bandwidth: number;
  width: number;
  height: number;
  totalBytes: number;
}

/** 选择种类，`type` 内部标记。 */
export type SelectionKind =
  | { type: 'hls'; options: HlsQualityOptionDto[] }
  | { type: 'bt'; files: BtFileDto[] }
  | { type: 'variant'; options: ResolveVariantOptionDto[] };

/**
 * 选择结果，`kind` 内部标记。`bt.indices` 空数组 = 全部文件；
 * `cancelled` 对 HLS 会被拒绝（invalidArgument）。
 */
export type SelectionOutcome =
  | { kind: 'hls'; index: number }
  | { kind: 'bt'; indices: number[] }
  | { kind: 'variant'; index: number }
  | { kind: 'cancelled' };

/** 待选择请求（快照 `pendingSelections` / `selectionPending` 事件）。 */
export interface SelectionRequestDto {
  requestId: string;
  taskId: string;
  kind: SelectionKind;
  /** 超时自动采用的默认选择。 */
  defaultChoice: SelectionOutcome;
  deadlineUnixMs: number;
}

/** `daemon.selection.resolve` 参数。 */
export interface SelectionResolutionDto {
  requestId: string;
  outcome: SelectionOutcome;
}
