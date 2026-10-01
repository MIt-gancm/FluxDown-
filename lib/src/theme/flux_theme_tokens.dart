import 'package:flutter/material.dart' show Colors;
import 'package:flutter/widgets.dart';
import 'package:flutter/foundation.dart';

import 'flux_metric_tokens.dart';

// ═══════════════════════════════════════════════════════════
//  FluxThemeScope — InheritedWidget 向下传递 Token
// ═══════════════════════════════════════════════════════════

/// 通过 widget tree 向下传递当前生效的 [FluxThemeTokens]。
///
/// 在 main.dart 中包裹整个应用，AppColors.of(context) 通过此节点获取 tokens。
class FluxThemeScope extends InheritedWidget {
  final FluxThemeTokens tokens;

  const FluxThemeScope({super.key, required this.tokens, required super.child});

  static FluxThemeTokens of(BuildContext context) {
    final scope = context.dependOnInheritedWidgetOfExactType<FluxThemeScope>();
    assert(scope != null, 'FluxThemeScope not found in widget tree');
    return scope!.tokens;
  }

  @override
  bool updateShouldNotify(FluxThemeScope oldWidget) =>
      tokens != oldWidget.tokens;
}

// ═══════════════════════════════════════════════════════════
//  FluxThemeTokens — 主题 Token 数据类
// ═══════════════════════════════════════════════════════════

/// FluxDown 主题 Token 系统
///
/// 将所有 UI 颜色抽象为语义化 Token，支持 JSON 序列化/反序列化，
/// 允许用户完全自定义每个 UI 元素的颜色。
@immutable
class FluxThemeTokens {
  // ── 元数据 ──
  final String name;
  final String? author;
  final Brightness appearance;

  // ── Surface（表面/背景层级）──
  final Color background;
  final Color surface1;
  final Color surface2;
  final Color surface3;

  // ── Element（交互态）──
  final Color elementHover;
  final Color elementSelected;
  final Color elementActive;

  // ── Text（文字层级）──
  final Color textPrimary;
  final Color textSecondary;
  final Color textMuted;
  final Color textDisabled;

  // ── Border（边框）──
  final Color border;
  final Color borderFocused;

  // ── Accent（强调色系）──
  final Color accent;
  final Color accentHover;
  final Color accentBackground;
  final Color accentForeground;

  // ── Input（输入框）──
  final Color inputBackground;
  final Color inputBorder;
  final Color inputFocusBorder;
  final Color inputFocusBackground;

  // ── Dialog（对话框）──
  final Color dialogBackground;
  final Color dialogBarrier;

  // ── Switch（开关）──
  final Color switchTrack;
  final Color switchThumb;

  // ── Shadow（阴影基色）──
  final Color shadow;

  // ── Status（语义状态色）──
  final Color statusSuccess;
  final Color statusWarning;
  final Color statusError;

  // ── Segment Palette（分片调色板）──
  final List<Color> segmentPalette;

  // ── Metric（Layer1：圆角/间距/透明度等非颜色设计变量）──
  final FluxMetricTokens metric;

  const FluxThemeTokens({
    required this.name,
    this.author,
    required this.appearance,
    required this.background,
    required this.surface1,
    required this.surface2,
    required this.surface3,
    required this.elementHover,
    required this.elementSelected,
    required this.elementActive,
    required this.textPrimary,
    required this.textSecondary,
    required this.textMuted,
    required this.textDisabled,
    required this.border,
    required this.borderFocused,
    required this.accent,
    required this.accentHover,
    required this.accentBackground,
    required this.accentForeground,
    required this.inputBackground,
    required this.inputBorder,
    required this.inputFocusBorder,
    required this.inputFocusBackground,
    required this.dialogBackground,
    required this.dialogBarrier,
    required this.switchTrack,
    required this.switchThumb,
    required this.shadow,
    required this.statusSuccess,
    required this.statusWarning,
    required this.statusError,
    this.segmentPalette = defaultSegmentPalette,
    this.metric = FluxMetricTokens.standard,
  });

