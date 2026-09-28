/**
 * 桌面客户端模拟窗口:标题栏 → 活动栏 + 路由内容(下载 / 设置)→ 状态栏,
 * 与 GPUI 客户端 ShellView 的布局一致。所有颜色 / 尺寸均取自当前编辑中的主题。
 */
import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { ArrowDownToLine, Minus, Moon, Search, Settings, Square, Sun, X } from "lucide-react";
import { tokenArea, type FluxThemeJson, type PreviewArea } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { alphaOf, num, rgba, tokenAttrs } from "./tokens";
import { PREVIEW_TASKS } from "./preview-data";
import { DownloadsView, type DownloadsState } from "./DownloadsView";
import { SettingsView } from "./SettingsView";

type MockMessages = ThemeBuilderMessages["mock"];

function RailButton({
  theme,
  label,
  selected,
  onClick,
  children,
}: {
  theme: FluxThemeJson;
  label: string;
  selected?: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      title={label}
      aria-label={label}
      aria-pressed={selected}
      onClick={onClick}
      className="relative mx-auto grid h-8 w-8 place-items-center hover:bg-[var(--tb-hover)]"
      style={{
        color: selected ? rgba(theme, "colors.text.primary") : rgba(theme, "colors.text.muted"),
        backgroundColor: selected ? rgba(theme, "colors.element.selected") : undefined,
        borderRadius: num(theme, "metrics.radius.md"),
        "--tb-hover": rgba(theme, "colors.element.hover"),
      } as CSSProperties}
      {...tokenAttrs("colors.element.selected", "colors.element.hover", "colors.text.primary", "colors.text.muted", "metrics.radius.md")}
    >
      {children}
    </button>
  );
}

