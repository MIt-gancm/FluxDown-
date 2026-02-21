import 'dart:async';
import 'dart:io';

import 'package:flutter/foundation.dart';
import 'package:rinf/rinf.dart';

import '../bindings/bindings.dart';
import '../services/analytics_service.dart';
import '../services/log_service.dart';
import 'download_queue.dart';
import 'download_task.dart';

const _tag = 'DownloadCtrl';

/// 顶部 Tab 状态筛选
enum StatusTab { all, downloading, completed, paused, error }

/// 核心状态管理器 — 桥接 Rust 信号和 Flutter UI
class DownloadController extends ChangeNotifier {
  /// 全局单例引用，供无 context 场景（如 ExternalDownloadService）读取队列信息
  static DownloadController? globalInstance;
  final List<DownloadTask> _tasks = [];
  String? _selectedTaskId;
  FileCategory _categoryFilter = FileCategory.all;
  StatusTab _statusTab = StatusTab.all;

  /// 命名队列列表（来自 Rust AllQueues 信号）
  List<DownloadQueue> _queues = [];

  /// 当前队列筛选 ID：null = 不过滤（显示全部），'' = 默认队列，非空 = 指定命名队列
  String? _queueFilter;

  // 管理模式（多选）
  bool _isManageMode = false;
  final Set<String> _checkedTaskIds = {};

  // 时间分组折叠状态（key 为 TimeGroup，value 为是否折叠）
  final Map<TimeGroup, bool> _collapsedGroups = {};

  /// 已删除任务 ID 集合 — 防止 Rust 残余信号将已删除任务「复活」到列表中。
  /// 在 _onAllTasks 刷新时清空（Rust 端不再包含该任务即可安全移除）。
  final Set<String> _deletedTaskIds = {};

  /// 用户主动暂停的任务 ID 集合（乐观暂停）。
  /// 守卫：阻止 _onAllTasks 从 DB 覆盖 UI 暂停状态，以及阻止积压的 downloading
  /// 信号将 UI 改回下载中。只在 resumeTask / deleteTask 时移除。
  final Set<String> _optimisticPausedIds = {};

  /// 下载完成回调 — 当任务状态从非 completed 变为 completed 时触发
  void Function(DownloadTask task)? onTaskCompleted;

  StreamSubscription<RustSignalPack<TaskProgress>>? _progressSub;
  StreamSubscription<RustSignalPack<AllTasks>>? _allTasksSub;
  StreamSubscription<RustSignalPack<SegmentProgress>>? _segmentSub;
  StreamSubscription<RustSignalPack<SegmentSplitEvent>>? _splitSub;
  StreamSubscription<RustSignalPack<TaskMetaProbed>>? _metaProbedSub;
  StreamSubscription<RustSignalPack<QueuePositionsUpdate>>? _queuePosSub;
  StreamSubscription<RustSignalPack<AllQueues>>? _allQueuesSub;

  bool _disposed = false;

  DownloadController() {
    logInfo(_tag, 'constructor — starting listeners');
    globalInstance = this;
    _startListening();
    // 启动时请求所有持久化任务和队列
    const RequestAllTasks().sendSignalToRust();
    const RequestAllQueues().sendSignalToRust();
  }

  @override
  void dispose() {
    logInfo(_tag, 'dispose called');
    _disposed = true;
    if (globalInstance == this) globalInstance = null;
    _progressSub?.cancel();
    _allTasksSub?.cancel();
    _segmentSub?.cancel();
    _splitSub?.cancel();
    _metaProbedSub?.cancel();
    _queuePosSub?.cancel();
    _allQueuesSub?.cancel();
    super.dispose();
    logInfo(_tag, 'dispose done');
  }

  /// 安全的 notifyListeners — dispose 后不再通知，避免
  /// "A DownloadController was used after being disposed" 异常
  void _safeNotifyListeners() {
    if (!_disposed) notifyListeners();
  }

  /// 从 URL 推断下载协议（用于分析埋点）
  static String _inferProtocol(String url) {
    if (url.isEmpty) return 'bt';
    final lower = url.toLowerCase();
    if (lower.startsWith('ftp')) return 'ftp';
    if (lower.startsWith('magnet:')) return 'bt';
    return 'http';
  }

