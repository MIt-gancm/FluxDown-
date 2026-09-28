/**
 * 预览:下载主视图,对应 GPUI 客户端下载页——「状态」文件夹侧栏(内嵌分类子项)、
 * 工具栏、任务表格与任务详情侧栏。
 */
import type { ComponentType, CSSProperties } from "react";
import {
  AlertCircle,
  Archive,
  ArrowDownToLine,
  CheckCircle2,
  File,
  FileText,
  Film,
  Image,
  LayoutList,
  Music,
  PauseCircle,
  Pause,
  Plus,
  Shapes,
} from "lucide-react";
import type { FluxThemeJson } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { num, rgba, tokenAttrs } from "./tokens";
import {
  FILE_CATEGORIES,
  PREVIEW_TASKS,
  STATUS_FILTERS,
  type FileCategory,
  type PreviewTask,
  type StatusFilter,
} from "./preview-data";
import { TaskDetail, statusColor, statusText } from "./TaskDetail";

type MockMessages = ThemeBuilderMessages["mock"];
type IconType = ComponentType<{ className?: string; style?: CSSProperties }>;

const STATUS_ICON: Record<StatusFilter, IconType> = {
  all: LayoutList,
  downloading: ArrowDownToLine,
  completed: CheckCircle2,
  error: AlertCircle,
  paused: PauseCircle,
};

const CATEGORY_ICON: Record<FileCategory, IconType> = {
  all: Shapes,
  video: Film,
  audio: Music,
  document: FileText,
  image: Image,
  archive: Archive,
  other: File,
};

function statusLabel(filter: StatusFilter, t: MockMessages): string {
  if (filter === "all") return t.tabAll;
  if (filter === "downloading") return t.tabDownloading;
  if (filter === "completed") return t.tabCompleted;
  if (filter === "paused") return t.tabPaused;
  return t.tabError;
}

function categoryLabel(category: FileCategory, t: MockMessages): string {
  return category === "all" ? t.allFiles : t[category];
}

function matches(task: PreviewTask, status: StatusFilter, category: FileCategory): boolean {
  return (status === "all" || task.status === status) && (category === "all" || task.fileCategory === category);
}

export interface DownloadsState {
  status: StatusFilter;
  category: FileCategory;
  selectedId: string;
}

