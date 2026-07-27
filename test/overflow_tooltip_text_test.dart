import 'package:flutter/gestures.dart';
import 'package:flutter/widgets.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:flux_down/src/i18n/locale_provider.dart';
import 'package:flux_down/src/theme/app_theme.dart';
import 'package:flux_down/src/theme/flux_theme_tokens.dart';
import 'package:flux_down/src/widgets/overflow_tooltip_text.dart';
import 'package:shadcn_ui/shadcn_ui.dart';

const _longName =
    'FluxDown-0.2.2-checksums-windows-x64-portable-signed-release.zip';
const _shortName = 'a.zip';

Widget _wrap(Widget child, {double width = 160}) {
  final tokens = FluxThemeTokens.defaultLight();
  final theme = buildThemeFromTokens(tokens);
  return FluxThemeScope(
    tokens: tokens,
    child: ShadTheme(
      data: theme,
      child: LocaleScope(
        s: S.of('zh'),
        child: Directionality(
          textDirection: TextDirection.ltr,
          child: DefaultTextStyle(
            style: theme.textTheme.p.copyWith(
              color: theme.colorScheme.foreground,
            ),
            child: WidgetsApp(
              color: theme.colorScheme.primary,
              debugShowCheckedModeBanner: false,
              home: Center(child: SizedBox(width: width, child: child)),
              pageRouteBuilder: <T>(RouteSettings settings, WidgetBuilder b) {
                return PageRouteBuilder<T>(
                  settings: settings,
                  pageBuilder: (context, _, _) => b(context),
                );
              },
            ),
          ),
        ),
      ),
    ),
  );
}

/// 把鼠标移到 [finder] 中心并停在那里，返回可继续操作的 gesture。
Future<TestGesture> _hover(WidgetTester tester, Finder finder) async {
  final gesture = await tester.createGesture(kind: PointerDeviceKind.mouse);
  await gesture.addPointer(location: Offset.zero);
  addTearDown(gesture.removePointer);
  await tester.pump();
  await gesture.moveTo(tester.getCenter(finder));
  await tester.pump();
  return gesture;
}

void main() {
  testWidgets('未溢出的短文件名不挂 tooltip，避免弹出内容相同的冗余气泡', (tester) async {
    await tester.pumpWidget(_wrap(const OverflowTooltipText(_shortName)));

    expect(find.byType(ShadTooltip), findsNothing);
    expect(find.text(_shortName), findsOneWidget);
  });

  testWidgets('溢出的长文件名挂上 tooltip', (tester) async {
    await tester.pumpWidget(_wrap(const OverflowTooltipText(_longName)));

    expect(find.byType(ShadTooltip), findsOneWidget);
    // 气泡未触发时全名只渲染一次（就是被省略号截断的那一处）
    expect(find.text(_longName), findsOneWidget);
  });

  testWidgets('悬浮满 500ms 才弹出全名，不足 500ms 不弹', (tester) async {
    await tester.pumpWidget(_wrap(const OverflowTooltipText(_longName)));
    await _hover(tester, find.text(_longName));

    await tester.pump(const Duration(milliseconds: 400));
    expect(
      find.text(_longName),
      findsOneWidget,
      reason: '未满 500ms 不应弹出气泡',
    );

    await tester.pump(const Duration(milliseconds: 200));
    await tester.pumpAndSettle();
    expect(
      find.text(_longName),
      findsNWidgets(2),
      reason: '满 500ms 后气泡内应出现同名全称',
    );
  });

  testWidgets('移开鼠标后气泡收起', (tester) async {
    await tester.pumpWidget(_wrap(const OverflowTooltipText(_longName)));
    final gesture = await _hover(tester, find.text(_longName));

    await tester.pump(kOverflowTooltipDelay + const Duration(milliseconds: 50));
    await tester.pumpAndSettle();
    expect(find.text(_longName), findsNWidgets(2));

    await gesture.moveTo(Offset.zero);
    await tester.pumpAndSettle();
    expect(find.text(_longName), findsOneWidget);
  });

  testWidgets('maxLines=2 时按两行判定溢出（网格卡片文件名）', (tester) async {
    // 两行放得下 → 不挂 tooltip
    await tester.pumpWidget(
      _wrap(const OverflowTooltipText(_shortName, maxLines: 2), width: 60),
    );
    expect(find.byType(ShadTooltip), findsNothing);

    // 两行仍放不下 → 挂 tooltip
    await tester.pumpWidget(
      _wrap(const OverflowTooltipText(_longName, maxLines: 2), width: 60),
    );
    expect(find.byType(ShadTooltip), findsOneWidget);
  });

  testWidgets('挂了 tooltip 的文本不吞点击，行选中照常生效', (tester) async {
    var taps = 0;
    await tester.pumpWidget(
      _wrap(
        GestureDetector(
          onTap: () => taps++,
          child: const OverflowTooltipText(_longName),
        ),
      ),
    );
    // 前提：确实走的是挂了 ShadGestureDetector 的溢出分支
    expect(find.byType(ShadTooltip), findsOneWidget);

    await tester.tap(find.text(_longName));
    await tester.pump();
    expect(taps, 1);
  });
}
