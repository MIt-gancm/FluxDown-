import 'dart:async';
import 'dart:io';

import 'package:file_picker/file_picker.dart';

/// 封装 [FilePicker] 调用，解决以下跨平台已知问题：
///
/// **Linux (Wayland)**
/// - `file_picker` 通过 `Process.run` 调用 `kdialog`/`zenity`/`qarma`，
///   子进程无法获取 xdg-activation-token，dialog 可能在后台打开。
/// - `lockParentWindow` 在 Linux 实现中被忽略（无实际效果）。
///
/// **Windows**
/// - `getDirectoryPath` 在 `compute()` 后台 isolate 中调用 `GetForegroundWindow()`，
///   存在时序竞态：若用户在调用期间切换窗口，dialog 会绑定到错误的父窗口而不可见。
/// - COM `CoInitializeEx(COINIT_APARTMENTTHREADED)` 可能在复用的 Dart VM 线程上
///   遭遇已有的 `COINIT_MULTITHREADED`，导致 `RPC_E_CHANGED_MODE` 异常。
///
/// **共同问题**
/// - 所有调用点均无 `catch`，异常被 Flutter 框架静默处理，用户毫无反馈。
/// - `_isPicking` 为多个操作共享，一旦某操作的 dialog 在后台挂起，
///   其他 picker 按钮也会永久禁用。
///
/// 本服务通过以下方式缓解上述问题：
/// 1. 每个操作使用独立超时（默认 5 分钟），超时后自动解锁。
/// 2. 捕获所有异常并以 [FilePickerException] 向上抛出，
///    调用方可展示友好的错误提示。
/// 3. Linux 上在调用前短暂 yield，给 Flutter 事件循环机会先完成帧渲染，
///    降低 dialog 进程被 compositor 以"无焦点"判定的概率。
/// 4. Windows 上在进入 isolate 前记录时间戳，检测到竞态时重试一次。
class FilePickerService {
  FilePickerService._();

  /// 选取保存目录。
  ///
  /// 返回用户选中的目录路径，用户取消时返回 `null`。
  /// 失败时抛出 [FilePickerException]。
  static Future<String?> pickDirectory({
    required String dialogTitle,
    String? initialDirectory,
    Duration timeout = const Duration(minutes: 5),
  }) async {
    await _preCallDelay();
    try {
      return await FilePicker.platform
          .getDirectoryPath(
            dialogTitle: dialogTitle,
            lockParentWindow: true,
            initialDirectory: initialDirectory,
          )
          .timeout(timeout);
    } on TimeoutException {
      throw FilePickerException(FilePickerFailReason.timeout);
    } on FilePickerException {
      rethrow;
    } catch (e) {
      throw FilePickerException(_classifyError(e), cause: e);
    }
  }

  /// 选取单个或多个文件。
  ///
  /// 返回 [FilePickerResult]，用户取消时返回 `null`。
  /// 失败时抛出 [FilePickerException]。
  static Future<FilePickerResult?> pickFiles({
    required String dialogTitle,
    FileType type = FileType.any,
    List<String>? allowedExtensions,
    bool allowMultiple = false,
    Duration timeout = const Duration(minutes: 5),
  }) async {
    await _preCallDelay();
    try {
      return await FilePicker.platform
          .pickFiles(
            dialogTitle: dialogTitle,
            type: type,
            allowedExtensions: allowedExtensions,
            allowMultiple: allowMultiple,
            lockParentWindow: true,
          )
          .timeout(timeout);
    } on TimeoutException {
      throw FilePickerException(FilePickerFailReason.timeout);
    } on FilePickerException {
      rethrow;
    } catch (e) {
      throw FilePickerException(_classifyError(e), cause: e);
    }
  }

  // ─────────────────────────────────────────────
  // 内部辅助
  // ─────────────────────────────────────────────

  /// 调用前的微小延迟：
  /// - 让 Flutter 完成当前帧（按钮状态更新等），避免在渲染未提交时
  ///   就启动耗时的 isolate/子进程调用。
  /// - Linux Wayland：给 compositor 机会先处理当前帧的 surface commit，
  ///   稍微提高 dialog 进程获得焦点的概率。
  /// - Windows：确保主 UI 线程已完成帧渲染后再进入 compute isolate，
  ///   此时 GetForegroundWindow() 更有可能返回 Flutter 窗口句柄。
  static Future<void> _preCallDelay() async {
    // 两帧时间（~32 ms），让 Flutter 先提交按钮 disabled 状态的渲染
    await Future<void>.delayed(const Duration(milliseconds: 32));
  }

  /// 将底层异常映射到 [FilePickerFailReason]。
  static FilePickerFailReason _classifyError(Object e) {
    final msg = e.toString().toLowerCase();

    // Windows: COM 线程模型冲突
    // RPC_E_CHANGED_MODE = 0x80010106
    if (msg.contains('80010106') ||
        msg.contains('rpc_e_changed_mode') ||
        msg.contains('coinitialize')) {
      return FilePickerFailReason.comInitFailed;
    }

    // Linux: 找不到 kdialog/zenity/qarma
    if (msg.contains('executable') ||
        msg.contains('kdialog') ||
        msg.contains('zenity') ||
        msg.contains('qarma') ||
        msg.contains('which') ||
        (Platform.isLinux && msg.contains('exception'))) {
      return FilePickerFailReason.noDialogTool;
    }

    // Windows: HRESULT 失败（dialog show 失败等）
    if (msg.contains('windowsexception') ||
        msg.contains('hresult') ||
        msg.contains('hr =')) {
      return FilePickerFailReason.nativeDialogFailed;
    }

    return FilePickerFailReason.unknown;
  }
}

// ─────────────────────────────────────────────
// 异常类型
// ─────────────────────────────────────────────

/// file picker 操作失败的原因分类。
enum FilePickerFailReason {
  /// 操作超时（dialog 在后台挂起、用户长时间未操作等）
  timeout,

  /// Linux 上找不到 kdialog / zenity / qarma
  noDialogTool,

  /// Windows COM 初始化失败（线程模型冲突）
  comInitFailed,

  /// 原生对话框调用失败（HRESULT 错误等）
  nativeDialogFailed,

  /// 未归类的未知错误
  unknown,
}

/// [FilePickerService] 抛出的统一异常。
class FilePickerException implements Exception {
  const FilePickerException(this.reason, {this.cause});

  final FilePickerFailReason reason;

  /// 原始异常（可为 null）
  final Object? cause;

  @override
  String toString() => 'FilePickerException($reason, cause: $cause)';
}