  /// 将错误消息分类为匿名类别，避免将 URL/路径等敏感信息发送到分析服务。
  static String _classifyError(String msg) {
    if (msg.isEmpty) return 'unknown';
    final lower = msg.toLowerCase();
    if (lower.contains('timeout') || lower.contains('timed out')) {
      return 'timeout';
    }
    if (lower.contains('connection') || lower.contains('network')) {
      return 'network';
    }
    if (lower.contains('disk') ||
        lower.contains('space') ||
        lower.contains('permission') ||
        lower.contains('access')) {
      return 'disk';
    }
    if (lower.contains('404') || lower.contains('not found')) {
      return 'not_found';
    }
    if (lower.contains('403') || lower.contains('forbidden')) {
      return 'forbidden';
    }
    if (lower.contains('ssl') || lower.contains('certificate')) {
      return 'ssl';
    }
    if (lower.contains('cancel')) return 'cancelled';
    return 'other';
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
    final byCategory = _categoryFiltered;
    return switch (_statusTab) {
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
    };
  }

  /// 将 filteredTasks 分组：活跃+排队任务置顶，历史任务按时间分组
  List<TaskGroup> get groupedTasks {
    final tasks = filteredTasks;

    // "全部" 和 "下载中" Tab：活跃+排队任务组置顶，历史任务按时间分组
    if (_statusTab == StatusTab.all || _statusTab == StatusTab.downloading) {
      final activeTasks =
          tasks.where((t) => t.status.isActiveOrQueued).toList()
            ..sort(_compareActiveTasks);
      final historicalTasks =
          tasks.where((t) => !t.status.isActiveOrQueued).toList();
      return [
        if (activeTasks.isNotEmpty)
          TaskGroup(group: null, tasks: activeTasks),
        ..._buildTimeGroups(historicalTasks),
      ];
    }
    return _buildTimeGroups(tasks);
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

  /// 某个时间分组是否折叠（active group 永不折叠）
  bool isGroupCollapsed(TimeGroup? group) {
    if (group == null) return false; // 活跃组不可折叠
    return _collapsedGroups[group] ?? false;
  }

  /// 切换某个时间分组的折叠状态
  void toggleGroupCollapsed(TimeGroup group) {
    _collapsedGroups[group] = !isGroupCollapsed(group);
    _safeNotifyListeners();
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
    };
  }

  /// 各文件类型分类的任务数量（用于侧边栏显示计数）。
  /// 若当前有队列筛选，仅统计该队列内的任务。
  int countForCategory(FileCategory category) {
    final base = _queueFiltered;
    if (category == FileCategory.all) return base.length;
    return base.where((t) => t.fileCategory == category).length;
  }

  /// 各状态的任务数量（用于侧边栏状态区块计数）。
  /// 若当前有队列筛选，仅统计该队列内的任务，使计数与点击后的实际结果一致。
  int countForStatus(StatusTab tab) {
    final base = _queueFiltered;
    return switch (tab) {
      StatusTab.all => base.length,
      StatusTab.downloading => base
          .where(
            (t) =>
                t.status == TaskStatus.downloading ||
                t.status == TaskStatus.pending ||
                t.status == TaskStatus.preparing ||
                t.status == TaskStatus.resuming,
          )
          .length,
      StatusTab.completed =>
        base.where((t) => t.status == TaskStatus.completed).length,
      StatusTab.paused =>
        base.where((t) => t.status == TaskStatus.paused).length,
      StatusTab.error =>
        base.where((t) => t.status == TaskStatus.error).length,
    };
  }

  /// 指定队列中的任务数量（用于侧边栏队列计数）
  /// [queueId] 为空字符串表示默认队列
  int countForQueue(String queueId) {
    return _tasks.where((t) => t.queueId == queueId).length;
  }

  // ---------------------------------------------------------------------------
  // 管理模式（多选批量操作）
  // ---------------------------------------------------------------------------