export function DownloadsView({
  theme,
  t,
  state,
  onChange,
}: {
  theme: FluxThemeJson;
  t: MockMessages;
  state: DownloadsState;
  onChange: (next: DownloadsState) => void;
}) {
  const border = rgba(theme, "colors.border.default");
  const accent = rgba(theme, "colors.accent.color");
  const secondary = rgba(theme, "colors.text.secondary");
  const muted = rgba(theme, "colors.text.muted");
  const tasks = PREVIEW_TASKS.filter((task) => matches(task, state.status, state.category));
  const selected = PREVIEW_TASKS.find((task) => task.id === state.selectedId) ?? tasks[0] ?? PREVIEW_TASKS[0]!;
  const count = (status: StatusFilter, category: FileCategory) =>
    PREVIEW_TASKS.filter((task) => matches(task, status, category)).length;

  return (
    <div className="flex min-h-0 flex-1">
      {/* 侧栏:状态文件夹,当前状态下内嵌分类子项 */}
      <aside
        className="hidden w-[184px] shrink-0 flex-col overflow-y-auto border-r py-2 [scrollbar-width:none] @2xl:flex"
        style={{ borderColor: border, backgroundColor: rgba(theme, "colors.surface.surface1") }}
        {...tokenAttrs("colors.surface.surface1", "colors.border.default")}
      >
        <div className="px-4 pb-1 pt-1 text-[10px] font-medium uppercase tracking-wide" style={{ color: muted }} {...tokenAttrs("colors.text.muted")}>
          {t.sectionStatus}
        </div>
        <div className="space-y-0.5 px-2">
          {STATUS_FILTERS.map((status) => {
            const active = state.status === status;
            const Icon = STATUS_ICON[status];
            return (
              <div key={status}>
                <button
                  type="button"
                  onClick={() => onChange({ ...state, status, category: "all" })}
                  className="flex h-7 w-full items-center justify-between px-2 text-left text-[11.5px] hover:bg-[var(--tb-hover)]"
                  style={{
                    color: active && state.category === "all" ? accent : active ? rgba(theme, "colors.text.primary") : secondary,
                    backgroundColor: active && state.category === "all" ? rgba(theme, "colors.element.selected") : undefined,
                    borderRadius: num(theme, "metrics.radius.md"),
                    "--tb-hover": rgba(theme, "colors.element.hover"),
                  } as CSSProperties}
                  {...tokenAttrs("colors.element.selected", "colors.element.hover", "colors.accent.color", "colors.text.primary", "colors.text.secondary", "metrics.radius.md")}
                >
                  <span className="inline-flex min-w-0 items-center gap-2">
                    <Icon className="h-3.5 w-3.5 shrink-0" />
                    <span className="truncate">{statusLabel(status, t)}</span>
                  </span>
                  <span className="tabular-nums" style={{ color: active ? accent : muted }}>
                    {count(status, "all")}
                  </span>
                </button>
                {active && (
                  <div className="ml-[15px] mt-0.5 space-y-0.5 border-l pl-1.5" style={{ borderColor: border }}>
                    {FILE_CATEGORIES.filter((c) => c !== "all").map((category) => {
                      const on = state.category === category;
                      const CatIcon = CATEGORY_ICON[category];
                      return (
                        <button
                          key={category}
                          type="button"
                          onClick={() => onChange({ ...state, category: on ? "all" : category })}
                          className="flex h-6 w-full items-center justify-between px-2 text-left text-[11px] hover:bg-[var(--tb-hover)]"
                          style={{
                            color: on ? accent : secondary,
                            backgroundColor: on ? rgba(theme, "colors.element.selected") : undefined,
                            borderRadius: num(theme, "metrics.radius.sm"),
                            "--tb-hover": rgba(theme, "colors.element.hover"),
                          } as CSSProperties}
                          {...tokenAttrs("colors.element.selected", "colors.element.hover", "colors.accent.color", "colors.text.secondary", "metrics.radius.sm")}
                        >
                          <span className="inline-flex min-w-0 items-center gap-2">
                            <CatIcon className="h-3 w-3 shrink-0" />
                            <span className="truncate">{categoryLabel(category, t)}</span>
                          </span>
                          <span className="tabular-nums" style={{ color: on ? accent : muted }}>
                            {count(status, category)}
                          </span>
                        </button>
                      );
                    })}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col" style={{ backgroundColor: rgba(theme, "colors.surface.background") }} {...tokenAttrs("colors.surface.background")}>
        {/* 工具栏 */}
        <div
          className="flex h-[38px] shrink-0 items-center justify-between gap-2 border-b px-3"
          style={{ borderColor: border }}
          {...tokenAttrs("colors.border.default", "colors.text.primary", "colors.text.muted")}
        >
          <div className="flex min-w-0 items-baseline gap-2">
            <span className="truncate text-[12.5px] font-semibold" style={{ color: rgba(theme, "colors.text.primary") }}>
              {state.category === "all" ? statusLabel(state.status, t) : categoryLabel(state.category, t)}
            </span>
            <span className="text-[10.5px] tabular-nums" style={{ color: muted }}>
              {tasks.length}
            </span>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            <div
              className="inline-flex h-7 items-center gap-1 border px-2 text-[11px] font-medium"
              style={{ borderColor: border, color: secondary, backgroundColor: rgba(theme, "colors.surface.surface1"), borderRadius: num(theme, "metrics.radius.md") }}
              {...tokenAttrs("colors.border.default", "colors.text.secondary", "colors.surface.surface1", "metrics.radius.md")}
            >
              <Pause className="h-3.5 w-3.5" />
              {t.btnPause}
            </div>
            <div
              className="inline-flex h-7 items-center gap-1 px-2 text-[11px] font-semibold"
              style={{ backgroundColor: accent, color: rgba(theme, "colors.accent.foreground"), borderRadius: num(theme, "metrics.radius.md") }}
              {...tokenAttrs("colors.accent.color", "colors.accent.foreground", "metrics.radius.md")}
            >
              <Plus className="h-3.5 w-3.5" />
              {t.newDownload}
            </div>
          </div>
        </div>

        {/* 表头 */}
        <div
          className="flex h-[28px] shrink-0 items-center border-b px-3 text-[10px] font-medium uppercase tracking-wide"
          style={{ borderColor: border, backgroundColor: rgba(theme, "colors.surface.surface1"), color: muted }}
          {...tokenAttrs("colors.surface.surface1", "colors.border.default", "colors.text.muted")}
        >
          <div className="min-w-0 flex-1">{t.colFilename}</div>
          <div className="w-[128px]">{t.colProgress}</div>
          <div className="hidden w-[72px] text-right @3xl:block">{t.colSpeed}</div>
          <div className="hidden w-[64px] text-right @xl:block">{t.colStatus}</div>
        </div>

        {/* 任务行 */}
        <div className="min-h-0 flex-1 overflow-y-auto [scrollbar-width:thin]" style={{ scrollbarColor: `${rgba(theme, "colors.surface.surface3")} transparent` }}>
          {tasks.length === 0 && (
            <div className="py-12 text-center text-[11px]" style={{ color: muted }}>
              {t.noTasks}
            </div>
          )}
          {tasks.map((task) => {
            const isSelected = selected.id === task.id;
            const color = statusColor(theme, task);
            return (
              <button
                key={task.id}
                type="button"
                onClick={() => onChange({ ...state, selectedId: task.id })}
                className="flex h-[46px] w-full items-center border-b px-3 text-left hover:bg-[var(--tb-hover)]"
                style={{
                  borderColor: border,
                  backgroundColor: isSelected ? rgba(theme, "colors.element.selected") : undefined,
                  "--tb-hover": rgba(theme, "colors.element.hover"),
                } as CSSProperties}
                {...tokenAttrs("colors.border.default", "colors.element.selected", "colors.element.hover")}
              >
                <div className="flex min-w-0 flex-1 items-center gap-2.5">
                  <div
                    className="flex h-7 w-7 shrink-0 items-center justify-center text-[9px] font-semibold uppercase"
                    style={{ backgroundColor: rgba(theme, "colors.surface.surface2"), color: isSelected ? accent : secondary, borderRadius: num(theme, "metrics.radius.iconTile") }}
                    {...tokenAttrs("colors.surface.surface2", "colors.accent.color", "colors.text.secondary", "metrics.radius.iconTile")}
                  >
                    {task.ext}
                  </div>
                  <div className="min-w-0 pr-2">
                    <div className="truncate text-[11.5px]" style={{ color: isSelected ? accent : rgba(theme, "colors.text.primary") }} {...tokenAttrs("colors.text.primary", "colors.accent.color")}>
                      {task.name}
                    </div>
                    <div className="truncate text-[10px]" style={{ color: muted }} {...tokenAttrs("colors.text.muted")}>
                      HTTP · {task.size}
                      {task.status === "downloading" ? ` · ${task.speed}` : task.status === "error" ? ` · ${t.subtitleTimeout}` : ""}
                    </div>
                  </div>
                </div>
                <div className="flex w-[128px] items-center gap-2 pr-2" {...tokenAttrs("colors.surface.surface3", "metrics.radius.progress", "colors.text.secondary")}>
                  <div className="h-[3px] flex-1 overflow-hidden" style={{ backgroundColor: rgba(theme, "colors.surface.surface3"), borderRadius: num(theme, "metrics.radius.progress") }}>
                    <div className="h-full" style={{ width: `${task.progress}%`, backgroundColor: color, borderRadius: num(theme, "metrics.radius.progress") }} />
                  </div>
                  <span className="w-9 text-right text-[10px] tabular-nums" style={{ color: secondary }}>
                    {task.progress.toFixed(1)}%
                  </span>
                </div>
                <div
                  className="hidden w-[72px] text-right text-[10px] tabular-nums @3xl:block"
                  style={{ color: task.status === "downloading" ? rgba(theme, "colors.status.success") : muted }}
                  {...tokenAttrs("colors.status.success", "colors.text.muted")}
                >
                  {task.speed}
                </div>
                <div
                  className="hidden w-[64px] text-right text-[10px] @xl:block"
                  style={{ color }}
                  {...tokenAttrs("colors.status.success", "colors.status.warning", "colors.status.error", "colors.accent.color")}
                >
                  {statusText(task, t)}
                </div>
              </button>
            );
          })}
        </div>
      </div>

      <TaskDetail theme={theme} task={selected} t={t} />
    </div>
  );
}
