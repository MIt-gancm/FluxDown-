/**
 * 预览:设置页(导航侧栏 + 外观/下载/行为/状态卡片),承载主窗口未出现的 token
 * (开关、对话框、多数 alpha、圆角/间距/描边/按钮高度)。
 */
import type { ReactNode } from "react";
import { Check, Download, Globe, Palette, Settings, X } from "lucide-react";
import { argbToCssRgba, type FluxThemeJson } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { alphaOf, num, rgba, tokenAttrs } from "./tokens";

type MockMessages = ThemeBuilderMessages["mock"];

/** 对应客户端设置卡片:dialog 圆角 + borderMedium 边框 + spacing 内距。 */
function SettingsCard({ theme, title, description, children }: { theme: FluxThemeJson; title: string; description?: string; children: ReactNode }) {
  return (
    <div
      style={{
        backgroundColor: rgba(theme, "colors.surface.surface1"),
        borderRadius: num(theme, "metrics.radius.dialog"),
        border: `${num(theme, "metrics.stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.borderMedium")}`,
        padding: `${num(theme, "metrics.spacing.md")}px ${num(theme, "metrics.spacing.lg")}px`,
      }}
      {...tokenAttrs("colors.surface.surface1", "colors.border.default", "metrics.radius.dialog", "metrics.stroke.thin", "metrics.alpha.borderMedium", "metrics.spacing.lg", "metrics.spacing.md")}
    >
      <div className="text-[12px] font-semibold" style={{ color: rgba(theme, "colors.text.primary") }} {...tokenAttrs("colors.text.primary")}>
        {title}
      </div>
      {description && (
        <div className="mt-0.5 text-[10px]" style={{ color: rgba(theme, "colors.text.muted") }} {...tokenAttrs("colors.text.muted")}>
          {description}
        </div>
      )}
      <div style={{ marginTop: num(theme, "metrics.spacing.sm") }} {...tokenAttrs("metrics.spacing.sm")}>
        {children}
      </div>
    </div>
  );
}

