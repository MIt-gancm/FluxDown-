// 应用根是裸 WidgetsApp（main.dart / mobile_app.dart / popup_app.dart 均未挂
// MaterialApp / ShadApp），但两套输入控件都要求树上有对应的
// LocalizationsDelegate 才能取到本地化文案：
// - Material `TextField` 经 MaterialLocalizations 取「复制/粘贴/全选」等，
//   没有对应 delegate 会直接抛异常；
// - shadcn 的 `ShadInput` 右键菜单经 `ShadLocalizations.of(context)` 取同类
//   文案，没有对应 delegate 时不抛异常，而是悄悄回退
//   `ShadLocalizationsData()`（英文默认值）——此前应用语言切到中文，
//   ShadInput 菜单仍固定英文正是这个原因（issue #515）。
//
// 本文件提供三个 WidgetsApp 根统一挂载所需的东西：
// - [frameworkLocale]：把 i18n locale 代码解析为 Material/Widgets 本地化
//   实际支持的 [Locale]；
// - [AppShadLocalizationsDelegate] + [AppShadLocalizationsDelegate.warmUp]：
//   带同步缓存的 shadcn 本地化 delegate，配合预热避免异步加载期间的空白帧；
// - [frameworkLocalizationDelegates]：三个根共用的 delegate 列表。

import 'package:flutter/foundation.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_localizations/flutter_localizations.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

/// 把 i18n locale 代码（如 `'en'`、`'zh'`、`'zh-hant'`、`'zh-tw'`）解析成
/// Material/Widgets 本地化实际支持的 [Locale]；解析结果不受支持时回退英文，
/// 避免 `MaterialLocalizations.of(context)` 因未知语言抛出异常。
Locale frameworkLocale(String code) {
  final parts = code.split(RegExp(r'[-_]')).where((p) => p.isNotEmpty).toList();
  final Locale locale;
  if (parts.isEmpty) {
    // 空串/纯分隔符：Locale 构造器断言 languageCode 非空，直接回退英文。
    return const Locale('en');
  } else if (parts.length == 1) {
    locale = Locale(parts.first);
  } else {
    final language = parts[0];
    final sub = parts[1];
    locale = sub.length == 4
        // 4 字母子标签 → script（如 zh-Hant、zh-Hans），首字母大写。
        ? Locale.fromSubtags(
            languageCode: language,
            scriptCode: sub[0].toUpperCase() + sub.substring(1).toLowerCase(),
          )
        // 2-3 字母子标签 → country（如 zh-TW、en-US），全大写。
        : Locale.fromSubtags(
            languageCode: language,
            countryCode: sub.toUpperCase(),
          );
  }
  return GlobalMaterialLocalizations.delegate.isSupported(locale)
      ? locale
      : const Locale('en');
}

/// 包一层同步缓存的 [GlobalShadLocalizations]。
///
/// `GlobalShadLocalizations.load()` 对英文走同步路径，对其余语言经
/// deferred import 异步加载（`strings.g.dart` 里 `await l_zh.loadLibrary()`
/// 之类）。已挂载的 `Localizations` 在异步刷新期间会保留旧资源渲染、不会
/// 空白（见 flutter 源码 `_LocalizationsState.didUpdateWidget`/`load`），
/// 但**全新挂载**的 `Localizations`（如弹窗小窗每次新载荷都用
/// `ValueKey(_epoch)` 重建整棵 `WidgetsApp`）在资源到达前会渲染
/// `SizedBox.shrink()`，也就是一帧空白。
///
/// 命中缓存的语言直接同步返回，从根源避免这一帧空白；[warmUp] 用于在
/// 语言切换 / 弹窗投递新载荷触发 `setState` 之前，把目标语言预热进缓存。
class AppShadLocalizationsDelegate
    extends LocalizationsDelegate<ShadLocalizationsData> {
  const AppShadLocalizationsDelegate._();

  /// 单例。
  static const delegate = AppShadLocalizationsDelegate._();

  static final _cache = <String, ShadLocalizationsData>{};

  @override
  bool isSupported(Locale locale) => true;

  @override
  Future<ShadLocalizationsData> load(Locale locale) {
    final key = locale.toLanguageTag();
    final cached = _cache[key];
    if (cached != null) {
      return SynchronousFuture<ShadLocalizationsData>(cached);
    }
    return GlobalShadLocalizations.delegate.load(locale).then((data) {
      _cache[key] = data;
      return data;
    });
  }

  @override
  bool shouldReload(AppShadLocalizationsDelegate old) => false;

  /// 预热 [locale] 的 shadcn 本地化资源；返回后 [load] 同一 locale
  /// 会同步命中缓存，不再触发异步加载。
  static Future<void> warmUp(Locale locale) => delegate.load(locale);
}

/// 三个 WidgetsApp 根（main / mobile / popup）统一使用的本地化 delegate 列表。
const frameworkLocalizationDelegates = <LocalizationsDelegate<dynamic>>[
  AppShadLocalizationsDelegate.delegate,
  GlobalWidgetsLocalizations.delegate,
  GlobalMaterialLocalizations.delegate,
];
