import 'dart:async';

import 'package:flutter/foundation.dart';
import 'package:rinf/rinf.dart';

import '../bindings/bindings.dart';
import '../services/log_service.dart';
import 'download_queue.dart';
import 'download_task.dart';

const _tag = 'DownloadCtrl';

/// 顶部 Tab 状态筛选
enum StatusTab { all, downloading, completed, paused, error, seeding }

/// 核心状态管理器 — 桥接 Rust 信号和 Flutter UI
class DownloadController extends ChangeNotifier {
  /// 全局单例引用，供无 context 场景（如 ExternalDownloadService）读取队列信息
  static DownloadController? globalInstance;
  static Completer<DownloadController>? _globalInstanceCompleter;

  final List<DownloadTask> _tasks = [];
  FileCategory _categoryFilter = FileCategory.all;
  StatusTab _statusTab = StatusTab.all;

  /// 命名队列列表（来自 Rust AllQueues 信号）
  List<DownloadQueue> _queues = [];
  final Completer<void> _queuesLoadedCompleter = Completer<void>();

  /// 当前队列筛选 ID：null = 不过滤（显示全部），'' = 默认队列，非空 = 指定命名队列
  String? _queueFilter;

  // 缓存 — 避免 filteredTasks / groupedTasks 每次访问重新计算
  List<DownloadTask>? _cachedFilteredTasks;
  List<TaskGroup>? _cachedGroupedTasks;

  /// 已删除任务 ID 集合 — 防止 Rust 残余信号将已删除任务「复活」到列表中。
  /// 在 _onAllTasks 刷新时清空（Rust 端不再包含该任务即可安全移除）。
  final Set<String> _deletedTaskIds = {};

  /// 用户主动暂停的任务 ID 集合（乐观暂停）。
  /// 守卫：阻止 _onAllTasks 从 DB 覆盖 UI 暂停状态，以及阻止积压的 downloading
  /// 信号将 UI 改回下载中。只在 resumeTask / deleteTask 时移除。
  final Set<String> _optimisticPausedIds = {};

  /// 下载完成回调 — 当任务状态从非 completed 变为 completed 时触发
  void Function(DownloadTask task)? onTaskCompleted;

  /// 当前 Boost 优先任务 ID（空字符串 = 无优先任务）
  String _priorityTaskId = '';

  /// 因 Boost 被加入 _optimisticPausedIds 的任务 ID 集合。
  /// 用于 boost 取消时精确清理守卫条目，避免遗漏或误删。
  final Set<String> _boostAutoPausedIds = {};

  /// 延迟恢复队列：resumeAll 时超出并发限制的任务 ID 在此排队，
  /// 当活跃任务完成/出错时由 _resumeNextDeferred() 逐个发送到 Rust。
  final List<String> _deferredResumeQueue = [];

  StreamSubscription<RustSignalPack<TaskProgress>>? _progressSub;
  StreamSubscription<RustSignalPack<AllTasks>>? _allTasksSub;
  StreamSubscription<RustSignalPack<SegmentProgress>>? _segmentSub;
  StreamSubscription<RustSignalPack<SegmentSplitEvent>>? _splitSub;
  StreamSubscription<RustSignalPack<TaskCdnEvent>>? _cdnEventSub;
  StreamSubscription<RustSignalPack<TaskMetaProbed>>? _metaProbedSub;
  StreamSubscription<RustSignalPack<QueuePositionsUpdate>>? _queuePosSub;
  StreamSubscription<RustSignalPack<AllQueues>>? _allQueuesSub;
  StreamSubscription<RustSignalPack<PriorityTaskChanged>>? _prioritySub;
  StreamSubscription<RustSignalPack<FileMissingChanged>>? _fileMissingSub;
  StreamSubscription<RustSignalPack<TaskQueueChanged>>? _taskQueueChangedSub;
  StreamSubscription<RustSignalPack<TaskRouteChanged>>? _taskRouteChangedSub;

  bool _disposed = false;

  DownloadController({bool requestInitialState = true}) {
    logInfo(_tag, 'constructor — starting listeners');
    globalInstance = this;
    final instanceWaiter = _globalInstanceCompleter;
    if (instanceWaiter != null && !instanceWaiter.isCompleted) {
      instanceWaiter.complete(this);
    }
    _globalInstanceCompleter = null;
    _startListening();
    if (requestInitialState) {
      requestPersistedState();
    }
  }

  /// 向引擎请求全量任务 / 队列 / 组快照。构造时与移动端回前台都会走这里。
  void requestPersistedState() {
    const RequestAllTasks().sendSignalToRust();
    const RequestAllQueues().sendSignalToRust();
  }

  @override
  void dispose() {
    logInfo(_tag, 'dispose called');
    _disposed = true;
    if (!_queuesLoadedCompleter.isCompleted) {
      _queuesLoadedCompleter.complete();
    }
    if (globalInstance == this) globalInstance = null;
    _progressSub?.cancel();
    _allTasksSub?.cancel();
    _segmentSub?.cancel();
    _splitSub?.cancel();
    _cdnEventSub?.cancel();
    _metaProbedSub?.cancel();
    _queuePosSub?.cancel();
    _allQueuesSub?.cancel();
    _prioritySub?.cancel();
    _fileMissingSub?.cancel();
    _taskQueueChangedSub?.cancel();
    _taskRouteChangedSub?.cancel();
    super.dispose();
    logInfo(_tag, 'dispose done');
  }

  /// 安全的 notifyListeners — dispose 后不再通知，避免
  /// "A DownloadController was used after being disposed" 异常
  void _safeNotifyListeners() {
    _cachedFilteredTasks = null;
    _cachedGroupedTasks = null;
    if (!_disposed) notifyListeners();
  }