  // ── 默认分片调色板（自定义主题未提供 segmentPalette 时的占位值；
  //    运行时由 SegmentPalette 基于 accent 动态生成 256 色）──
  static const defaultSegmentPalette = <Color>[
    Color(0xFF22C55E),
    Color(0xFFF59E0B),
    Color(0xFFA855F7),
    Color(0xFF06B6D4),
    Color(0xFFEC4899),
    Color(0xFF14B8A6),
    Color(0xFFEF4444),
    Color(0xFF8B5CF6),
    Color(0xFFF97316),
    Color(0xFF10B981),
    Color(0xFFE11D48),
    Color(0xFF0EA5E9),
    Color(0xFFD946EF),
    Color(0xFF84CC16),
    Color(0xFF64748B),
    Color(0xFF3B82F6),
  ];

  // ═══════════════════════════════════════════════════════════
  //  内置预设
  // ═══════════════════════════════════════════════════════════

  /// 默认暗色主题（Apple 风格深灰）
  static FluxThemeTokens defaultDark({Color accent = const Color(0xFF3B82F6)}) {
    final hsl = HSLColor.fromColor(accent);
    final hover = hsl
        .withLightness((hsl.lightness + 0.08).clamp(0.0, 1.0))
        .toColor();
    final fg = _foregroundFor(accent);
    return FluxThemeTokens(
      name: 'Default Dark',
      appearance: Brightness.dark,
      // Surface
      background: const Color(0xFF1C1C1E),
      surface1: const Color(0xFF2C2C2E),
      surface2: const Color(0xFF3A3A3C),
      surface3: const Color(0xFF48484A),
      // Element
      elementHover: const Color(0xFF424245),
      elementSelected: const Color(0xFF3A3A3C),
      elementActive: accent.withValues(alpha: 0.18),
      // Text
      textPrimary: const Color(0xFFF5F5F7),
      textSecondary: const Color(0xFFA1A1A6),
      textMuted: const Color(0xFF8E8E93),
      textDisabled: const Color(0xFF8E8E93).withValues(alpha: 0.5),
      // Border
      border: const Color(0xFF48484A),
      borderFocused: accent,
      // Accent
      accent: accent,
      accentHover: hover,
      accentBackground: accent.withValues(alpha: 0.18),
      accentForeground: fg,
      // Input
      inputBackground: const Color(0xFF1C1C1E),
      inputBorder: const Color(0xFF48484A),
      inputFocusBorder: accent,
      inputFocusBackground: accent.withValues(alpha: 0.08),
      // Dialog
      dialogBackground: const Color(0xFF2C2C2E),
      dialogBarrier: const Color(0x40000000),
      // Switch
      switchTrack: const Color(0xFF636366),
      switchThumb: const Color(0xFFFFFFFF),
      // Shadow
      shadow: const Color(0xFF000000),
      // Status
      statusSuccess: const Color(0xFF22C55E),
      statusWarning: const Color(0xFFF59E0B),
      statusError: const Color(0xFFEF4444),
    );
  }

