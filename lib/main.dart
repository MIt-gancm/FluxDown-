import 'dart:async';
import 'dart:ui';

import 'package:flutter/widgets.dart';
import 'package:rinf/rinf.dart';
import 'src/bindings/bindings.dart';
import 'src/mobile/mobile_app.dart';
import 'src/services/foreground_service.dart';
import 'src/services/log_service.dart';
import 'src/services/kv_store.dart';
import 'src/i18n/locale_provider.dart';
import 'src/i18n/framework_localizations.dart';
import 'src/theme/theme_provider.dart';

/// 启动阶段的非关键步骤统一加超时保护和日志，
/// 防止某一步卡住导致整个应用白屏。
Future<void> _runStartupStep(
  String name,
  Future<void> Function() action, {
  Duration timeout = const Duration(seconds: 3),
}) async {
  final sw = Stopwatch()..start();
  logInfo('startup', 'starting $name');
  try {
    await action().timeout(timeout);
    logInfo('startup', 'completed $name in ${sw.elapsedMilliseconds}ms');
  } catch (e, stack) {
    logError(
      'startup',
      '$name failed after ${sw.elapsedMilliseconds}ms, continuing with defaults',
      e,
      stack,
    );
  }
}

/// 记录 Flutter 官方定义的启动边界：引擎完成首帧栅格化。
void _logFirstFrameWhenRasterized(Stopwatch startupStopwatch) {
  unawaited(
    WidgetsBinding.instance.waitUntilFirstFrameRasterized.then((_) {
      startupStopwatch.stop();
      logInfo(
        'startup',
        'first frame rasterized in ${startupStopwatch.elapsedMilliseconds}ms',
      );
    }),
  );
}

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final startupStopwatch = Stopwatch()..start();

  // 初始化日志服务 — 必须尽早执行。
  // 预览版 Windows 上若 SharedPreferences / 插件初始化卡住，
  // 需要保证这些启动前故障也能写入日志，而不是只剩白屏。
  LogService.instance.init();
  logInfo('main', 'bootstrap start, args=$args');

  // 必须早于所有 provider/service 读取。便携模式使用 exe 目录 settings.json。
  await _runStartupStep('kv store init', () => KvStore.instance.init());

  // 主题只依赖已载入的 KvStore，与翻译资源加载互不依赖。并发等待可让
  // AssetBundle I/O 与本地主题解析重叠，同时仍保证 runApp 前主题和语言
  // 都已就绪，不引入首帧闪烁。
  final themeProvider = ThemeProvider();
  await Future.wait<void>([
    _runStartupStep('i18n load', I18nStore.load),
    _runStartupStep('theme init', () => themeProvider.init()),
  ]);
  localeNotifier = LocaleNotifier();
  await _runStartupStep('locale init', () => localeNotifier.init());
  await _runStartupStep(
    'shad l10n warmup',
    () => AppShadLocalizationsDelegate.warmUp(frameworkLocale(currentLocale)),
  );

  // 设置全局异常捕获 — Flutter 框架异常
  FlutterError.onError = (details) {
    logError(
      'FlutterError',
      details.exceptionAsString(),
      details.exception,
      details.stack,
    );
  };

  // 设置全局异常捕获 — Dart 未捕获异步异常
  // 使用 PlatformDispatcher.onError 而非 runZonedGuarded，
  // 避免 Zone mismatch（ensureInitialized 和 runApp 必须在同一 Zone）
  PlatformDispatcher.instance.onError = (error, stack) {
    logError('PlatformError', 'Uncaught async error', error, stack);
    return true; // 已处理，不再向上传播
  };

  logInfo('main', 'theme and locale init steps finished');

  // ===== 移动端启动流程 =====
  ForegroundServiceManager.initCommunicationPort();
  logInfo('main', 'initializing Rust runtime (mobile)...');
  await initializeRust(assignRustSignal);
  logInfo('main', 'starting mobile shell');
  runApp(
    FluxDownMobileApp(
      themeProvider: themeProvider,
      localeNotifier: localeNotifier,
    ),
  );
  _logFirstFrameWhenRasterized(startupStopwatch);
}