  // ---------------------------------------------------------------------------
  // Public getters
  // ---------------------------------------------------------------------------

  List<DownloadTask> get tasks => _tasks;

  FileCategory get categoryFilter => _categoryFilter;
  StatusTab get statusTab => _statusTab;

  /// 命名队列列表（已按 position 排序）
  List<DownloadQueue> get queues => _queues;

  /// 当前队列筛选（null = 不过滤，'' = 默认队列，非空 = 指定命名队列）
  String? get queueFilter => _queueFilter;

  /// 按队列 ID 过滤
  List<DownloadTask> get _queueFiltered {
    if (_queueFilter == null) return _tasks;
    return _tasks.where((t) => t.queueId == _queueFilter).toList();
  }

  /// 按文件类型过滤（在队列过滤基础上叠加）
  List<DownloadTask> get _categoryFiltered {
    final byQueue = _queueFiltered;
    if (_categoryFilter == FileCategory.all) return byQueue;
    return byQueue.where((t) => t.fileCategory == _categoryFilter).toList();
  }

  /// 双维度组合过滤后的任务列表（侧边栏文件类型 + 顶部状态 Tab）
  List<DownloadTask> get filteredTasks {
    if (_cachedFilteredTasks != null) return _cachedFilteredTasks!;
    final byCategory = _categoryFiltered;
    final result = switch (_statusTab) {
      StatusTab.all => byCategory,
      StatusTab.downloading =>
        byCategory
            .where(
              (t) =>
                  t.status == TaskStatus.downloading ||
                  t.status == TaskStatus.pending ||
                  t.status == TaskStatus.preparing ||
                  t.status == TaskStatus.resuming,
            )
            .toList(),
      StatusTab.completed =>
        byCategory.where((t) => t.status == TaskStatus.completed).toList(),
      StatusTab.paused =>
        byCategory.where((t) => t.status == TaskStatus.paused).toList(),
      StatusTab.error =>
        byCategory.where((t) => t.status == TaskStatus.error).toList(),
      StatusTab.seeding => byCategory.where((t) => t.isSeeding).toList(),
    };
    _cachedFilteredTasks = result;
    return result;
  }

  /// 将 filteredTasks 分组：活跃+排队任务置顶，历史任务按时间分组
  List<TaskGroup> get groupedTasks {
    if (_cachedGroupedTasks != null) return _cachedGroupedTasks!;
    final tasks = filteredTasks;

    // "全部" 和 "下载中" Tab：活跃+排队任务组置顶，历史任务按时间分组
    late final List<TaskGroup> result;
    if (_statusTab == StatusTab.all || _statusTab == StatusTab.downloading) {
      final activeTasks = tasks.where((t) => t.status.isActiveOrQueued).toList()
        ..sort(_compareActiveTasks);
      final historicalTasks = tasks
          .where((t) => !t.status.isActiveOrQueued)
          .toList();
      result = [
        if (activeTasks.isNotEmpty) TaskGroup(group: null, tasks: activeTasks),
        ..._buildTimeGroups(historicalTasks),
      ];
    } else {
      result = _buildTimeGroups(tasks);
    }
    _cachedGroupedTasks = result;
    return result;
  }

  List<TaskGroup> _buildTimeGroups(List<DownloadTask> tasks) {
    final Map<TimeGroup, List<DownloadTask>> map = {};
    for (final task in tasks) {
      (map[TimeGroup.fromDateTime(task.createdAt)] ??= []).add(task);
    }
    return [
      for (final g in TimeGroup.values)
        if (map[g] != null && map[g]!.isNotEmpty)
          TaskGroup(group: g, tasks: map[g]!),
    ];
  }

  int _compareActiveTasks(DownloadTask a, DownloadTask b) {
    int priority(TaskStatus s) => switch (s) {
      TaskStatus.downloading => 0,
      TaskStatus.preparing => 1,
      TaskStatus.resuming => 1,
      TaskStatus.pending => 2,
      _ => 3,
    };
    final diff = priority(a.status).compareTo(priority(b.status));
    if (diff != 0) return diff;
    // pending：按队列位置升序（位置仅在入队/出队时变化，稳定）
    if (a.status == TaskStatus.pending) {
      return a.queuePosition.compareTo(b.queuePosition);
    }
    // 活跃任务（downloading/preparing/resuming）：按创建时间升序，顺序稳定不抖动
    return a.createdAt.compareTo(b.createdAt);
  }

  /// 在当前文件类型筛选下，各状态的任务数量（用于 Tab 显示计数）
  int filteredCountForStatus(StatusTab tab) {
    final byCategory = _categoryFiltered;
    return switch (tab) {
      StatusTab.all => byCategory.length,
      StatusTab.downloading =>
        byCategory
            .where(
              (t) =>
                  t.status == TaskStatus.downloading ||
                  t.status == TaskStatus.pending ||
                  t.status == TaskStatus.preparing ||
                  t.status == TaskStatus.resuming,
            )
            .length,
      StatusTab.completed =>
        byCategory.where((t) => t.status == TaskStatus.completed).length,
      StatusTab.paused =>
        byCategory.where((t) => t.status == TaskStatus.paused).length,
      StatusTab.error =>
        byCategory.where((t) => t.status == TaskStatus.error).length,
      StatusTab.seeding => byCategory.where((t) => t.isSeeding).length,
    };
  }

  /// 当前 Boost 优先任务 ID（空字符串 = 无优先任务）
  String get priorityTaskId => _priorityTaskId;