export function SettingsView({ theme, t, onBack }: { theme: FluxThemeJson; t: MockMessages; onBack: () => void }) {
  const accent = rgba(theme, "colors.accent.color");
  const accentFg = rgba(theme, "colors.accent.foreground");
  const textPrimary = rgba(theme, "colors.text.primary");
  const textSecondary = rgba(theme, "colors.text.secondary");
  const textMuted = rgba(theme, "colors.text.muted");
  const n = (path: string) => num(theme, `metrics.${path}`);
  const nav = [
    { key: "general", label: t.settingsGeneral, Icon: Settings },
    { key: "appearance", label: t.settingsAppearance, Icon: Palette },
    { key: "download", label: t.settingsDownload, Icon: Download },
    { key: "language", label: t.settingsLanguage, Icon: Globe },
  ];

  return (
    <div className="flex min-h-0 flex-1">
      <aside
        className="hidden w-[160px] shrink-0 flex-col border-r @2xl:flex"
        style={{ borderColor: rgba(theme, "colors.border.default"), backgroundColor: rgba(theme, "colors.surface.surface1") }}
        {...tokenAttrs("colors.surface.surface1", "colors.border.default")}
      >
        <button
          type="button"
          onClick={onBack}
          className="m-2 inline-flex items-center gap-1.5 self-start px-2 py-1 text-[11px] font-medium"
          style={{ color: textSecondary, borderRadius: n("radius.md"), backgroundColor: alphaOf(theme, "colors.accent.color", "metrics.alpha.soft") }}
          {...tokenAttrs("colors.text.secondary", "colors.accent.color", "metrics.radius.md", "metrics.alpha.soft")}
        >
          <X className="h-3 w-3" aria-hidden />
          {t.settingsBack}
        </button>
        <div className="mt-1 space-y-0.5 px-2">
          {nav.map(({ key, label, Icon }, index) => {
            const active = index === 1;
            return (
              <div
                key={key}
                className="flex h-8 items-center gap-2 px-2 text-[11px]"
                style={{ color: active ? accent : textSecondary, backgroundColor: active ? rgba(theme, "colors.element.selected") : "transparent", borderRadius: n("radius.md") }}
                {...tokenAttrs("colors.element.selected", "colors.accent.color", "colors.text.secondary", "metrics.radius.md")}
              >
                <Icon className="h-3.5 w-3.5" aria-hidden />
                {label}
              </div>
            );
          })}
        </div>
      </aside>

      <div
        className="min-h-0 flex-1 overflow-y-auto [scrollbar-width:thin]"
        style={{ backgroundColor: rgba(theme, "colors.surface.background"), scrollbarColor: `${rgba(theme, "colors.surface.surface3")} transparent` }}
        {...tokenAttrs("colors.surface.background")}
      >
        <div className="flex flex-col" style={{ gap: n("spacing.xl"), padding: n("spacing.lg") }} {...tokenAttrs("metrics.spacing.xl", "metrics.spacing.lg")}>
          {/* 主题选择:迷你窗口卡片 */}
          <SettingsCard theme={theme} title={t.settingsThemeSelect} description={t.settingsThemeSelectHint}>
            <div className="flex flex-wrap" style={{ gap: n("spacing.sm") }} {...tokenAttrs("metrics.spacing.sm")}>
              {[0, 1, 2].map((i) => {
                const selected = i === 1;
                return (
                  <div
                    key={i}
                    className="w-[110px] p-2"
                    style={{
                      backgroundColor: rgba(theme, "colors.surface.surface1"),
                      borderRadius: n("radius.dialog"),
                      border: `${selected ? n("stroke.thick") : n("stroke.thin")}px solid ${selected ? accent : alphaOf(theme, "colors.border.default", "metrics.alpha.borderMedium")}`,
                      boxShadow: selected
                        ? `0 4px 12px ${alphaOf(theme, "colors.shadow", "metrics.alpha.shadowSoft")}`
                        : `0 1px 3px ${alphaOf(theme, "colors.shadow", "metrics.alpha.shadowFaint")}`,
                    }}
                    {...tokenAttrs("metrics.radius.dialog", "metrics.stroke.thick", "metrics.stroke.thin", "metrics.alpha.borderMedium", "metrics.alpha.shadowSoft", "metrics.alpha.shadowFaint")}
                  >
                    <div
                      className="flex h-[52px] overflow-hidden"
                      style={{
                        backgroundColor: rgba(theme, "colors.surface.background"),
                        borderRadius: n("radius.md"),
                        border: `${n("stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.borderFaint")}`,
                      }}
                      {...tokenAttrs("metrics.radius.md", "metrics.alpha.borderFaint")}
                    >
                      <div className="flex w-7 flex-col justify-center gap-1 px-1" style={{ backgroundColor: rgba(theme, "colors.surface.surface1") }}>
                        <div className="h-[3px] w-4" style={{ backgroundColor: accent, borderRadius: n("radius.progress") }} {...tokenAttrs("metrics.radius.progress")} />
                        <div className="h-[3px] w-4" style={{ backgroundColor: alphaOf(theme, "colors.text.muted", "metrics.alpha.borderSubtle"), borderRadius: n("radius.progress") }} {...tokenAttrs("metrics.alpha.borderSubtle")} />
                        <div className="h-[3px] w-4" style={{ backgroundColor: alphaOf(theme, "colors.text.muted", "metrics.alpha.borderSubtle"), borderRadius: n("radius.progress") }} />
                      </div>
                      <div className="flex flex-1 flex-col justify-center gap-1 px-1">
                        <div className="h-[3px] w-full" style={{ backgroundColor: alphaOf(theme, "colors.text.primary", "metrics.alpha.borderFaint"), borderRadius: n("radius.progress") }} />
                        <div className="h-[3px] w-3/4" style={{ backgroundColor: alphaOf(theme, "colors.text.muted", "metrics.alpha.faint"), borderRadius: n("radius.progress") }} {...tokenAttrs("metrics.alpha.faint")} />
                        <div className="h-[5px] w-full" style={{ backgroundColor: rgba(theme, "colors.surface.surface3"), borderRadius: n("radius.xs") }} {...tokenAttrs("colors.surface.surface3", "metrics.radius.xs")}>
                          <div className="h-full w-1/2" style={{ backgroundColor: accent, borderRadius: n("radius.xs") }} />
                        </div>
                      </div>
                    </div>
                    <div className="mt-1.5 flex items-center justify-between">
                      <span className="text-[10px]" style={{ color: selected ? accent : textSecondary }}>
                        {t.settingsThemeName(i + 1)}
                      </span>
                      {selected && <Check className="h-3 w-3" style={{ color: accent }} aria-hidden />}
                    </div>
                  </div>
                );
              })}
            </div>
          </SettingsCard>

          {/* 主题色:色点 */}
          <SettingsCard theme={theme} title={t.settingsColorScheme}>
            <div className="flex flex-wrap items-center" style={{ gap: n("spacing.sm") }} {...tokenAttrs("metrics.spacing.sm")}>
              {theme.colors.segmentPalette.slice(0, 16).map((c, i) => {
                const selected = i === 0;
                return (
                  <div
                    key={i}
                    className="h-6 w-6"
                    style={{
                      backgroundColor: argbToCssRgba(c),
                      borderRadius: n("radius.pill"),
                      border: `${n("stroke.thick")}px solid ${selected ? alphaOf(theme, "colors.accent.color", "metrics.alpha.selectedBorder") : alphaOf(theme, "colors.border.default", "metrics.alpha.borderMedium")}`,
                      boxShadow: selected ? `0 0 6px ${alphaOf(theme, "colors.shadow", "metrics.alpha.shadowStrong")}` : "none",
                    }}
                    {...tokenAttrs(`colors.segmentPalette.${i}`, "metrics.radius.pill", "metrics.stroke.thick", "metrics.alpha.selectedBorder", "metrics.alpha.borderMedium", "metrics.alpha.shadowStrong")}
                  />
                );
              })}
            </div>
          </SettingsCard>

          {/* 语言 / 缩放:chip 段控 */}
          <SettingsCard theme={theme} title={t.settingsLanguage}>
            <div className="flex" style={{ gap: n("spacing.xs") }} {...tokenAttrs("metrics.spacing.xs")}>
              {["English", "简体中文"].map((label, i) => {
                const selected = i === 0;
                return (
                  <div
                    key={label}
                    className="px-3 py-1 text-[11px]"
                    style={{
                      color: selected ? accent : textSecondary,
                      backgroundColor: selected ? alphaOf(theme, "colors.accent.color", "metrics.alpha.active") : "transparent",
                      borderRadius: n("radius.chipLg"),
                      border: `${n("stroke.thin")}px solid ${selected ? alphaOf(theme, "colors.accent.color", "metrics.alpha.selectedBorder") : alphaOf(theme, "colors.border.default", "metrics.alpha.border")}`,
                    }}
                    {...tokenAttrs("metrics.radius.chipLg", "metrics.alpha.active", "metrics.alpha.selectedBorder", "metrics.alpha.border", "colors.accent.color")}
                  >
                    {label}
                  </div>
                );
              })}
            </div>
            <div className="mt-2 flex" style={{ gap: n("spacing.xs") }}>
              {["100%", "125%", "150%"].map((scale, i) => {
                const selected = i === 0;
                return (
                  <div
                    key={scale}
                    className="px-2.5 py-1 text-[11px]"
                    style={{
                      color: selected ? accent : textSecondary,
                      backgroundColor: selected ? alphaOf(theme, "colors.accent.color", "metrics.alpha.subtle") : rgba(theme, "colors.surface.surface1"),
                      borderRadius: n("radius.chipXl"),
                      border: `${n("stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.border")}`,
                    }}
                    {...tokenAttrs("metrics.radius.chipXl", "metrics.alpha.subtle")}
                  >
                    {scale}
                  </div>
                );
              })}
            </div>
          </SettingsCard>

          {/* 下载:输入框 + 分割线 + 三档按钮 */}
          <SettingsCard theme={theme} title={t.settingsDownload} description={t.settingsDownloadHint}>
            <div
              className="flex w-full items-center px-2.5 text-[11px]"
              style={{
                height: n("button.heightMd"),
                color: textPrimary,
                backgroundColor: rgba(theme, "colors.input.background"),
                borderRadius: n("radius.input"),
                border: `${n("stroke.thick")}px solid ${alphaOf(theme, "colors.input.focusBorder", "metrics.alpha.focusRing")}`,
              }}
              {...tokenAttrs("colors.input.background", "colors.input.focusBorder", "metrics.radius.input", "metrics.button.heightMd", "metrics.stroke.thick", "metrics.alpha.focusRing")}
            >
              D:\Downloads\FluxDown
            </div>
            <div className="mt-1.5 text-[10px]" style={{ color: textMuted }}>
              <span style={{ backgroundColor: alphaOf(theme, "colors.accent.color", "metrics.alpha.textSelection"), color: textPrimary }} {...tokenAttrs("metrics.alpha.textSelection")}>
                {t.settingsSelectionSample}
              </span>
            </div>
            <div
              className="my-2"
              style={{ height: n("stroke.thin"), backgroundColor: alphaOf(theme, "colors.border.default", "metrics.alpha.borderFaint") }}
              {...tokenAttrs("metrics.stroke.thin", "metrics.alpha.borderFaint")}
            />
            <div className="flex flex-wrap items-center" style={{ gap: n("spacing.sm") }}>
              <div
                className="flex items-center px-2.5 text-[11px] font-medium"
                style={{ height: n("button.heightSm"), color: textSecondary, backgroundColor: rgba(theme, "colors.surface.surface2"), borderRadius: n("radius.sm"), border: `${n("stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.borderSubtle")}` }}
                {...tokenAttrs("metrics.button.heightSm", "metrics.radius.sm", "metrics.alpha.borderSubtle")}
              >
                {t.settingsBtnSmall}
              </div>
              <div
                className="flex items-center px-3 text-[11px] font-semibold"
                style={{ height: n("button.heightMd"), color: accentFg, backgroundColor: accent, borderRadius: n("radius.md") }}
                {...tokenAttrs("metrics.button.heightMd", "metrics.radius.md")}
              >
                {t.settingsBtnMedium}
              </div>
              <div
                className="flex items-center px-4 text-[11px] font-semibold"
                style={{ height: n("button.heightLg"), color: accentFg, backgroundColor: accent, borderRadius: n("radius.md") }}
                {...tokenAttrs("metrics.button.heightLg")}
              >
                {t.settingsBtnLarge}
              </div>
            </div>
          </SettingsCard>

          {/* 行为:开关 + 玻璃面板 + 禁用态 + 遮罩 */}
          <SettingsCard theme={theme} title={t.settingsMisc}>
            <div className="flex items-center justify-between">
              <span className="text-[11px]" style={{ color: textSecondary }}>
                {t.settingsSwitchOn}
              </span>
              <div
                className="flex h-4 w-7 items-center justify-end px-0.5"
                style={{ backgroundColor: accent, borderRadius: n("radius.pill") }}
                {...tokenAttrs("colors.switch.track", "colors.switch.thumb", "metrics.radius.pill")}
              >
                <div className="h-3 w-3" style={{ backgroundColor: rgba(theme, "colors.switch.thumb"), borderRadius: n("radius.pill") }} />
              </div>
            </div>
            <div className="mt-1.5 flex items-center justify-between">
              <span className="text-[11px]" style={{ color: alphaOf(theme, "colors.text.primary", "metrics.alpha.disabled") }} {...tokenAttrs("metrics.alpha.disabled")}>
                {t.settingsSwitchOff}
              </span>
              <div
                className="flex h-4 w-7 items-center px-0.5"
                style={{ backgroundColor: rgba(theme, "colors.switch.track"), borderRadius: n("radius.pill") }}
                {...tokenAttrs("colors.switch.track", "metrics.radius.pill")}
              >
                <div className="h-3 w-3" style={{ backgroundColor: alphaOf(theme, "colors.switch.thumb", "metrics.alpha.mutedStrong"), borderRadius: n("radius.pill") }} {...tokenAttrs("metrics.alpha.mutedStrong")} />
              </div>
            </div>
            <div
              className="mt-2 flex items-center gap-2 px-2.5 py-1.5"
              style={{ backgroundColor: alphaOf(theme, "colors.surface.surface2", "metrics.alpha.glass"), borderRadius: n("radius.chipXl"), border: `${n("stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.emphasis")}` }}
              {...tokenAttrs("metrics.alpha.glass", "metrics.radius.chipXl", "metrics.alpha.emphasis")}
            >
              <span className="h-2 w-2" style={{ backgroundColor: alphaOf(theme, "colors.text.secondary", "metrics.alpha.borderStrong"), borderRadius: n("radius.pill") }} {...tokenAttrs("metrics.alpha.borderStrong")} />
              <span className="text-[10px]" style={{ color: textMuted }}>
                {t.settingsGlassPanel}
              </span>
            </div>
            <div
              className="mt-1.5 flex items-center gap-2 px-2.5 py-1.5"
              style={{ backgroundColor: alphaOf(theme, "colors.surface.surface3", "metrics.alpha.glassSubtle"), borderRadius: n("radius.chipXl") }}
              {...tokenAttrs("metrics.alpha.glassSubtle")}
            >
              <span className="h-2 w-2" style={{ backgroundColor: alphaOf(theme, "colors.text.muted", "metrics.alpha.muted"), borderRadius: n("radius.pill") }} {...tokenAttrs("metrics.alpha.muted")} />
              <span className="text-[10px]" style={{ color: textMuted }}>
                {t.settingsGlassSubtle}
              </span>
            </div>
            <div
              className="mt-2 flex items-center justify-center py-2 text-[10px]"
              style={{ backgroundColor: alphaOf(theme, "colors.dialog.barrier", "metrics.alpha.scrim"), color: textMuted, borderRadius: n("radius.sm") }}
              {...tokenAttrs("colors.dialog.barrier", "metrics.alpha.scrim")}
            >
              {t.settingsScrim}
            </div>
          </SettingsCard>

          {/* 状态与对话框:hover / badge / disabled / focused / dialog */}
          <SettingsCard theme={theme} title={t.settingsStates}>
            <div className="flex flex-wrap items-center" style={{ gap: n("spacing.sm") }}>
              <div
                className="px-3 py-1 text-[11px] font-semibold"
                style={{ color: accentFg, backgroundColor: rgba(theme, "colors.accent.hover"), borderRadius: n("radius.md") }}
                {...tokenAttrs("colors.accent.hover", "colors.accent.foreground")}
              >
                {t.settingsStateHover}
              </div>
              <span
                className="px-2 py-0.5 text-[10px] font-medium"
                style={{ color: accent, backgroundColor: rgba(theme, "colors.accent.background"), borderRadius: n("radius.badge") }}
                {...tokenAttrs("colors.accent.background", "colors.accent.color", "metrics.radius.badge")}
              >
                {t.settingsStateBadge}
              </span>
              <span
                className="px-2 py-0.5 text-[10px]"
                style={{ color: alphaOf(theme, "colors.text.disabled", "metrics.alpha.disabled"), backgroundColor: rgba(theme, "colors.element.active"), borderRadius: n("radius.sm") }}
                {...tokenAttrs("colors.text.disabled", "colors.element.active")}
              >
                {t.settingsStateDisabled}
              </span>
            </div>
            <div
              className="mt-2 w-full px-2.5 py-1.5 text-[11px]"
              style={{ color: textPrimary, backgroundColor: rgba(theme, "colors.input.focusBackground"), borderRadius: n("radius.input"), border: `${n("stroke.thick")}px solid ${rgba(theme, "colors.border.focused")}` }}
              {...tokenAttrs("colors.input.focusBackground", "colors.border.focused")}
            >
              {t.settingsStateFocused}
            </div>
            <div
              className="mt-2 px-3 py-2 text-[10px]"
              style={{ color: textSecondary, backgroundColor: rgba(theme, "colors.dialog.background"), borderRadius: n("radius.dialog"), border: `${n("stroke.thin")}px solid ${alphaOf(theme, "colors.border.default", "metrics.alpha.borderFaint")}` }}
              {...tokenAttrs("colors.dialog.background")}
            >
              {t.settingsStateDialog}
            </div>
          </SettingsCard>
        </div>
      </div>
    </div>
  );
}