  bool get isManageMode => _isManageMode;
  Set<String> get checkedTaskIds => _checkedTaskIds;
  int get checkedCount => _checkedTaskIds.length;

  /// 进入/退出管理模式
  void toggleManageMode() {
    _isManageMode = !_isManageMode;
    if (!_isManageMode) _checkedTaskIds.clear();
    _safeNotifyListeners();
  }

  void exitManageMode() {
    if (!_isManageMode) return;
    _isManageMode = false;
    _checkedTaskIds.clear();
    _safeNotifyListeners();
  }

  /// 切换单个任务的选中状态
  void toggleTaskChecked(String taskId) {
    if (_checkedTaskIds.contains(taskId)) {
      _checkedTaskIds.remove(taskId);
    } else {
      _checkedTaskIds.add(taskId);
    }
    _safeNotifyListeners();
  }

  /// 全选当前筛选列表中的任务
  void selectAllFiltered() {
    for (final t in filteredTasks) {
      _checkedTaskIds.add(t.id);
    }
    _safeNotifyListeners();
  }

  /// 取消全选
  void deselectAll() {
    _checkedTaskIds.clear();
    _safeNotifyListeners();
  }

  /// 当前筛选列表是否已全选
  bool get isAllFilteredChecked {
    final filtered = filteredTasks;
    if (filtered.isEmpty) return false;
    return filtered.every((t) => _checkedTaskIds.contains(t.id));
  }

  /// 批量删除选中的任务
  void deleteCheckedTasks({required bool deleteFiles}) {
    final ids = _checkedTaskIds.toList();
    logInfo(
      _tag,
      'deleteCheckedTasks: ${ids.length} tasks, deleteFiles=$deleteFiles',
    );
    for (final id in ids) {
      _optimisticPausedIds.remove(id);
      final action = deleteFiles ? 3 : 4;
      ControlTask(taskId: id, action: action).sendSignalToRust();
      _deletedTaskIds.add(id);
      _tasks.removeWhere((t) => t.id == id);
      if (_selectedTaskId == id) _selectedTaskId = null;
    }
    _checkedTaskIds.clear();
    _isManageMode = false;
    _safeNotifyListeners();
  }

  String? get selectedTaskId => _selectedTaskId;

  DownloadTask? get selectedTask {
    if (_selectedTaskId == null) return null;
    final idx = _tasks.indexWhere((t) => t.id == _selectedTaskId);
    return idx >= 0 ? _tasks[idx] : null;
  }