  /// 统计数据
  int get downloadingCount =>
      _tasks.where((t) => t.status == TaskStatus.downloading).length;
  int get pendingCount =>
      _tasks.where((t) => t.status == TaskStatus.pending).length;
  int get preparingCount =>
      _tasks.where((t) => t.status == TaskStatus.preparing).length;
  int get resumingCount =>
      _tasks.where((t) => t.status == TaskStatus.resuming).length;
  int get activeCount =>
      downloadingCount + pendingCount + preparingCount + resumingCount;

  /// 全局下载速度
  int get totalDownloadSpeed {
    int sum = 0;
    for (final t in _tasks) {
      if (t.status == TaskStatus.downloading) sum += t.speed;
    }
    return sum;
  }

  // ---------------------------------------------------------------------------
  // Actions — 发送信号到 Rust
  // ---------------------------------------------------------------------------

  void createTask({
    required String url,
    required String saveDir,
    String fileName = '',
    int segments = 0,
    String cookies = '',
    Uint8List? torrentFileBytes,
    String proxyUrl = '',
    String userAgent = '',
    String queueId = '',
    String checksum = '',
    bool ignoreTlsErrors = false,
    Map<String, String> extraHeaders = const {},
    List<int> selectedFileIndices = const [],
    bool startPaused = false,
    String httpUser = '',
    String httpPassword = '',
    bool saveSiteAuth = false,
  }) {
    logInfo(
      _tag,
      'createTask: url=$url, dir=$saveDir, file=$fileName, seg=$segments, cookies_len=${cookies.length}, torrent_bytes=${torrentFileBytes?.length ?? 0}, queue=$queueId, headers=${extraHeaders.length}, selected_files=${selectedFileIndices.length}, later=$startPaused',
    );
    CreateTask(
      url: url,
      saveDir: saveDir,
      fileName: fileName,
      segments: segments,
      cookies: cookies,
      torrentFileBytes: torrentFileBytes ?? Uint8List(0),
      proxyUrl: proxyUrl,
      userAgent: userAgent,
      queueId: queueId,
      checksum: checksum,
      ignoreTlsErrors: ignoreTlsErrors,
      extraHeaders: extraHeaders,
      selectedFileIndices: selectedFileIndices,
      startPaused: startPaused,
      httpUser: httpUser,
      httpPassword: httpPassword,
      saveSiteAuth: saveSiteAuth,
    ).sendSignalToRust();
  }

  /// 乐观更新：将做种中任务标记为用户暂停。
  DownloadTask _pauseSeederOptimistically(DownloadTask t) => t.copyWith(
    status: TaskStatus.completed,
    seedingStatus: SeedingStatus.userStopped,
    uploadSpeedBps: 0,
  );

  /// 乐观更新：将用户暂停的做种任务恢复为做种中。
  DownloadTask _resumeSeederOptimistically(DownloadTask t) => t.copyWith(
    status: TaskStatus.completed,
    seedingStatus: SeedingStatus.seeding,
  );

  void pauseTask(String taskId) {
    logInfo(_tag, 'pauseTask: $taskId');
    _optimisticPausedIds.add(taskId);
    // 乐观更新：立即切换到 paused 状态，防止用户快速重复点击
    final idx = _tasks.indexWhere((t) => t.id == taskId);
    if (idx >= 0) {
      final t = _tasks[idx];
      // 仅对活跃状态的任务执行暂停
      if (t.status == TaskStatus.downloading ||
          t.status == TaskStatus.resuming ||
          t.status == TaskStatus.pending ||
          t.status == TaskStatus.preparing) {
        _tasks[idx] = t.copyWith(status: TaskStatus.paused, speed: 0);
        _safeNotifyListeners();
      } else if (t.isSeeding) {
        // 做种中的 BT 任务：保持 completed 状态，仅将做种状态切为 userStopped。
        _tasks[idx] = _pauseSeederOptimistically(t);
        _safeNotifyListeners();
      }
    }
    ControlTask(taskId: taskId, action: 0).sendSignalToRust();
  }

  void resumeTask(String taskId) {
    logInfo(_tag, 'resumeTask: $taskId');
    _optimisticPausedIds.remove(taskId);
    _boostAutoPausedIds.remove(taskId);
    // 立即切换到 resuming 状态，让 UI 即时响应
    final idx = _tasks.indexWhere((t) => t.id == taskId);
    if (idx >= 0) {
      final t = _tasks[idx];
      if (t.status == TaskStatus.completed && t.isSeedingStopped) {
        // 做种已停止（用户暂停/限制达标）：保持 completed，仅恢复做种状态。
        _tasks[idx] = _resumeSeederOptimistically(t);
      } else {
        _tasks[idx] = _tasks[idx].copyWith(status: TaskStatus.resuming);
      }
      _safeNotifyListeners();
    }
    ControlTask(taskId: taskId, action: 1).sendSignalToRust();
  }

  /// 删除任务。[deleteFiles] 为 true 时同时删除磁盘上的已下载文件。
  void deleteTask(String taskId, {bool deleteFiles = true}) {
    logInfo(_tag, 'deleteTask: $taskId, deleteFiles=$deleteFiles');
    _optimisticPausedIds.remove(taskId);
    _boostAutoPausedIds.remove(taskId);
    _deferredResumeQueue.remove(taskId);
    final action = deleteFiles ? 3 : 4;
    ControlTask(taskId: taskId, action: action).sendSignalToRust();
    _deletedTaskIds.add(taskId);
    _tasks.removeWhere((t) => t.id == taskId);
    _safeNotifyListeners();
  }

  void setCategoryFilter(FileCategory category) {
    if (_categoryFilter == category) return;
    _categoryFilter = category;
    _safeNotifyListeners();
  }

  void setStatusTab(StatusTab tab) {
    if (_statusTab == tab) return;
    _statusTab = tab;
    _safeNotifyListeners();
  }

