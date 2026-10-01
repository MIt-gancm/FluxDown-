import 'package:flutter/foundation.dart';

// ═══════════════════════════════════════════════════════════
//  FluxMetricTokens — Layer1 非颜色设计变量
// ═══════════════════════════════════════════════════════════

/// FluxDown 主题的 Layer1 Token：命名圆角 / 命名透明度 / 间距 / 描边 /
/// 按钮尺寸 / 移动端几何。
///
/// 与 [FluxThemeTokens]（Layer0 颜色）平行的第二层设计变量，使圆角/尺寸/
/// 透明度也能像颜色一样被主题独立设计（编译期强类型字段而非运行时 registry）。
///
/// **值域夹取**：字段以私有原始值存储、公开 getter 懒夹取（`clamp` 不能写进
/// `const` 构造/初始化列表，否则触发 `const_eval_method_invocation` 编译错）。
/// `==` / [hashCode] / [toJson] 一律使用夹取后的 getter 值，保证同一有效值
/// round-trip 一致。
///
/// **向后兼容**：所有字段带默认值（= 迁移前最高频字面量，保证视觉不回归）；
/// [FluxMetricTokens.fromJson] 逐字段回退默认，旧主题 JSON 缺失 `metrics` 段
/// 时 [FluxThemeTokens] 用 `const FluxMetricTokens()` 兜底。
@immutable
class FluxMetricTokens {
  // ── 圆角角色（每个 circular(N) 值有明确归属）──
  final double _radiusProgress;
  final double _radiusXs;
  final double _radiusSegmentCell;
  final double _radiusSm;
  final double _radiusMd;
  final double _radiusInput;
  final double _radiusCard;
  final double _radiusIconTile;
  final double _radiusDialog;
  final double _radiusFieldMobile;
  final double _radiusChipLg;
  final double _radiusChipXl;
  final double _radiusBadge;
  final double _radiusPill;
  final double _radiusSheet;

  // ── 描边宽度 ──
  final double _strokeThin;
  final double _strokeThick;

  // ── 间距 ──
  final double _spacingXs;
  final double _spacingSm;
  final double _spacingMd;
  final double _spacingLg;
  final double _spacingXl;

  // ── 按钮高度 ──
  final double _buttonHeightSm;
  final double _buttonHeightMd;
  final double _buttonHeightLg;

  // ── 透明度角色（每用途一角色，杜绝非本意合并）──
  final double _alphaSubtle;
  final double _alphaSoft;
  final double _alphaMuted;
  final double _alphaMutedStrong;
  final double _alphaActive;
  final double _alphaSelectedBorder;
  final double _alphaScrim;
  final double _alphaBorder;
  final double _alphaBorderStrong;
  final double _alphaDisabled;
  final double _alphaGlass;
  final double _alphaFocusRing;
  final double _alphaShadowStrong;
  final double _alphaShadowSoft;
  final double _alphaShadowFaint;
  final double _alphaFaint;
  final double _alphaTextSelection;
  final double _alphaBorderSubtle;
  final double _alphaBorderFaint;
  final double _alphaBorderMedium;
  final double _alphaEmphasis;
  final double _alphaGlassSubtle;

  // ── 移动端几何（承接旧 MobileDims）──
  final double _mobilePageMargin;
  final double _mobileCardRadius;
  final double _mobileCardGap;
  final double _mobileAppBarHeight;
  final double _mobileTabsHeight;
  final double _mobileDockBottomGap;
  final double _mobileFabSize;
  final double _mobileScrollBottomPadding;