  /// 默认亮色主题
  static FluxThemeTokens defaultLight({
    Color accent = const Color(0xFF3B82F6),
  }) {
    final hsl = HSLColor.fromColor(accent);
    final hover = hsl
        .withLightness((hsl.lightness + 0.06).clamp(0.0, 1.0))
        .toColor();
    final fg = _foregroundFor(accent);
    return FluxThemeTokens(
      name: 'Default Light',
      appearance: Brightness.light,
      // Surface
      background: const Color(0xFFF8F9FA),
      surface1: const Color(0xFFFFFFFF),
      surface2: const Color(0xFFF1F3F5),
      surface3: const Color(0xFFE9ECEF),
      // Element
      elementHover: const Color(0xFFF1F3F5),
      elementSelected: accent.withValues(alpha: 0.10),
      elementActive: accent.withValues(alpha: 0.10),
      // Text
      textPrimary: const Color(0xFF09090B),
      textSecondary: const Color(0xFF71717A),
      textMuted: const Color(0xFFA1A1AA),
      textDisabled: const Color(0xFFA1A1AA).withValues(alpha: 0.5),
      // Border
      border: const Color(0xFFE4E4E7),
      borderFocused: accent,
      // Accent
      accent: accent,
      accentHover: hover,
      accentBackground: accent.withValues(alpha: 0.10),
      accentForeground: fg,
      // Input——字段用比面板深一档的浅灰填充（对话框/卡片是纯白，字段若
      // 同为白就只剩一圈细边框，层次感尽失）。经 app_theme 接入 ShadTheme
      // 的 input/select decoration，全局生效。
      inputBackground: const Color(0xFFF2F4F7),
      inputBorder: const Color(0xFFE4E4E7),
      inputFocusBorder: accent,
      inputFocusBackground: const Color(0xFFFFFFFF),
      // Dialog
      dialogBackground: const Color(0xFFFFFFFF),
      dialogBarrier: const Color(0x1A000000),
      // Switch
      switchTrack: const Color(0xFFE5E5EA),
      switchThumb: const Color(0xFFFFFFFF),
      // Shadow
      shadow: const Color(0xFF000000),
      // Status
      statusSuccess: const Color(0xFF22C55E),
      statusWarning: const Color(0xFFF59E0B),
      statusError: const Color(0xFFEF4444),
    );
  }

  /// 根据颜色亮度自动选择前景色
  static Color _foregroundFor(Color c) =>
      c.computeLuminance() > 0.5 ? const Color(0xFF09090B) : Colors.white;

  // ═══════════════════════════════════════════════════════════
  //  额外内置主题
  // ═══════════════════════════════════════════════════════════

  // ═══════════════════════════════════════════════════════════
  //  JSON 序列化
  // ═══════════════════════════════════════════════════════════

  // ═══════════════════════════════════════════════════════════
  //  copyWith
  // ═══════════════════════════════════════════════════════════

  @override
  bool operator ==(Object other) {
    if (identical(this, other)) return true;
    return other is FluxThemeTokens &&
        name == other.name &&
        author == other.author &&
        appearance == other.appearance &&
        background == other.background &&
        surface1 == other.surface1 &&
        surface2 == other.surface2 &&
        surface3 == other.surface3 &&
        elementHover == other.elementHover &&
        elementSelected == other.elementSelected &&
        elementActive == other.elementActive &&
        textPrimary == other.textPrimary &&
        textSecondary == other.textSecondary &&
        textMuted == other.textMuted &&
        textDisabled == other.textDisabled &&
        border == other.border &&
        borderFocused == other.borderFocused &&
        accent == other.accent &&
        accentHover == other.accentHover &&
        accentBackground == other.accentBackground &&
        accentForeground == other.accentForeground &&
        inputBackground == other.inputBackground &&
        inputBorder == other.inputBorder &&
        inputFocusBorder == other.inputFocusBorder &&
        inputFocusBackground == other.inputFocusBackground &&
        dialogBackground == other.dialogBackground &&
        dialogBarrier == other.dialogBarrier &&
        switchTrack == other.switchTrack &&
        switchThumb == other.switchThumb &&
        shadow == other.shadow &&
        statusSuccess == other.statusSuccess &&
        statusWarning == other.statusWarning &&
        statusError == other.statusError &&
        metric == other.metric &&
        listEquals(segmentPalette, other.segmentPalette);
  }

  @override
  int get hashCode => Object.hashAll([
    name,
    author,
    appearance,
    background,
    surface1,
    surface2,
    surface3,
    elementHover,
    elementSelected,
    elementActive,
    textPrimary,
    textSecondary,
    textMuted,
    textDisabled,
    border,
    borderFocused,
    accent,
    accentHover,
    accentBackground,
    accentForeground,
    inputBackground,
    inputBorder,
    inputFocusBorder,
    inputFocusBackground,
    dialogBackground,
    dialogBarrier,
    switchTrack,
    switchThumb,
    shadow,
    statusSuccess,
    statusWarning,
    statusError,
    metric,
    Object.hashAll(segmentPalette),
  ]);
}