  /// 统计数据
  int get downloadingCount =>
      _tasks.where((t) => t.status == TaskStatus.downloading).length;
  int get completedCount =>
      _tasks.where((t) => t.status == TaskStatus.completed).length;
  int get pausedCount =>
      _tasks.where((t) => t.status == TaskStatus.paused).length;
  int get errorCount =>
      _tasks.where((t) => t.status == TaskStatus.error).length;
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
  }) {
    logInfo(
      _tag,
      'createTask: url=$url, dir=$saveDir, file=$fileName, seg=$segments, cookies_len=${cookies.length}, torrent_bytes=${torrentFileBytes?.length ?? 0}, queue=$queueId',
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
    ).sendSignalToRust();
    // 分析埋点
    final protocol = (torrentFileBytes != null && torrentFileBytes.isNotEmpty)
        ? 'bt'
        : _inferProtocol(url);
    AnalyticsService.instance.trackDownloadCreated(protocol);
  }

  /// Create a download task from a .torrent file on disk.
  Future<void> createTaskFromTorrentFile({
    required String torrentFilePath,
    required String saveDir,
    String proxyUrl = '',
  }) async {
    logInfo(
      _tag,
      'createTaskFromTorrentFile: path=$torrentFilePath, dir=$saveDir',
    );
    await sendTorrentFileSignal(torrentFilePath, saveDir, proxyUrl: proxyUrl);
  }

  /// Read a .torrent file from disk and send a [CreateTask] signal to Rust.
  ///
  /// This is a static helper so it can be called both from a
  /// [DownloadController] instance and from [main.dart] (which has no
  /// controller instance at startup).
  static Future<void> sendTorrentFileSignal(
    String torrentFilePath,
    String saveDir, {
    String proxyUrl = '',
  }) async {
    try {
      final file = File(torrentFilePath);
      final bytes = await file.readAsBytes();
      if (bytes.isEmpty) {
        logInfo(_tag, 'torrent file is empty: $torrentFilePath');
        return;
      }
      // Use the .torrent file name (without extension) as the initial display name.
      final baseName = file.uri.pathSegments.last;
      final displayName = baseName.endsWith('.torrent')
          ? baseName.substring(0, baseName.length - 8)
          : baseName;

      CreateTask(
        url: '',
        saveDir: saveDir,
        fileName: displayName,
        segments: 0,
        cookies: '',
        torrentFileBytes: bytes,
        proxyUrl: proxyUrl,
        userAgent: '',
        queueId: '',
        checksum: '',
      ).sendSignalToRust();
      AnalyticsService.instance.trackDownloadCreated('bt');
    } catch (e) {
      logInfo(_tag, 'failed to read torrent file: $e');
    }
  }

  /// 批量创建下载任务（多个 URL 共享同一保存目录和线程数）
  void batchCreateTask({
    required List<UrlEntry> entries,
    required String saveDir,
    int segments = 0,
    String proxyUrl = '',
    String userAgent = '',
    String queueId = '',
  }) {
    logInfo(
      _tag,
      'batchCreateTask: ${entries.length} entries, dir=$saveDir, seg=$segments, queue=$queueId',
    );
    BatchCreateTask(
      entries: entries,
      saveDir: saveDir,
      segments: segments,
      proxyUrl: proxyUrl,
      userAgent: userAgent,
      queueId: queueId,
    ).sendSignalToRust();
    for (final entry in entries) {
      final protocol = entry.url.toLowerCase().startsWith('ftp') ? 'ftp' : 'http';
      AnalyticsService.instance.trackDownloadCreated(protocol);
    }
  }

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
      }
    }
    ControlTask(taskId: taskId, action: 0).sendSignalToRust();
  }

  void resumeTask(String taskId) {
    logInfo(_tag, 'resumeTask: $taskId');
    _optimisticPausedIds.remove(taskId);
    // 立即切换到 resuming 状态，让 UI 即时响应
    final idx = _tasks.indexWhere((t) => t.id == taskId);
    if (idx >= 0) {
      _tasks[idx] = _tasks[idx].copyWith(status: TaskStatus.resuming);
      _safeNotifyListeners();
    }
    ControlTask(taskId: taskId, action: 1).sendSignalToRust();
  }

  void cancelTask(String taskId) {
    logInfo(_tag, 'cancelTask: $taskId');
    ControlTask(taskId: taskId, action: 2).sendSignalToRust();
  }

  /// 删除任务。[deleteFiles] 为 true 时同时删除磁盘上的已下载文件。
  void deleteTask(String taskId, {bool deleteFiles = true}) {
    logInfo(_tag, 'deleteTask: $taskId, deleteFiles=$deleteFiles');
    _optimisticPausedIds.remove(taskId);
    final action = deleteFiles ? 3 : 4;
    ControlTask(taskId: taskId, action: action).sendSignalToRust();
    _deletedTaskIds.add(taskId);
    _tasks.removeWhere((t) => t.id == taskId);
    if (_selectedTaskId == taskId) _selectedTaskId = null;
    _safeNotifyListeners();
  }

  void selectTask(String? taskId) {
    if (_selectedTaskId == taskId) return;
    _selectedTaskId = taskId;
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

  /// 设置队列筛选。传入相同 ID 则切换回「不过滤」。
  void setQueueFilter(String? queueId) {
    if (_queueFilter == queueId) {
      _queueFilter = null;
    } else {
      _queueFilter = queueId;
    }
    _safeNotifyListeners();
  }

  // ---------------------------------------------------------------------------
  // Queue CRUD — 发送信号到 Rust
  // ---------------------------------------------------------------------------

  void createQueue({
    required String name,
    int speedLimitKbps = 0,
    int maxConcurrent = 0,
    String defaultSaveDir = '',
    int defaultSegments = 0,
    String defaultUserAgent = '',
  }) {
    logInfo(
      _tag,
      'createQueue: name=$name, speedLimit=$speedLimitKbps, maxConcurrent=$maxConcurrent, defaultSegments=$defaultSegments, ua=${defaultUserAgent.isEmpty ? "(global)" : defaultUserAgent.substring(0, defaultUserAgent.length.clamp(0, 20))}',
    );
    CreateQueue(
      name: name,
      speedLimitKbps: speedLimitKbps,
      maxConcurrent: maxConcurrent,
      defaultSaveDir: defaultSaveDir,
      defaultSegments: defaultSegments,
      defaultUserAgent: defaultUserAgent,
    ).sendSignalToRust();
  }

  void updateQueue({
    required String queueId,
    required String name,
    int speedLimitKbps = 0,
    int maxConcurrent = 0,
    String defaultSaveDir = '',
    int defaultSegments = 0,
    String defaultUserAgent = '',
  }) {
    logInfo(_tag, 'updateQueue: id=$queueId, name=$name, defaultSegments=$defaultSegments, ua=${defaultUserAgent.isEmpty ? "(global)" : defaultUserAgent.substring(0, defaultUserAgent.length.clamp(0, 20))}');
    UpdateQueue(
      queueId: queueId,
      name: name,
      speedLimitKbps: speedLimitKbps,
      maxConcurrent: maxConcurrent,
      defaultSaveDir: defaultSaveDir,
      defaultSegments: defaultSegments,
      defaultUserAgent: defaultUserAgent,
    ).sendSignalToRust();
  }

  void deleteQueue(String queueId) {
    logInfo(_tag, 'deleteQueue: id=$queueId');
    DeleteQueue(queueId: queueId).sendSignalToRust();
    // 如果当前正在筛选该队列，取消筛选
    if (_queueFilter == queueId) {
      _queueFilter = null;
      _safeNotifyListeners();
    }
  }

  void moveTaskToQueue(String taskId, String queueId) {
    logInfo(_tag, 'moveTaskToQueue: task=$taskId, queue=$queueId');
    MoveTaskToQueue(taskId: taskId, queueId: queueId).sendSignalToRust();
  }

  void pauseAll() {
    logInfo(_tag, 'pauseAll');
    for (final t in _tasks) {
      if (t.status == TaskStatus.downloading ||
          t.status == TaskStatus.resuming ||
          t.status == TaskStatus.pending ||
          t.status == TaskStatus.preparing) {
        pauseTask(t.id);
      }
    }
  }

  void resumeAll() {
    logInfo(_tag, 'resumeAll');
    for (final t in _tasks) {
      if (t.status == TaskStatus.paused || t.status == TaskStatus.error) {
        resumeTask(t.id);
      }
    }
  }

  /// 默认下载目录
  static String get defaultSaveDir {
    // Windows: C:\Users\<user>\Downloads
    // macOS/Linux: ~/Downloads
    final home =
        Platform.environment['USERPROFILE'] ??
        Platform.environment['HOME'] ??
        '.';
    return '$home${Platform.pathSeparator}Downloads';
  }

  // ---------------------------------------------------------------------------
  // Signal listeners
  // ---------------------------------------------------------------------------

  void _startListening() {
    _allTasksSub = AllTasks.rustSignalStream.listen(_onAllTasks);
    _progressSub = TaskProgress.rustSignalStream.listen(_onProgress);
    _segmentSub = SegmentProgress.rustSignalStream.listen(_onSegmentProgress);
    _splitSub = SegmentSplitEvent.rustSignalStream.listen(_onSplitEvent);
    _metaProbedSub = TaskMetaProbed.rustSignalStream.listen(_onTaskMetaProbed);
    _queuePosSub =
        QueuePositionsUpdate.rustSignalStream.listen(_onQueuePositionsUpdate);
    _allQueuesSub = AllQueues.rustSignalStream.listen(_onAllQueues);
  }

  void _onAllTasks(RustSignalPack<AllTasks> pack) {
    if (_disposed) {
      logInfo(_tag, '_onAllTasks skipped (disposed)');
      return;
    }
    final incoming = pack.message.tasks;
    logInfo(_tag, '_onAllTasks: received ${incoming.length} tasks');
    _deletedTaskIds.clear(); // 全量刷新后旧的删除标记不再需要
    _tasks.clear();
    for (final info in incoming) {
      var task = DownloadTask.fromTaskInfo(info);
      // 若用户已乐观暂停该任务，DB 的旧状态（downloading/resuming 等）不得覆盖 UI。
      // 避免 AllTasks 到达时 DB 尚未写入 paused，导致任务被还原成下载中，
      // 随后守卫又因 old=downloading != paused 而失效。
      if (_optimisticPausedIds.contains(info.taskId) &&
          task.status != TaskStatus.paused) {
        task = task.copyWith(status: TaskStatus.paused, speed: 0);
      }
      _tasks.add(task);
    }
    _safeNotifyListeners();
  }

  void _onProgress(RustSignalPack<TaskProgress> pack) {
    if (_disposed) return;
    final p = pack.message;
    // 忽略已删除任务的残余信号，防止「僵尸复活」
    if (_deletedTaskIds.contains(p.taskId)) return;
    final newStatus = taskStatusFromInt(p.status);
    final idx = _tasks.indexWhere((t) => t.id == p.taskId);
    if (idx >= 0) {
      final oldStatus = _tasks[idx].status;
      // 防止 Rust progress_reporter channel 中积压的 status=1 更新覆盖已
      // 乐观设置的 paused 状态。仅对用户主动暂停的任务生效（_optimisticPausedIds），
      // 避免 AllTasks 从 DB 加载的历史 paused 状态也触发此守卫，导致任务永远
      // 卡在「暂停」显示。
      if (_optimisticPausedIds.contains(p.taskId) &&
          oldStatus == TaskStatus.paused &&
          newStatus == TaskStatus.downloading) {
        return;
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
        final proto = _inferProtocol(_tasks[idx].url);
        AnalyticsService.instance.trackDownloadCompleted(proto, p.totalBytes);
      }
      // 检测下载失败：从非 error 状态变为 error
      if (oldStatus != TaskStatus.error && newStatus == TaskStatus.error) {
        AnalyticsService.instance.trackDownloadFailed(
          _inferProtocol(_tasks[idx].url),
          _classifyError(p.errorMessage),
        );
      }
    } else {
      // 新任务（刚刚创建的）
      logInfo(_tag, 'new task from progress: ${p.taskId} status=$newStatus');
      final task = DownloadTask(
        id: p.taskId,
        url: p.url,
        fileName: p.fileName.isEmpty ? '未知文件' : p.fileName,
        saveDir: p.saveDir,
        status: newStatus,
        downloadedBytes: p.downloadedBytes,
        totalBytes: p.totalBytes,
        speed: p.speed,
        errorMessage: p.errorMessage,
      );
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

  void _onTaskMetaProbed(RustSignalPack<TaskMetaProbed> pack) {
    if (_disposed) return;
    final p = pack.message;
    if (_deletedTaskIds.contains(p.taskId)) return;
    final idx = _tasks.indexWhere((t) => t.id == p.taskId);
    if (idx < 0) return;
    _tasks[idx] = _tasks[idx].copyWith(
      fileName: p.fileName.isNotEmpty ? p.fileName : null,
      totalBytes: p.totalBytes > 0 ? p.totalBytes : null,
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
    if (_disposed) return;
    final incoming = pack.message.queues;
    logInfo(_tag, '_onAllQueues: ${incoming.length} queues');
    _queues = incoming
        .map(DownloadQueue.fromQueueInfo)
        .toList()
      ..sort((a, b) => a.position.compareTo(b.position));
    // 如果当前筛选的队列已被删除，取消筛选
    if (_queueFilter != null &&
        _queueFilter!.isNotEmpty &&
        !_queues.any((q) => q.queueId == _queueFilter)) {
      _queueFilter = null;
    }
    _safeNotifyListeners();
  }
}