  /// 全字段具名带默认值（默认 = 迁移前最高频字面量）。构造函数纯赋值，
  /// 无方法调用，保证 `const` 合法、可作 [FluxThemeTokens] 默认参数值。
  const FluxMetricTokens({
    double radiusProgress = 1.5,
    double radiusXs = 2,
    double radiusSegmentCell = 2.5,
    double radiusSm = 4,
    double radiusMd = 6,
    double radiusInput = 8,
    double radiusCard = 8,
    double radiusIconTile = 9,
    double radiusDialog = 10,
    double radiusFieldMobile = 11,
    double radiusChipLg = 12,
    double radiusChipXl = 14,
    double radiusBadge = 18,
    double radiusPill = 999,
    double radiusSheet = 26,
    double strokeThin = 1,
    double strokeThick = 1.5,
    double spacingXs = 4,
    double spacingSm = 8,
    double spacingMd = 12,
    double spacingLg = 16,
    double spacingXl = 24,
    double buttonHeightSm = 28,
    double buttonHeightMd = 32,
    double buttonHeightLg = 36,
    double alphaSubtle = 0.08,
    double alphaSoft = 0.10,
    double alphaMuted = 0.12,
    double alphaMutedStrong = 0.14,
    double alphaActive = 0.18,
    double alphaSelectedBorder = 0.35,
    double alphaScrim = 0.45,
    double alphaBorder = 0.5,
    double alphaBorderStrong = 0.8,
    double alphaDisabled = 0.5,
    double alphaGlass = 0.72,
    double alphaFocusRing = 0.6,
    double alphaShadowStrong = 0.25,
    double alphaShadowSoft = 0.16,
    double alphaShadowFaint = 0.08,
    double alphaFaint = 0.06,
    double alphaTextSelection = 0.25,
    double alphaBorderSubtle = 0.3,
    double alphaBorderFaint = 0.4,
    double alphaBorderMedium = 0.6,
    double alphaEmphasis = 0.7,
    double alphaGlassSubtle = 0.55,
    double mobilePageMargin = 16,
    double mobileCardRadius = 12,
    double mobileCardGap = 10,
    double mobileAppBarHeight = 56,
    double mobileTabsHeight = 44,
    double mobileDockBottomGap = 16,
    double mobileFabSize = 46,
    double mobileScrollBottomPadding = 120,
  }) : _radiusProgress = radiusProgress,
       _radiusXs = radiusXs,
       _radiusSegmentCell = radiusSegmentCell,
       _radiusSm = radiusSm,
       _radiusMd = radiusMd,
       _radiusInput = radiusInput,
       _radiusCard = radiusCard,
       _radiusIconTile = radiusIconTile,
       _radiusDialog = radiusDialog,
       _radiusFieldMobile = radiusFieldMobile,
       _radiusChipLg = radiusChipLg,
       _radiusChipXl = radiusChipXl,
       _radiusBadge = radiusBadge,
       _radiusPill = radiusPill,
       _radiusSheet = radiusSheet,
       _strokeThin = strokeThin,
       _strokeThick = strokeThick,
       _spacingXs = spacingXs,
       _spacingSm = spacingSm,
       _spacingMd = spacingMd,
       _spacingLg = spacingLg,
       _spacingXl = spacingXl,
       _buttonHeightSm = buttonHeightSm,
       _buttonHeightMd = buttonHeightMd,
       _buttonHeightLg = buttonHeightLg,
       _alphaSubtle = alphaSubtle,
       _alphaSoft = alphaSoft,
       _alphaMuted = alphaMuted,
       _alphaMutedStrong = alphaMutedStrong,
       _alphaActive = alphaActive,
       _alphaSelectedBorder = alphaSelectedBorder,
       _alphaScrim = alphaScrim,
       _alphaBorder = alphaBorder,
       _alphaBorderStrong = alphaBorderStrong,
       _alphaDisabled = alphaDisabled,
       _alphaGlass = alphaGlass,
       _alphaFocusRing = alphaFocusRing,
       _alphaShadowStrong = alphaShadowStrong,
       _alphaShadowSoft = alphaShadowSoft,
       _alphaShadowFaint = alphaShadowFaint,
       _alphaFaint = alphaFaint,
       _alphaTextSelection = alphaTextSelection,
       _alphaBorderSubtle = alphaBorderSubtle,
       _alphaBorderFaint = alphaBorderFaint,
       _alphaBorderMedium = alphaBorderMedium,
       _alphaEmphasis = alphaEmphasis,
       _alphaGlassSubtle = alphaGlassSubtle,
       _mobilePageMargin = mobilePageMargin,
       _mobileCardRadius = mobileCardRadius,
       _mobileCardGap = mobileCardGap,
       _mobileAppBarHeight = mobileAppBarHeight,
       _mobileTabsHeight = mobileTabsHeight,
       _mobileDockBottomGap = mobileDockBottomGap,
       _mobileFabSize = mobileFabSize,
       _mobileScrollBottomPadding = mobileScrollBottomPadding;

