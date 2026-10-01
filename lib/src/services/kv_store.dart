import 'package:flutter/foundation.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'log_service.dart';

const _tag = 'KvStore';

/// 轻量级键值存储门面，包装 [SharedPreferences]，提供同步读取。
///
/// 对外仅暴露同步读取（`getString/getBool/getDouble`）与异步写入
/// （`setString/setBool/setDouble`、`remove`）子集。所有值在 [init] 时
/// 一次性载入内存缓存，因此读取始终同步；写入即时更新缓存并透传到
/// [SharedPreferences]。
///
/// 使用前必须先 `await KvStore.instance.init()`（在 `main` 最早期，
/// 早于任何 provider/service 的读取）。
class KvStore {
  KvStore._();

  /// 全局单例。
  static final KvStore instance = KvStore._();

  /// 内存缓存：所有键值一次性载入，读取全部同步命中。
  final Map<String, Object> _cache = {};

  SharedPreferences? _prefs;

  /// 是否已初始化，防止重复 init。
  bool _initialized = false;

  /// 载入持久化数据到内存缓存。
  ///
  /// 幂等：重复调用直接返回。任何异常都降级为空缓存并记日志，绝不阻塞启动。
  Future<void> init() async {
    if (_initialized) return;
    _initialized = true;
    try {
      final prefs = await SharedPreferences.getInstance();
      _prefs = prefs;
      for (final key in prefs.getKeys()) {
        final value = prefs.get(key);
        // 仅缓存本门面支持的标量类型；本项目所有调用点均只用
        // String/bool/double。
        if (value is String || value is bool || value is double) {
          _cache[key] = value as Object;
        }
      }
      logInfo(_tag, 'SharedPreferences backend, ${_cache.length} entries');
    } catch (e, stack) {
      logError(_tag, 'init failed, using empty cache', e, stack);
    }
  }

  /// 测试专用：清空状态，供多个测试用例间隔离。
  @visibleForTesting
  void debugReset() {
    _cache.clear();
    _prefs = null;
    _initialized = false;
  }

  /// 读取字符串；不存在或类型不符时返回 null。
  String? getString(String key) {
    final value = _cache[key];
    return value is String ? value : null;
  }

  /// 读取布尔；不存在或类型不符时返回 null。
  bool? getBool(String key) {
    final value = _cache[key];
    return value is bool ? value : null;
  }

  /// 读取浮点；不存在或类型不符时返回 null。
  double? getDouble(String key) {
    final value = _cache[key];
    return value is double ? value : null;
  }

  /// 写入字符串。
  Future<void> setString(String key, String value) => _put(key, value);

  /// 写入布尔。
  Future<void> setBool(String key, bool value) => _put(key, value);

  /// 写入浮点。
  Future<void> setDouble(String key, double value) => _put(key, value);

  /// 删除键。
  Future<void> remove(String key) async {
    _cache.remove(key);
    await _prefs?.remove(key);
  }

  Future<void> _put(String key, Object value) async {
    _cache[key] = value;
    final prefs = _prefs;
    if (prefs == null) return;
    if (value is String) {
      await prefs.setString(key, value);
    } else if (value is bool) {
      await prefs.setBool(key, value);
    } else if (value is double) {
      await prefs.setDouble(key, value);
    }
  }
}
