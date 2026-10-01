import 'dart:io';

import 'package:flutter/services.dart';

/// 移动端"打开文件"失败原因，供调用端映射为 i18n 提示。
enum OpenFileError {
  /// 文件不存在（已被外部删除/移动）
  notFound,

  /// 没有应用能处理该文件类型（或系统拒绝）
  noHandler,

  /// 其他失败（FileProvider 根未覆盖该路径等）
  failed,
}

/// 移动端打开文件失败异常，携带结构化原因。
class OpenFileException implements Exception {
  final OpenFileError error;
  final String message;

  const OpenFileException(this.error, this.message);

  @override
  String toString() => 'OpenFileException($error): $message';
}

/// 与 MainActivity.kt / AppDelegate.swift 的 `com.fluxdown/storage` 通道对应。
const _storageChannel = MethodChannel('com.fluxdown/storage');

/// 用系统默认程序打开文件。
///
/// 经 `com.fluxdown/storage` MethodChannel 走原生实现：
/// - **Android**（MainActivity.kt `openFile`）：FileProvider 生成 content:// URI +
///   ACTION_VIEW（targetSdk ≥ 24 禁止 file:// 出应用）；按扩展名解析 MIME 交给
///   默认关联应用，无关联时回退系统选择器（chooser）让用户自选。
/// - **iOS**（AppDelegate.swift `openFile`）：UIDocumentInteractionController
///   弹出系统"打开方式"菜单，由用户选择应用。
///
/// 失败抛 [OpenFileException]（notFound / noHandler / failed），由调用端映射
/// 为 i18n 提示。
Future<void> openFile(String filePath) async {
  if (Platform.isAndroid || Platform.isIOS) {
    try {
      await _storageChannel.invokeMethod<bool>('openFile', {'path': filePath});
    } on PlatformException catch (e) {
      final error = switch (e.code) {
        'not_found' => OpenFileError.notFound,
        'no_handler' => OpenFileError.noHandler,
        _ => OpenFileError.failed,
      };
      throw OpenFileException(error, e.message ?? e.code);
    }
  }
}