  /// 全默认 Layer1（圆角/间距不区分 dark/light）。
  static const FluxMetricTokens standard = FluxMetricTokens();

  // ── 夹取上下限 ──
  static double _clampGeom(double v) => v.clamp(0, 2000).toDouble();
  static double _clampStroke(double v) => v.clamp(0, 8).toDouble();
  static double _clampAlpha(double v) => v.clamp(0, 1).toDouble();

  // ── 圆角 getter（夹取后值）──
  double get radiusProgress => _clampGeom(_radiusProgress);
  double get radiusXs => _clampGeom(_radiusXs);
  double get radiusSegmentCell => _clampGeom(_radiusSegmentCell);
  double get radiusSm => _clampGeom(_radiusSm);
  double get radiusMd => _clampGeom(_radiusMd);
  double get radiusInput => _clampGeom(_radiusInput);
  double get radiusCard => _clampGeom(_radiusCard);
  double get radiusIconTile => _clampGeom(_radiusIconTile);
  double get radiusDialog => _clampGeom(_radiusDialog);
  double get radiusFieldMobile => _clampGeom(_radiusFieldMobile);
  double get radiusChipLg => _clampGeom(_radiusChipLg);
  double get radiusChipXl => _clampGeom(_radiusChipXl);
  double get radiusBadge => _clampGeom(_radiusBadge);
  double get radiusPill => _clampGeom(_radiusPill);
  double get radiusSheet => _clampGeom(_radiusSheet);

  // ── 描边 getter ──
  double get strokeThin => _clampStroke(_strokeThin);
  double get strokeThick => _clampStroke(_strokeThick);

  // ── 间距 getter ──
  double get spacingXs => _clampGeom(_spacingXs);
  double get spacingSm => _clampGeom(_spacingSm);
  double get spacingMd => _clampGeom(_spacingMd);
  double get spacingLg => _clampGeom(_spacingLg);
  double get spacingXl => _clampGeom(_spacingXl);

  // ── 按钮高度 getter ──
  double get buttonHeightSm => _clampGeom(_buttonHeightSm);
  double get buttonHeightMd => _clampGeom(_buttonHeightMd);
  double get buttonHeightLg => _clampGeom(_buttonHeightLg);

  // ── 透明度 getter ──
  double get alphaSubtle => _clampAlpha(_alphaSubtle);
  double get alphaSoft => _clampAlpha(_alphaSoft);
  double get alphaMuted => _clampAlpha(_alphaMuted);
  double get alphaMutedStrong => _clampAlpha(_alphaMutedStrong);
  double get alphaActive => _clampAlpha(_alphaActive);
  double get alphaSelectedBorder => _clampAlpha(_alphaSelectedBorder);
  double get alphaScrim => _clampAlpha(_alphaScrim);
  double get alphaBorder => _clampAlpha(_alphaBorder);
  double get alphaBorderStrong => _clampAlpha(_alphaBorderStrong);
  double get alphaDisabled => _clampAlpha(_alphaDisabled);
  double get alphaGlass => _clampAlpha(_alphaGlass);
  double get alphaFocusRing => _clampAlpha(_alphaFocusRing);
  double get alphaShadowStrong => _clampAlpha(_alphaShadowStrong);
  double get alphaShadowSoft => _clampAlpha(_alphaShadowSoft);
  double get alphaShadowFaint => _clampAlpha(_alphaShadowFaint);
  double get alphaFaint => _clampAlpha(_alphaFaint);
  double get alphaTextSelection => _clampAlpha(_alphaTextSelection);
  double get alphaBorderSubtle => _clampAlpha(_alphaBorderSubtle);
  double get alphaBorderFaint => _clampAlpha(_alphaBorderFaint);
  double get alphaBorderMedium => _clampAlpha(_alphaBorderMedium);
  double get alphaEmphasis => _clampAlpha(_alphaEmphasis);
  double get alphaGlassSubtle => _clampAlpha(_alphaGlassSubtle);