export function PreviewWindow({
  theme,
  t,
  focusedPath,
  onToggleAppearance,
}: {
  theme: FluxThemeJson;
  t: MockMessages;
  focusedPath: string | null;
  onToggleAppearance: () => void;
}) {
  const [view, setView] = useState<PreviewArea>("downloads");
  const [downloads, setDownloads] = useState<DownloadsState>({ status: "all", category: "all", selectedId: "t2" });

  // 聚焦某 token 时切到承载它的视图,让用户立即看到它影响的元素。
  useEffect(() => {
    if (focusedPath) setView(tokenArea(focusedPath));
  }, [focusedPath]);

  const border = rgba(theme, "colors.border.default");
  const surface1 = rgba(theme, "colors.surface.surface1");
  const secondary = rgba(theme, "colors.text.secondary");
  const muted = rgba(theme, "colors.text.muted");
  const active = PREVIEW_TASKS.filter((task) => task.status === "downloading").length;
  const paused = PREVIEW_TASKS.filter((task) => task.status === "paused").length;
  const WindowIcon = theme.appearance === "dark" ? Sun : Moon;

  return (
    <div
      className="@container flex h-full min-h-0 flex-col overflow-hidden border"
      style={{
        borderColor: border,
        backgroundColor: surface1,
        borderRadius: num(theme, "metrics.radius.dialog"),
        boxShadow: `0 24px 64px ${alphaOf(theme, "colors.shadow", "metrics.alpha.shadowStrong")}`,
        fontFamily: "var(--font-sans)",
      }}
      {...tokenAttrs("colors.surface.surface1", "colors.border.default", "colors.shadow", "metrics.alpha.shadowStrong", "metrics.radius.dialog")}
    >
      {/* 标题栏 */}
      <div className="flex h-10 shrink-0 items-center gap-3 border-b pl-3" style={{ borderColor: border }} {...tokenAttrs("colors.surface.surface1", "colors.border.default")}>
        <div className="flex shrink-0 items-center gap-2">
          <img src="/logo.svg" alt="" className="h-4 w-4" />
          <span className="text-[12px] font-semibold">
            <span style={{ color: rgba(theme, "colors.accent.color") }} {...tokenAttrs("colors.accent.color")}>
              Flux
            </span>
            <span style={{ color: rgba(theme, "colors.text.primary") }} {...tokenAttrs("colors.text.primary")}>
              Down
            </span>
          </span>
        </div>
        <div className="flex min-w-0 flex-1 justify-center">
          <div
            className="flex h-7 w-full max-w-[280px] items-center gap-2 border px-2 text-[11px]"
            style={{ borderColor: rgba(theme, "colors.input.border"), backgroundColor: rgba(theme, "colors.input.background"), color: muted, borderRadius: num(theme, "metrics.radius.input") }}
            {...tokenAttrs("colors.input.background", "colors.input.border", "colors.text.muted", "metrics.radius.input")}
          >
            <Search className="h-3.5 w-3.5 shrink-0" aria-hidden />
            <span className="min-w-0 flex-1 truncate">{t.searchPlaceholder}</span>
            <span className="hidden font-mono text-[10px] @xl:inline" style={{ color: secondary }}>
              Ctrl F
            </span>
          </div>
        </div>
        <div className="flex shrink-0 items-center" style={{ color: secondary }} {...tokenAttrs("colors.text.secondary", "colors.border.default")}>
          {[Minus, Square, X].map((Icon, i) => (
            <span key={i} className="grid h-10 w-10 place-items-center" aria-hidden>
              <Icon className={i === 1 ? "h-3 w-3" : "h-3.5 w-3.5"} />
            </span>
          ))}
        </div>
      </div>

      <div className="flex min-h-0 flex-1">
        {/* 活动栏:与侧栏同为 chrome 底 */}
        <div
          className="flex w-11 shrink-0 flex-col justify-between py-2"
          style={{ backgroundColor: surface1 }}
          {...tokenAttrs("colors.surface.surface1")}
        >
          <div className="flex flex-col gap-1">
            <RailButton theme={theme} label={t.routeDownloads} selected={view === "downloads"} onClick={() => setView("downloads")}>
              <ArrowDownToLine className="h-4 w-4" />
            </RailButton>
          </div>
          <div className="flex flex-col gap-1">
            <RailButton theme={theme} label={t.toggleAppearance} onClick={onToggleAppearance}>
              <WindowIcon className="h-4 w-4" />
            </RailButton>
            <RailButton theme={theme} label={t.routeSettings} selected={view === "settings"} onClick={() => setView(view === "settings" ? "downloads" : "settings")}>
              <Settings className="h-4 w-4" />
            </RailButton>
          </div>
        </div>

        <div className="flex min-w-0 flex-1 border-l" style={{ borderColor: border }}>
          {view === "downloads" ? (
            <DownloadsView theme={theme} t={t} state={downloads} onChange={setDownloads} />
          ) : (
            <SettingsView theme={theme} t={t} onBack={() => setView("downloads")} />
          )}
        </div>
      </div>

      {/* 状态栏 */}
      <div
        className="flex h-6 shrink-0 items-center gap-3 border-t px-3 text-[10px]"
        style={{ borderColor: border, backgroundColor: surface1, color: muted }}
        {...tokenAttrs("colors.surface.surface1", "colors.border.default", "colors.text.muted", "colors.status.success")}
      >
        <span className="inline-flex items-center gap-1.5">
          <span className="h-1.5 w-1.5 rounded-full" style={{ backgroundColor: rgba(theme, "colors.status.success") }} />
          {t.activeSpeed}
          <span className="tabular-nums" style={{ color: rgba(theme, "colors.status.success") }}>
            12.8 MB/s
          </span>
        </span>
        <span className="hidden truncate tabular-nums @lg:inline">{t.statusActive(active, paused, PREVIEW_TASKS.length)}</span>
        <span
          className="ml-auto px-1.5 font-mono leading-4"
          style={{ backgroundColor: rgba(theme, "colors.surface.surface2"), color: secondary, borderRadius: num(theme, "metrics.radius.badge") }}
          {...tokenAttrs("colors.surface.surface2", "metrics.radius.badge")}
        >
          {theme.appearance}
        </span>
      </div>
    </div>
  );
}
