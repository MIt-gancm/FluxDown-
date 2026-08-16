import 'dart:convert';
import 'dart:io';

import 'package:flutter/services.dart';

import '../../services/log_service.dart';

const _tag = 'ShareIntent';

/// 系统分享 / URL scheme 接入桥（对应 Android [MainActivity] 的
/// `com.fluxdown/share` channel 与 iOS [AppDelegate] 的同名 channel）。
///
/// 两端约定：
/// - 原生侧 invoke `onShare`（热启动，应用已在前台/后台收到新分享 intent）；
/// - Dart 侧 invoke `getInitialShare`（冷启动，首帧就绪后主动拉取暂存内容，
///   取走即清空，避免重复触发）。
///
/// 载荷双形态兼容：Android 传 `{url, filename}` Map（filename 仅
/// fluxdown:// 协议携带，其余为空串）；iOS 仍传纯 String（无协议模式）。
///
/// 分享内容可能夹带描述文字（如“看看这个 https://x/f.zip”），[extractUrl]
/// 从中提取首个可下载的 URL / magnet。
class SharedDownloadRequest {
  final String url;
  final String filename;
  final String cookies;
  final String referrer;
  final Map<String, String> headers;
  final bool external;

  const SharedDownloadRequest({
    required this.url,
    this.filename = '',
    this.cookies = '',
    this.referrer = '',
    this.headers = const {},
    this.external = false,
  });
}

class ShareIntentService {
  ShareIntentService._();

  static const _channel = MethodChannel('com.fluxdown/share');

  /// 当前平台是否支持系统分享接入
  static bool get supported => Platform.isAndroid || Platform.isIOS;

  static void Function(SharedDownloadRequest request)? _onShared;

  /// 注册分享回调，并立即拉取冷启动时暂存的分享内容。
  static Future<void> init(
    void Function(SharedDownloadRequest request) onShared,
  ) async {
    if (!supported) return;
    _onShared = onShared;
    _channel.setMethodCallHandler(_handle);
    try {
      final initial = await _channel.invokeMethod<Object>('getInitialShare');
      _dispatch(initial);
    } catch (e, st) {
      logError(_tag, 'getInitialShare failed', e, st);
    }
  }

  static void shutdown() {
    _onShared = null;
    if (supported) _channel.setMethodCallHandler(null);
  }

  static Future<void> _handle(MethodCall call) async {
    if (call.method == 'onShare') {
      _dispatch(call.arguments);
    }
  }

  static Future<bool> returnToSourceApp() async {
    if (!Platform.isAndroid) return false;
    try {
      return await _channel.invokeMethod<bool>('moveTaskToBack') ?? false;
    } catch (e, st) {
      logError(_tag, 'moveTaskToBack failed', e, st);
      return false;
    }
  }

  static void _dispatch(Object? raw) {
    final String? text;
    var filename = '';
    var cookies = '';
    var referrer = '';
    var headers = const <String, String>{};
    var external = false;
    if (raw is Map) {
      text = raw['url'] as String?;
      filename = (raw['filename'] as String?)?.trim() ?? '';
      cookies = raw['cookies'] as String? ?? '';
      referrer = (raw['referrer'] as String?)?.trim() ?? '';
      headers = _decodeHeaders(raw['headers'] as String?);
      external = raw['external'] == 'true';
    } else {
      text = raw as String?;
    }
    final url = extractUrl(text);
    if (url == null) {
      if (text != null && text.isNotEmpty) {
        logInfo(_tag, 'shared text has no usable url');
      }
      return;
    }
    logInfo(
      _tag,
      'shared url received: cookies_len=${cookies.length}, headers=${headers.length}',
    );
    _onShared?.call(
      SharedDownloadRequest(
        url: url,
        filename: filename,
        cookies: cookies,
        referrer: referrer,
        headers: headers,
        external: external,
      ),
    );
  }

  static Map<String, String> _decodeHeaders(String? raw) {
    if (raw == null || raw.isEmpty) return const {};
    try {
      final decoded = jsonDecode(raw);
      if (decoded is! Map) return const {};
      return {
        for (final entry in decoded.entries)
          if (entry.key is String && entry.value is String)
            entry.key as String: entry.value as String,
      };
    } catch (e) {
      logInfo(_tag, 'invalid shared headers ignored: $e');
      return const {};
    }
  }

  /// 从分享文本中提取首个可下载链接。
  ///
  /// 优先匹配 magnet，其次 http(s)/ftp 直链；整串本身即为链接时直接返回。
  /// 无匹配返回 `null`。
  static String? extractUrl(String? raw) {
    if (raw == null) return null;
    final text = raw.trim();
    if (text.isEmpty) return null;
    final match = _urlPattern.firstMatch(text);
    return match?.group(0);
  }

  static final RegExp _urlPattern = RegExp(
    r'(magnet:\?[^\s]+|ed2k://[^\s]+|(?:https?|ftp)://[^\s]+)',
    caseSensitive: false,
  );
}
