// 应用级本地化装配回归测试（issue #515）：应用根是裸 WidgetsApp，Material
// TextField 与 shadcn ShadInput 的右键菜单/选区工具栏各自依赖
// MaterialLocalizations / ShadLocalizations，此前树上没有对应 delegate，
// ShadInput 一侧还会悄悄回退英文（不抛异常，容易被忽略）。
import 'package:flutter/material.dart' show MaterialLocalizations;
import 'package:flutter/widgets.dart';
import 'package:flutter/foundation.dart' show SynchronousFuture;
import 'package:flutter_test/flutter_test.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

import 'package:flux_down/src/i18n/framework_localizations.dart';

void main() {
  TestWidgetsFlutterBinding.ensureInitialized();

  test('frameworkLocale 解析 i18n 代码为 Material 本地化实际支持的 Locale', () {
    expect(frameworkLocale('zh'), const Locale('zh'));
    expect(frameworkLocale('xx'), const Locale('en'));
    expect(frameworkLocale(''), const Locale('en'));
    expect(frameworkLocale('zh-hant').scriptCode, 'Hant');
  });

  testWidgets('中文 WidgetsApp 根：Material 与 shadcn 本地化文案都跟随中文', (tester) async {
    final locale = frameworkLocale('zh');
    late BuildContext capturedContext;

    await tester.pumpWidget(
      WidgetsApp(
        color: const Color(0xFF000000),
        locale: locale,
        supportedLocales: [locale],
        localizationsDelegates: frameworkLocalizationDelegates,
        builder: (context, child) {
          capturedContext = context;
          return const SizedBox.shrink();
        },
      ),
    );
    await tester.pumpAndSettle();

    expect(tester.takeException(), isNull);
    expect(MaterialLocalizations.of(capturedContext).copyButtonLabel, '复制');
    expect(ShadLocalizations.of(capturedContext).input.copy, '复制');
  });

  test('预热后的语言在 AppShadLocalizationsDelegate.load 中同步命中缓存', () async {
    const locale = Locale('zh');
    await AppShadLocalizationsDelegate.warmUp(locale);

    final future = AppShadLocalizationsDelegate.delegate.load(locale);
    expect(future, isA<SynchronousFuture<ShadLocalizationsData>>());
    expect((await future).input.copy, '复制');
  });
}