  /// 设置队列筛选。
  ///
  /// **不做「再点一次取消」**：侧边栏每个分区恒有一个激活项（见
  /// [syncSidebarFilters]），能取消到「没有任何高亮」只会让人以为筛选仍在
  /// 生效却找不到它在哪。想看别的队列就点别的队列。
  void setQueueFilter(String? queueId) {
    if (_queueFilter == queueId) return;
    _queueFilter = queueId;
    _safeNotifyListeners();
  }

  // ---------------------------------------------------------------------------
  // Queue CRUD — 发送信号到 Rust
  // ---------------------------------------------------------------------------

  void moveTaskToQueue(String taskId, String queueId) {
    logInfo(_tag, 'moveTaskToQueue: task=$taskId, queue=$queueId');
    MoveTaskToQueue(taskId: taskId, queueId: queueId).sendSignalToRust();
  }

  /// 按 ID 查找队列（不存在返回 null）。
  DownloadQueue? queueById(String queueId) {
    for (final q in _queues) {
      if (q.queueId == queueId) return q;
    }
    return null;
  }

  /// 队列是否运行中。空 ID / 未知队列视作运行中（防御）。
  bool isQueueRunning(String queueId) {
    if (queueId.isEmpty) return true;
    return queueById(queueId)?.isRunning ?? true;
  }

  /// 设置或取消优先下载任务（Boost 模式）。
  /// 传入当前优先任务 ID 则切换（取消），传入其他 ID 则设置为新优先任务。
  ///
  /// 在发送信号给 Rust 之前先做**乐观 UI 更新**：
  /// 一次性将所有受影响任务设置到目标状态，避免 Rust 分批处理信号导致的抖动。
  void setPriorityTask(String taskId) {
    logInfo(_tag, 'setPriorityTask: $taskId');
    final isCancel = taskId.isEmpty || taskId == _priorityTaskId;

    // 先清除上一轮 boost 守卫（无论激活新任务还是取消都需要重置）
    for (final id in _boostAutoPausedIds) {
      _optimisticPausedIds.remove(id);
    }
    _boostAutoPausedIds.clear();

    if (isCancel) {
      // 乐观取消：重置 boost 状态，后续 Rust resume 信号将还原各任务 UI
      _priorityTaskId = '';
    } else {
      // 乐观激活：立即将所有活跃/排队任务（除目标外）设为 paused，
      // 避免 Rust 分批处理期间 UI 多次重建抖动，
      // 同时为后续乱序到达的 status=1 信号建立守卫。
      for (int i = 0; i < _tasks.length; i++) {
        final t = _tasks[i];
        if (t.id == taskId) continue;
        if (!t.status.isActiveOrQueued) continue;
        _boostAutoPausedIds.add(t.id);
        _optimisticPausedIds.add(t.id);
        _tasks[i] = t.copyWith(status: TaskStatus.paused, speed: 0);
      }
      _priorityTaskId = taskId;
      // _boostAutoPausedCount 以 Rust 确认值为准，由 _onPriorityTaskChanged 更新
    }

    _safeNotifyListeners();
    SetPriorityTask(taskId: isCancel ? '' : taskId).sendSignalToRust();
  }

  /// 取消 Boost 模式
  void cancelBoost() {
    logInfo(_tag, 'cancelBoost');
    setPriorityTask('');
  }

  /// 批量暂停所有活跃任务（单次 IPC）
  void pauseAll() {
    logInfo(_tag, 'pauseAll');
    _deferredResumeQueue.clear();
    final toPause = <String>[];
    for (int i = 0; i < _tasks.length; i++) {
      final t = _tasks[i];
      if (t.status == TaskStatus.downloading ||
          t.status == TaskStatus.resuming ||
          t.status == TaskStatus.pending ||
          t.status == TaskStatus.preparing ||
          // 修复：boost 结束后，部分任务在 Rust 侧已入 pending_queue，
          // 但 Dart 侧仍显示 paused（未收到 status=0 信号）。
          // queuePosition > 0 说明任务确实在 Rust 的队列里等待启动。
          (t.status == TaskStatus.paused && t.queuePosition > 0) ||
          t.isSeeding) {
        toPause.add(t.id);
        _optimisticPausedIds.add(t.id);
        // 乐观 UI 更新
        if (t.isSeeding) {
          _tasks[i] = _pauseSeederOptimistically(t);
        } else if (t.status != TaskStatus.paused) {
          _tasks[i] = t.copyWith(status: TaskStatus.paused, speed: 0);
        }
      }
    }
    if (toPause.isEmpty) return;
    BatchControlTask(taskIds: toPause, action: 0).sendSignalToRust();
    _safeNotifyListeners();
  }

  /// 恢复所有暂停/出错的任务。
  ///
  /// 将全部候选任务一次性发送给 Rust 的 DownloadManager，由其
  /// pending_queue 统一管理并发限制，避免 Dart 侧与 Rust 侧双重
  /// 并发控制产生的竞态（重复恢复、槽位计算不一致等问题）。
  /// Dart 侧仅做乐观 UI 更新，将所有候选任务立即显示为 resuming。
  void resumeAll() {
    logInfo(_tag, 'resumeAll');
    _deferredResumeQueue.clear();

    final candidates = <String>[];
    for (int i = 0; i < _tasks.length; i++) {
      final t = _tasks[i];
      final isPausedSeeder =
          t.status == TaskStatus.completed &&
          t.seedingStatus == SeedingStatus.userStopped;
      if (t.status == TaskStatus.paused ||
          t.status == TaskStatus.error ||
          isPausedSeeder) {
        // 停止队列（含「稍后下载」栈）里的任务不参与全局恢复，
        // 由「启动队列」显式恢复——与引擎侧 resume_all_eligible 语义一致。
        if (!isQueueRunning(t.queueId)) continue;
        candidates.add(t.id);
        _boostAutoPausedIds.remove(t.id);
        _optimisticPausedIds.remove(t.id);
        if (isPausedSeeder) {
          _tasks[i] = _resumeSeederOptimistically(t);
        } else {
          _tasks[i] = t.copyWith(status: TaskStatus.resuming);
        }
      }
    }
    if (candidates.isEmpty) return;
    BatchControlTask(taskIds: candidates, action: 1).sendSignalToRust();
    _safeNotifyListeners();
  }

