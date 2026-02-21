import '../bindings/bindings.dart';

/// 命名下载队列的 Dart 侧模型（对应 Rust QueueInfo）
class DownloadQueue {
  final String queueId;
  final String name;

  /// 速度限制（KB/s），0 = 不限制
  final int speedLimitKbps;

  /// 同时下载任务数，0 = 使用全局设置
  final int maxConcurrent;

  /// 默认保存目录，空 = 使用全局设置
  final String defaultSaveDir;

  /// 显示顺序（从小到大）
  final int position;

  /// 每任务默认线程数（HTTP 分段连接数），0 = 自动（全局 segment_advisor）
  final int defaultSegments;

  /// 此队列任务使用的默认 User-Agent，空字符串 = 继承全局 UA
  final String defaultUserAgent;

  const DownloadQueue({
    required this.queueId,
    required this.name,
    required this.speedLimitKbps,
    required this.maxConcurrent,
    required this.defaultSaveDir,
    required this.position,
    this.defaultSegments = 0,
    this.defaultUserAgent = '',
  });

  factory DownloadQueue.fromQueueInfo(QueueInfo info) {
    return DownloadQueue(
      queueId: info.queueId,
      name: info.name,
      speedLimitKbps: info.speedLimitKbps,
      maxConcurrent: info.maxConcurrent,
      defaultSaveDir: info.defaultSaveDir,
      position: info.position,
      defaultSegments: info.defaultSegments,
      defaultUserAgent: info.defaultUserAgent,
    );
  }

  DownloadQueue copyWith({
    String? queueId,
    String? name,
    int? speedLimitKbps,
    int? maxConcurrent,
    String? defaultSaveDir,
    int? position,
    int? defaultSegments,
    String? defaultUserAgent,
  }) {
    return DownloadQueue(
      queueId: queueId ?? this.queueId,
      name: name ?? this.name,
      speedLimitKbps: speedLimitKbps ?? this.speedLimitKbps,
      maxConcurrent: maxConcurrent ?? this.maxConcurrent,
      defaultSaveDir: defaultSaveDir ?? this.defaultSaveDir,
      position: position ?? this.position,
      defaultSegments: defaultSegments ?? this.defaultSegments,
      defaultUserAgent: defaultUserAgent ?? this.defaultUserAgent,
    );
  }
}
