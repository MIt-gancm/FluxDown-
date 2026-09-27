/** 预览:任务详情侧栏(进度、分段条、分段分布网格、属性列表、操作按钮)。 */
import { useMemo } from "react";
import { X } from "lucide-react";
import type { FluxThemeJson } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { formatBytes, num, rgba, segmentPath, tokenAttrs } from "./tokens";
import { buildGridCells, type PreviewTask } from "./preview-data";

type MockMessages = ThemeBuilderMessages["mock"];

export function statusText(task: PreviewTask, t: MockMessages): string {
  if (task.status === "downloading") return t.statusDownloading;
  if (task.status === "paused") return t.statusPaused;
  if (task.status === "completed") return t.statusCompleted;
  return t.statusError;
}

export function statusColor(theme: FluxThemeJson, task: PreviewTask): string {
  if (task.status === "downloading") return rgba(theme, "colors.accent.color");
  if (task.status === "completed") return rgba(theme, "colors.status.success");
  if (task.status === "paused") return rgba(theme, "colors.status.warning");
  return rgba(theme, "colors.status.error");
}

export function TaskDetail({ theme, task, t }: { theme: FluxThemeJson; task: PreviewTask; t: MockMessages }) {
  const cells = useMemo(() => buildGridCells(theme, task.segments, task.totalBytes), [theme, task]);
  const border = rgba(theme, "colors.border.default");
  const muted = rgba(theme, "colors.text.muted");

  const rows = [
    { label: t.labelSize, value: task.size, path: "colors.text.secondary" },
    { label: t.labelDownloaded, value: formatBytes(task.downloadedBytes), path: "colors.text.secondary" },
    { label: t.labelSpeed, value: task.speed, path: "colors.text.secondary" },
    { label: t.labelRemaining, value: task.eta, path: "colors.text.secondary" },
    { label: t.labelStatus, value: statusText(task, t), path: "colors.accent.color" },
    { label: t.labelThreads, value: t.threadsValue(task.segments.length), path: "colors.text.secondary" },
    { label: t.labelPath, value: task.saveDir, path: "colors.text.secondary" },
    { label: t.labelUrl, value: task.url, path: "colors.text.secondary" },
  ];

  return (
    <aside
      className="hidden w-[244px] shrink-0 flex-col border-l @4xl:flex"
      style={{ borderColor: border, backgroundColor: rgba(theme, "colors.surface.surface1") }}
      {...tokenAttrs("colors.surface.surface1", "colors.border.default")}
    >
      <div className="flex h-[38px] shrink-0 items-center justify-between border-b px-3" style={{ borderColor: border }}>
        <span className="text-[12px] font-semibold" style={{ color: rgba(theme, "colors.text.primary") }}>
          {t.detail}
        </span>
        <X className="h-3.5 w-3.5" style={{ color: muted }} aria-hidden />
      </div>

      <div className="min-h-0 flex-1 space-y-3 overflow-y-auto p-3 [scrollbar-width:thin]" style={{ scrollbarColor: `${rgba(theme, "colors.surface.surface3")} transparent` }}>
        <div className="flex items-center gap-2.5">
          <div
            className="flex h-9 w-9 shrink-0 items-center justify-center text-[10px] font-semibold uppercase"
            style={{ backgroundColor: rgba(theme, "colors.surface.surface2"), color: rgba(theme, "colors.text.secondary"), borderRadius: num(theme, "metrics.radius.iconTile") }}
          >
            {task.ext}
          </div>
          <div className="min-w-0 flex-1">
            <div className="truncate text-xs font-medium" style={{ color: rgba(theme, "colors.text.primary") }}>
              {task.name}
            </div>
            <div className="truncate text-[10px]" style={{ color: muted }}>
              HTTP · {task.size}
            </div>
          </div>
        </div>

        <div className="text-2xl font-semibold tabular-nums" style={{ color: rgba(theme, "colors.text.primary") }}>
          {task.progress.toFixed(1)}%
        </div>

        <div className="flex h-1.5 overflow-hidden" style={{ backgroundColor: rgba(theme, "colors.surface.surface3"), borderRadius: num(theme, "metrics.radius.progress") }}>
          {task.segments.map((segment) => {
            const size = segment.endByte - segment.startByte + 1;
            return (
              <div key={segment.index} className="h-full" style={{ width: `${(size / task.totalBytes) * 100}%` }}>
                <div
                  className="h-full"
                  style={{ width: `${Math.min(1, segment.downloadedBytes / size) * 100}%`, backgroundColor: rgba(theme, segmentPath(theme, segment.index)) }}
                  {...tokenAttrs(segmentPath(theme, segment.index))}
                />
              </div>
            );
          })}
        </div>

        <div className="space-y-1.5">
          <div className="text-[11px] font-medium" style={{ color: muted }}>
            {t.distLabel}
          </div>
          <div
            className="border p-1.5"
            style={{ borderColor: border, backgroundColor: rgba(theme, "colors.surface.surface2"), borderRadius: num(theme, "metrics.radius.card") }}
            {...tokenAttrs("colors.border.default", "colors.surface.surface2", "metrics.radius.card")}
          >
            <div className="grid gap-[3px]" style={{ gridTemplateColumns: "repeat(44, minmax(0, 1fr))" }}>
              {cells.map((filled, index) => {
                const colorPath = filled ? segmentPath(theme, index % task.segments.length) : "colors.surface.surface3";
                return (
                  <div
                    key={index}
                    className="aspect-square"
                    style={{ backgroundColor: rgba(theme, colorPath), borderRadius: num(theme, "metrics.radius.segmentCell") }}
                    {...tokenAttrs(colorPath, "metrics.radius.segmentCell")}
                  />
                );
              })}
            </div>
          </div>
        </div>

        <dl className="space-y-1">
          {rows.map((row) => (
            <div key={row.label} className="grid grid-cols-[64px_minmax(0,1fr)] gap-1.5">
              <dt className="text-[11px]" style={{ color: muted }}>
                {row.label}
              </dt>
              <dd className="truncate text-[11px]" style={{ color: rgba(theme, row.path) }}>
                {row.value}
              </dd>
            </div>
          ))}
        </dl>
      </div>

      <div className="flex shrink-0 flex-col gap-2 border-t px-3 py-2" style={{ borderColor: border }}>
        <div
          className="flex items-center justify-center px-2.5 py-1.5 text-[11px] font-semibold"
          style={{ backgroundColor: rgba(theme, "colors.accent.color"), color: rgba(theme, "colors.accent.foreground"), borderRadius: num(theme, "metrics.radius.card") }}
          {...tokenAttrs("colors.accent.color", "colors.accent.foreground", "metrics.radius.card")}
        >
          {task.status === "downloading" ? t.btnPause : t.btnResume}
        </div>
        <div
          className="flex items-center justify-center border px-2.5 py-1.5 text-[11px] font-semibold"
          style={{
            borderColor: rgba(theme, "colors.status.error"),
            color: rgba(theme, "colors.status.error"),
            backgroundColor: rgba(theme, "colors.element.active"),
            borderRadius: num(theme, "metrics.radius.card"),
          }}
          {...tokenAttrs("colors.status.error", "colors.element.active", "metrics.radius.card")}
        >
          {t.btnDelete}
        </div>
      </div>
    </aside>
  );
}