  /// 从延迟恢复队列中取出下一个任务并发送 resume。
  /// 当活跃任务完成/出错释放槽位时调用。
  void _resumeNextDeferred() {
    while (_deferredResumeQueue.isNotEmpty) {
      final nextId = _deferredResumeQueue.removeAt(0);
      final idx = _tasks.indexWhere((t) => t.id == nextId);
      // 跳过已删除、已完成或已在下载的任务
      if (idx < 0) continue;
      final t = _tasks[idx];
      if (t.status != TaskStatus.paused &&
          t.status != TaskStatus.error &&
          t.status != TaskStatus.pending) {
        continue;
      }
      // 发送单个 resume
      _optimisticPausedIds.remove(nextId);
      _tasks[idx] = t.copyWith(status: TaskStatus.resuming);
      ControlTask(taskId: nextId, action: 1).sendSignalToRust();
      _safeNotifyListeners();
      return;
    }
  }

  // ---------------------------------------------------------------------------
  // Signal listeners
  // ---------------------------------------------------------------------------

  void _startListening() {
    _allTasksSub = AllTasks.rustSignalStream.listen(_onAllTasks);
    _progressSub = TaskProgress.rustSignalStream.listen(_onProgress);
    _segmentSub = SegmentProgress.rustSignalStream.listen(_onSegmentProgress);
    _splitSub = SegmentSplitEvent.rustSignalStream.listen(_onSplitEvent);
    _cdnEventSub = TaskCdnEvent.rustSignalStream.listen(_onCdnEvent);
    _metaProbedSub = TaskMetaProbed.rustSignalStream.listen(_onTaskMetaProbed);
    _queuePosSub = QueuePositionsUpdate.rustSignalStream.listen(
      _onQueuePositionsUpdate,
    );
    _allQueuesSub = AllQueues.rustSignalStream.listen(_onAllQueues);
    _prioritySub = PriorityTaskChanged.rustSignalStream.listen(
      _onPriorityTaskChanged,
    );
    _fileMissingSub = FileMissingChanged.rustSignalStream.listen(
      _onFileMissingChanged,
    );
    _taskQueueChangedSub = TaskQueueChanged.rustSignalStream.listen(
      _onTaskQueueChanged,
    );
    _taskRouteChangedSub = TaskRouteChanged.rustSignalStream.listen(
      _onTaskRouteChanged,
    );
  }

  void _onAllTasks(RustSignalPack<AllTasks> pack) {
    if (_disposed) {
      logInfo(_tag, '_onAllTasks skipped (disposed)');
      return;
    }
    final incoming = pack.message.tasks;
    logInfo(_tag, '_onAllTasks: received ${incoming.length} tasks');

    // 只移除已被 Rust 确认彻底删除（不在 DB 中）的 ID：DB 中还没来得及删除的
    // 任务仍在 AllTasks 里，需保留守卫以防止 _onAllTasks 把它们重新加回 _tasks。
    final incomingIds = {for (final t in incoming) t.taskId};
    _deletedTaskIds.removeWhere((id) => !incomingIds.contains(id));

    // AllTasks 快照来自 DB，不含会话内存事件（分段拆分/CDN/链路）；重建前
    // 按 ID 留存旧实例的会话字段，否则任何一次 AllTasks 推送（如新建任务）
    // 都会抹掉其他任务日志 Tab 的记录。
    final prevById = {for (final t in _tasks) t.id: t};
    _tasks.clear();
    for (final info in incoming) {
      // 跳过仍在删除中的任务，防止 AllTasks 把它们重新插回列表（僵尸复活）。
      if (_deletedTaskIds.contains(info.taskId)) continue;
      var task = DownloadTask.fromTaskInfo(info);
      final prev = prevById[info.taskId];
      if (prev != null) {
        task = task.copyWith(
          recentSplits: prev.recentSplits,
          cdnEvents: prev.cdnEvents,
          routeEvents: prev.routeEvents,
        );
      }
      // 消费早于任务到达的链路事件缓冲（引擎启动基线可能先于本快照广播）。
      final pendingRoutes = _pendingRouteEvents.remove(info.taskId);
      if (pendingRoutes != null) {
        final events = List<RouteEventData>.from(task.routeEvents);
        var changed = false;
        for (final e in pendingRoutes) {
          changed = _appendRouteEvent(events, e.route) || changed;
        }
        if (changed) task = task.copyWith(routeEvents: events);
      }
      // 若用户已乐观暂停该任务，DB 的旧活跃状态（downloading/resuming/pending 等）
      // 不得覆盖 UI。completed 等非活跃状态不在此列（例如做种暂停应保持 completed）。
      if (_optimisticPausedIds.contains(info.taskId) &&
          task.status.isActiveOrQueued) {
        task = task.copyWith(status: TaskStatus.paused, speed: 0);
      }
      _tasks.add(task);
    }
    // 清理已不存在任务的缓冲（删除/清空场景防泄漏）。
    _pendingRouteEvents.removeWhere((id, _) => !incomingIds.contains(id));
    _safeNotifyListeners();
  }