  // ── 移动端几何 getter ──
  double get mobilePageMargin => _clampGeom(_mobilePageMargin);
  double get mobileCardRadius => _clampGeom(_mobileCardRadius);
  double get mobileCardGap => _clampGeom(_mobileCardGap);

  /// 布局骨架：改动影响顶栏可点击区高度，主题作者慎调。
  double get mobileAppBarHeight => _clampGeom(_mobileAppBarHeight);

  /// 布局骨架：改动影响 Tab 可点击区高度，主题作者慎调。
  double get mobileTabsHeight => _clampGeom(_mobileTabsHeight);
  double get mobileDockBottomGap => _clampGeom(_mobileDockBottomGap);
  double get mobileFabSize => _clampGeom(_mobileFabSize);
  double get mobileScrollBottomPadding =>
      _clampGeom(_mobileScrollBottomPadding);

  // ═══════════════════════════════════════════════════════════
  //  JSON 序列化（值取夹取后 getter）
  // ═══════════════════════════════════════════════════════════

  // ═══════════════════════════════════════════════════════════
  //  copyWith（接受原始值）
  // ═══════════════════════════════════════════════════════════

  // ═══════════════════════════════════════════════════════════
  //  == / hashCode（全字段 getter 值）
  // ═══════════════════════════════════════════════════════════

  /// 夹取后 getter 值的有序列表，供 [==] / [hashCode] 复用（防漏字段）。
  List<double> get _fields => [
    radiusProgress,
    radiusXs,
    radiusSegmentCell,
    radiusSm,
    radiusMd,
    radiusInput,
    radiusCard,
    radiusIconTile,
    radiusDialog,
    radiusFieldMobile,
    radiusChipLg,
    radiusChipXl,
    radiusBadge,
    radiusPill,
    radiusSheet,
    strokeThin,
    strokeThick,
    spacingXs,
    spacingSm,
    spacingMd,
    spacingLg,
    spacingXl,
    buttonHeightSm,
    buttonHeightMd,
    buttonHeightLg,
    alphaSubtle,
    alphaSoft,
    alphaMuted,
    alphaMutedStrong,
    alphaActive,
    alphaSelectedBorder,
    alphaScrim,
    alphaBorder,
    alphaBorderStrong,
    alphaDisabled,
    alphaGlass,
    alphaFocusRing,
    alphaShadowStrong,
    alphaShadowSoft,
    alphaShadowFaint,
    alphaFaint,
    alphaTextSelection,
    alphaBorderSubtle,
    alphaBorderFaint,
    alphaBorderMedium,
    alphaEmphasis,
    alphaGlassSubtle,
    mobilePageMargin,
    mobileCardRadius,
    mobileCardGap,
    mobileAppBarHeight,
    mobileTabsHeight,
    mobileDockBottomGap,
    mobileFabSize,
    mobileScrollBottomPadding,
  ];

  @override
  bool operator ==(Object other) =>
      identical(this, other) ||
      other is FluxMetricTokens && listEquals(_fields, other._fields);

  @override
  int get hashCode => Object.hashAll(_fields);
}
