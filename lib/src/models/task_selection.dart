/// 多选范围计算的纯函数 —— 供 [DownloadController.modifierTapTask] 与
/// 桌面端框选（marquee）共用的 Shift 区间语义，独立成纯函数以便直接单测
/// （DownloadController 依赖 rinf FFI 无法在测试中实例化）。
library;

/// 返回 [anchorId] 与 [targetId] 在 [orderedIds] 中的闭区间（按
/// [orderedIds] 的顺序排列，与调用方传入 anchor/target 的先后顺序无关）。
///
/// - [targetId] 不在 [orderedIds] 中：无法确定任何选区，返回空列表。
/// - 仅 [anchorId] 不在 [orderedIds] 中（[targetId] 存在）：锚点失效，退化
///   为只选中 [targetId]。
List<String> taskSelectionRange(
  List<String> orderedIds,
  String anchorId,
  String targetId,
) {
  final targetIndex = orderedIds.indexOf(targetId);
  if (targetIndex < 0) return const [];
  final anchorIndex = orderedIds.indexOf(anchorId);
  if (anchorIndex < 0) return [targetId];
  final start = anchorIndex < targetIndex ? anchorIndex : targetIndex;
  final end = anchorIndex < targetIndex ? targetIndex : anchorIndex;
  return orderedIds.sublist(start, end + 1);
}