  /// 文件跟踪：引擎扫描后定向更新受影响任务的 fileMissing 标志。只 copyWith
  /// 单个字段、不重建整表，避免活跃下载 UI 闪烁。沿用 _deletedTaskIds 守卫
  /// 防止对已删除任务的残余更新。
  void _onFileMissingChanged(RustSignalPack<FileMissingChanged> pack) {
    if (_disposed) return;
    var changed = false;
    for (final u in pack.message.updates) {
      if (_deletedTaskIds.contains(u.taskId)) continue;
      final idx = _tasks.indexWhere((t) => t.id == u.taskId);
      if (idx >= 0 && _tasks[idx].fileMissing != u.missing) {
        _tasks[idx] = _tasks[idx].copyWith(fileMissing: u.missing);
        changed = true;
      }
    }
    if (changed) _safeNotifyListeners();
  }

  /// 任务队列归属变化：move_task_to_queue 后引擎定向广播。只 copyWith 单个
  /// 字段、不重建整表（与文件跟踪同理），沿用 _deletedTaskIds 守卫。
  void _onTaskQueueChanged(RustSignalPack<TaskQueueChanged> pack) {
    if (_disposed) return;
    final m = pack.message;
    if (_deletedTaskIds.contains(m.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == m.taskId);
    if (idx >= 0 && _tasks[idx].queueId != m.queueId) {
      _tasks[idx] = _tasks[idx].copyWith(queueId: m.queueId);
      _safeNotifyListeners();
    }
  }

  /// 每任务保留的链路定论事件上限（环形缓冲，同 _maxCdnEvents 语义）。
  static const _maxRouteEvents = 8;

  /// 早于任务出现的链路事件缓冲：引擎在 do_start_task 内广播
  /// TaskRouteChanged，可能先于首条 TaskProgress/AllTasks 到达（新建任务
  /// 时序），此时任务尚不在 _tasks，直接丢弃会漏掉启动基线记录。
  /// 在任务两条创建路径（_onProgress 新任务 / _onAllTasks）消费。
  final Map<String, List<RouteEventData>> _pendingRouteEvents = {};

  /// 把 [route] 追加进 [events]（连续重复去重 + 封顶），返回是否有变化。
  static bool _appendRouteEvent(List<RouteEventData> events, String route) {
    if (route.isEmpty) return false;
    if (events.isNotEmpty && events.last.route == route) return false;
    events.add(RouteEventData(route: route));
    if (events.length > _maxRouteEvents) {
      events.removeRange(0, events.length - _maxRouteEvents);
    }
    return true;
  }

  /// Auto 代理链路变化：引擎按站点采样/切换后定向广播。只 copyWith 单个
  /// 字段、不重建整表，沿用 _deletedTaskIds 守卫（同 _onTaskQueueChanged）。
  /// 每次定论（含启动基线 direct）都追加进 routeEvents 供日志 Tab 展示——
  /// Auto 任务必有一条最终链路记录。事件去重对照本会话最后一条记录而非
  /// autoRoute 字段（DB 里的旧值会吞掉本会话首条）。
  void _onTaskRouteChanged(RustSignalPack<TaskRouteChanged> pack) {
    if (_disposed) return;
    final m = pack.message;
    if (_deletedTaskIds.contains(m.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == m.taskId);
    if (idx < 0) {
      // 任务尚未进列表（创建时序）：缓冲，待创建路径消费。
      _appendRouteEvent(
        _pendingRouteEvents.putIfAbsent(m.taskId, () => []),
        m.route,
      );
      return;
    }
    var task = _tasks[idx];
    final fieldChanged = task.autoRoute != m.route;
    final events = List<RouteEventData>.from(task.routeEvents);
    final logged = _appendRouteEvent(events, m.route);
    if (!fieldChanged && !logged) return;
    if (fieldChanged) task = task.copyWith(autoRoute: m.route);
    if (logged) task = task.copyWith(routeEvents: events);
    _tasks[idx] = task;
    _safeNotifyListeners();
  }

  void _onProgress(RustSignalPack<TaskProgress> pack) {
    if (_disposed) return;
    final p = pack.message;
    // 忽略已删除任务的残余信号，防止「僵尸复活」
    if (_deletedTaskIds.contains(p.taskId)) return;
    // 外部途径（浏览器扩展 / aria2 RPC / 管理 API）发起的删除：Dart 侧没有
    // 乐观删除记录，Rust 的删除确认信号（status=4, error="deleted"）会落到
    // 这里。直接移除任务并登记守卫，而不是把它显示为「失败」。
    if (p.status == 4 && p.errorMessage == 'deleted') {
      _deletedTaskIds.add(p.taskId);
      _tasks.removeWhere((t) => t.id == p.taskId);
      _safeNotifyListeners();
      return;
    }
    final newStatus = taskStatusFromInt(p.status);
    final idx = _tasks.indexWhere((t) => t.id == p.taskId);
    if (idx >= 0) {
      final oldStatus = _tasks[idx].status;
      // 额外守卫：用户已乐观暂停的做种任务，在 Rust 落库 seeding_status=4 之前
      // 可能仍有滞后的 seeding_status=1 进度信号到达，拦截它避免 UI 闪回做种中。
      if (_optimisticPausedIds.contains(p.taskId) &&
          oldStatus == TaskStatus.completed &&
          _tasks[idx].seedingStatus == SeedingStatus.userStopped &&
          p.seedingStatus == SeedingStatus.seeding.index) {
        return;
      }
      // 守卫逻辑：防止 Rust 积压/提前到达的信号覆盖乐观 UI 状态。
      // 对在 _optimisticPausedIds 中且当前 UI 为 paused 或 pending（延迟队列）的任务生效。
      if (_optimisticPausedIds.contains(p.taskId) &&
          (oldStatus == TaskStatus.paused || oldStatus == TaskStatus.pending)) {
        if (newStatus == TaskStatus.downloading ||
            newStatus == TaskStatus.preparing ||
            newStatus == TaskStatus.pending) {
          // 该任务仍在守卫中（未被 resumeAll 立即恢复），拦截所有活跃状态信号。
          // 延迟恢复队列会在合适时机移除守卫并发送 resume。
          return;
        }
        // paused/pending → paused / error / completed 等其他状态：不干预，直接放行。
      }
      _tasks[idx] = _tasks[idx].applyProgress(p);
      // 任务离开 downloading 状态时清空 recentSplits，避免内存泄漏
      if (oldStatus == TaskStatus.downloading &&
          newStatus != TaskStatus.downloading &&
          _tasks[idx].recentSplits.isNotEmpty) {
        _tasks[idx] = _tasks[idx].copyWith(recentSplits: const []);
      }
      // 检测下载完成：从非 completed 状态变为 completed
      if (oldStatus != TaskStatus.completed &&
          newStatus == TaskStatus.completed) {
        logInfo(_tag, 'task completed: ${p.taskId} (${p.fileName})');
        onTaskCompleted?.call(_tasks[idx]);
        // 释放槽位，从延迟队列恢复下一个任务
        _resumeNextDeferred();
      }
      // 检测下载失败：从非 error 状态变为 error
      if (oldStatus != TaskStatus.error && newStatus == TaskStatus.error) {
        // 释放槽位，从延迟队列恢复下一个任务
        _resumeNextDeferred();
      }
    } else {
      // 新任务（刚刚创建的）
      logInfo(_tag, 'new task from progress: ${p.taskId} status=$newStatus');
      var task = DownloadTask(
        id: p.taskId,
        url: p.url,
        fileName: p.fileName.isEmpty ? placeholderTaskName(p.url) : p.fileName,
        saveDir: p.saveDir,
        status: newStatus,
        downloadedBytes: p.downloadedBytes,
        totalBytes: p.totalBytes,
        speed: p.speed,
        errorMessage: p.errorMessage,
        // TaskProgress 不携带 queue_id：归属待定（非「未分组」），
        // 紧随其后的 AllTasks 快照会带来真实归属。
        queueId: kQueueAttributionPending,
      );
      // 消费早于任务到达的链路事件缓冲（引擎在 do_start_task 内广播启动
      // 基线，常先于首条 TaskProgress 抵达）。
      final pendingRoutes = _pendingRouteEvents.remove(p.taskId);
      if (pendingRoutes != null && pendingRoutes.isNotEmpty) {
        task = task.copyWith(
          routeEvents: pendingRoutes,
          autoRoute: pendingRoutes.last.route,
        );
      }
      _tasks.insert(0, task);
      // 新任务直接以 completed 状态出现（如瞬间完成的小文件）
      if (newStatus == TaskStatus.completed) {
        logInfo(_tag, 'new task instantly completed: ${p.taskId}');
        onTaskCompleted?.call(task);
      }
    }
    _safeNotifyListeners();
  }

  void _onSegmentProgress(RustSignalPack<SegmentProgress> pack) {
    if (_disposed) return;
    final sp = pack.message;
    if (_deletedTaskIds.contains(sp.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == sp.taskId);
    if (idx < 0) return;

    final segments = sp.segments
        .map(
          (s) => SegmentData(
            index: s.index,
            startByte: s.startByte,
            endByte: s.endByte,
            downloadedBytes: s.downloadedBytes,
          ),
        )
        .toList();

    _tasks[idx] = _tasks[idx].copyWith(segments: segments);
    _safeNotifyListeners();
  }

  /// Maximum number of split events to keep per task (ring buffer).
  static const _maxSplitEvents = 20;

  void _onSplitEvent(RustSignalPack<SegmentSplitEvent> pack) {
    if (_disposed) return;
    final evt = pack.message;
    if (_deletedTaskIds.contains(evt.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == evt.taskId);
    if (idx < 0) return;

    // 仅在下载中状态时记录拆分事件
    if (_tasks[idx].status != TaskStatus.downloading) return;

    final splitData = SplitEventData(
      parentIndex: evt.parentIndex,
      parentNewEnd: evt.parentNewEnd,
      childIndex: evt.childIndex,
      childStart: evt.childStart,
      childEnd: evt.childEnd,
      isProactive: evt.isProactive,
      totalSegments: evt.totalSegments,
    );

    // Keep only the most recent split events.
    final current = List<SplitEventData>.from(_tasks[idx].recentSplits);
    current.add(splitData);
    if (current.length > _maxSplitEvents) {
      current.removeRange(0, current.length - _maxSplitEvents);
    }

    _tasks[idx] = _tasks[idx].copyWith(recentSplits: current);
    logInfo(
      _tag,
      'split event: task=${evt.taskId}, '
      'parent=#${evt.parentIndex}→end=${evt.parentNewEnd}, '
      'child=#${evt.childIndex} [${evt.childStart}, ${evt.childEnd}], '
      'proactive=${evt.isProactive}, total=${evt.totalSegments}',
    );
    _safeNotifyListeners();
  }

  /// Maximum number of CDN events to keep per task (ring buffer).
  static const _maxCdnEvents = 40;

  void _onCdnEvent(RustSignalPack<TaskCdnEvent> pack) {
    if (_disposed) return;
    final evt = pack.message;
    if (_deletedTaskIds.contains(evt.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == evt.taskId);
    if (idx < 0) return;

    // 与 recentSplits 不同：CDN 事件在任务离开 downloading 后仍保留——
    // 日志 Tab 的核心价值正是完成后回看走了哪些节点（封顶防无界增长）。
    // leases 快照是"当前状态"而非离散事件：连续快照只保留最新一条，
    // 避免日志 Tab 被节流后的滚动快照刷屏。
    final current = List<CdnEventData>.from(_tasks[idx].cdnEvents);
    if (evt.kind == 'leases' &&
        current.isNotEmpty &&
        current.last.kind == 'leases') {
      current.removeLast();
    }
    current.add(
      CdnEventData(
        kind: evt.kind,
        host: evt.host,
        nodes: evt.nodes,
        ip: evt.ip,
        reason: evt.reason,
        candidates: evt.candidates,
        alive: evt.alive,
        cap: evt.cap,
        autoCap: evt.autoCap,
      ),
    );
    if (current.length > _maxCdnEvents) {
      current.removeRange(0, current.length - _maxCdnEvents);
    }
    _tasks[idx] = _tasks[idx].copyWith(cdnEvents: current);
    logInfo(
      _tag,
      'cdn event: task=${evt.taskId}, kind=${evt.kind}, host=${evt.host}, '
      'nodes=${evt.nodes.length}, ip=${evt.ip}, reason=${evt.reason}',
    );
    _safeNotifyListeners();
  }

  void _onTaskMetaProbed(RustSignalPack<TaskMetaProbed> pack) {
    if (_disposed) return;
    final p = pack.message;
    if (_deletedTaskIds.contains(p.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == p.taskId);
    if (idx < 0) return;

    final task = _tasks[idx];

    // Guard: if the task already has a confirmed file name (set by the user
    // in the dialog or resolved by the download engine via TaskProgress),
    // do NOT let the background meta-probe overwrite it.
    //
    // This prevents the race where:
    //   1. User sets a custom name → TaskProgress confirms it (fileNameConfirmed=true)
    //   2. pending-queue probe finishes → sends TaskMetaProbed with server name
    //   3. Without this guard the UI name would flip to the server name while
    //      the actual file on disk uses the user's name → mismatch.
    //
    // Rust already guards the DB side (update_task_file_name only writes when
    // file_name is empty), and all probe paths (HTTP/FTP/magnet) return an
    // empty name when file_name is non-empty — so p.fileName should already
    // be empty here whenever fileNameConfirmed is true.  This is a second
    // line of defence in case a future code path forgets that contract.
    final acceptFileName = p.fileName.isNotEmpty && !task.fileNameConfirmed;

    _tasks[idx] = task.copyWith(
      fileName: acceptFileName ? p.fileName : null,
      totalBytes: p.totalBytes > 0 ? p.totalBytes : null,
      // If we accepted a probe-supplied name, mark it confirmed so a second
      // probe signal (unlikely but possible) doesn't keep flipping the name.
      fileNameConfirmed: acceptFileName ? true : null,
    );
    _safeNotifyListeners();
  }

  void _onQueuePositionsUpdate(RustSignalPack<QueuePositionsUpdate> pack) {
    if (_disposed) return;
    final posMap = {
      for (final p in pack.message.positions) p.taskId: p.position,
    };
    bool changed = false;
    for (int i = 0; i < _tasks.length; i++) {
      final newPos = posMap[_tasks[i].id] ?? -1;
      if (_tasks[i].queuePosition != newPos) {
        _tasks[i] = _tasks[i].copyWith(queuePosition: newPos);
        changed = true;
      }
    }
    if (changed) _safeNotifyListeners();
  }

  void _onAllQueues(RustSignalPack<AllQueues> pack) {
    applyLoadedQueues(pack.message.queues);
  }

  /// Applies an authoritative queue snapshot from Rust.
  @visibleForTesting
  void applyLoadedQueues(List<QueueInfo> incoming) {
    if (_disposed) return;
    logInfo(_tag, '_onAllQueues: ${incoming.length} queues');
    _queues = incoming.map(DownloadQueue.fromQueueInfo).toList()
      ..sort((a, b) => a.position.compareTo(b.position));
    // 如果当前筛选的队列已被删除，取消筛选
    if (_queueFilter != null &&
        _queueFilter!.isNotEmpty &&
        !_queues.any((q) => q.queueId == _queueFilter)) {
      _queueFilter = null;
    }
    if (!_queuesLoadedCompleter.isCompleted) {
      _queuesLoadedCompleter.complete();
    }
    _safeNotifyListeners();
  }

  void _onPriorityTaskChanged(RustSignalPack<PriorityTaskChanged> pack) {
    if (_disposed) return;
    final p = pack.message;
    logInfo(
      _tag,
      '_onPriorityTaskChanged: priority=${p.priorityTaskId}, autoPaused=${p.autoPausedCount}',
    );

    if (p.priorityTaskId.isNotEmpty) {
      // Boost 激活确认：守卫和乐观 UI 更新已由 setPriorityTask() 完成，
      // 此处不能清空 _boostAutoPausedIds（否则守卫失效，乱序 status=1 会覆盖 UI）。
      // 仅以 Rust 权威值更新优先任务 ID。
      _priorityTaskId = p.priorityTaskId;
    } else {
      // Boost 取消（Rust 侧触发：优先任务完成、被手动暂停或删除等）。
      // 若 Dart 侧已通过 setPriorityTask('') 提前清理，_boostAutoPausedIds 为空，此处为 no-op。
      for (final id in _boostAutoPausedIds) {
        _optimisticPausedIds.remove(id);
      }
      _boostAutoPausedIds.clear();
      _priorityTaskId = '';
    }

    _safeNotifyListeners();
  }
}
