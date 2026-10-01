import 'package:flutter/material.dart';

import '../services/kv_store.dart';
import '../services/log_service.dart';
import 'flux_theme_tokens.dart';

/// KvStore 存储 key
const _kThemeMode = 'theme_mode';

/// 全局主题管理器
///
/// 仅管理亮/暗/跟随系统模式；暗色使用 [FluxThemeTokens.defaultDark]，
/// 亮色使用 [FluxThemeTokens.defaultLight]。
class ThemeProvider extends ChangeNotifier {
  ThemeMode _themeMode = ThemeMode.system;

  /// 缓存
  FluxThemeTokens? _cachedTokens;
  bool _cachedIsDark = false;

  ThemeMode get themeMode => _themeMode;

  /// 当前亮/暗对应的 token（按亮暗缓存）。
  FluxThemeTokens activeTokens(BuildContext context) {
    final dark = isDark(context);
    if (_cachedTokens != null && _cachedIsDark == dark) return _cachedTokens!;
    _cachedIsDark = dark;
    _cachedTokens = dark
        ? FluxThemeTokens.defaultDark()
        : FluxThemeTokens.defaultLight();
    return _cachedTokens!;
  }

  /// 从 KvStore 载入主题模式；失败时直接使用默认值，不阻塞进入 UI。
  Future<void> init() async {
    try {
      final modeStr = KvStore.instance.getString(_kThemeMode);
      if (modeStr != null) {
        _themeMode = ThemeMode.values.firstWhere(
          (m) => m.name == modeStr,
          orElse: () => ThemeMode.system,
        );
      }
    } catch (e, stack) {
      logError('ThemeProvider', 'init failed, using defaults', e, stack);
    }
  }

  void setThemeMode(ThemeMode mode) {
    if (_themeMode == mode) return;
    _themeMode = mode;
    _cachedTokens = null;
    notifyListeners();
    KvStore.instance.setString(_kThemeMode, mode.name);
  }

  bool isDark(BuildContext context) {
    if (_themeMode == ThemeMode.system) {
      return MediaQuery.platformBrightnessOf(context) == Brightness.dark;
    }
    return _themeMode == ThemeMode.dark;
  }
}
