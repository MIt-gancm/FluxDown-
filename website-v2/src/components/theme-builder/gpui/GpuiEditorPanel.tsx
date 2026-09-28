/**
 * GPUI 编辑器左栏：元信息、extends 基底、编辑层（shared / dark / light）、形状预设、
 * 导入 / 导出（diff / full）/ 复制 / 重置、诊断、token 搜索与按 registry groups 数据驱动的分组列表。
 */
import { useMemo, useState, type RefObject } from "react";
import { AlertTriangle, ChevronRight, Copy, Download, RotateCcw, Search, Upload, X } from "lucide-react";
import { EXTENDS_VALUES, GROUPS, TOKENS } from "@/lib/gpui-theme/registry";
import { tokenValue } from "@/lib/gpui-theme/document";
import type { ResolvedTokens } from "@/lib/gpui-theme/resolve";
import type { Diagnostic, Json, ThemeDocument, TokenLayer } from "@/lib/gpui-theme/types";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { cn } from "@/lib/utils";
import { SHAPE_PRESETS, type ShapePreset } from "./model";
import { GpuiTokenRow } from "./GpuiTokenRow";

type GpuiMessages = ThemeBuilderMessages["gpui"];

export interface GpuiEditorActions {
  setMeta: (key: "name" | "author", value: string) => void;
  setExtends: (value: string) => void;
  setLayer: (layer: TokenLayer) => void;
  applyShape: (preset: ShapePreset) => void;
  setToken: (path: string, value: Json) => void;
  removeToken: (path: string) => void;
  importFile: () => void;
  exportFile: (mode: "diff" | "full") => void;
  copyJson: () => void;
  reset: () => void;
}

const LABEL = "mb-1 block font-mono text-[10px] uppercase tracking-[0.12em] text-subtle";
const FIELD = "field h-8 text-[13px]";
const TOOL =
  "inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-2.5 text-[12px] font-medium text-muted shadow-[inset_0_0_0_1px_var(--line-strong)] transition-colors hover:bg-inset hover:text-fg focus-visible:outline-2 focus-visible:outline-accent";
const SEGMENT =
  "inline-flex h-7 items-center justify-center rounded-[5px] text-[12px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent";

export function GpuiEditorPanel({
  document,
  layer,
  resolved,
  diagnostics,
  search,
  onSearch,
  focusedPath,
  onRowFocus,
  rowRefs,
  actions,
  t,
}: {
  document: ThemeDocument;
  layer: TokenLayer;
  /** 当前编辑层对应模式（shared 层取预览模式）的解析结果。 */
  resolved: ResolvedTokens;
  diagnostics: Diagnostic[];
  search: string;
  onSearch: (value: string) => void;
  focusedPath: string | null;
  onRowFocus: (path: string) => void;
  rowRefs: RefObject<Record<string, HTMLDivElement | null>>;
  actions: GpuiEditorActions;
  t: GpuiMessages;
}) {
  const [diagnosticsOpen, setDiagnosticsOpen] = useState(false);
  const groups = useMemo(() => {
    const query = search.trim().toLowerCase();
    return GROUPS.map((group) => ({
      group,
      specs: TOKENS.filter(
        (spec) => spec.group === group && (!query || spec.path.toLowerCase().includes(query) || group.toLowerCase().includes(query)),
      ),
    })).filter((entry) => entry.specs.length > 0);
  }, [search]);

  return (
    <div className="flex min-h-0 flex-col border-r border-line bg-bg">
      <div className="space-y-3 border-b border-line p-3">
        <div className="grid grid-cols-2 gap-2">
          <label className="block">
            <span className={LABEL}>{t.meta.name}</span>
            <input value={document.meta?.name ?? ""} onChange={(e) => actions.setMeta("name", e.target.value)} className={FIELD} />
          </label>
          <label className="block">
            <span className={LABEL}>{t.meta.author}</span>
            <input value={document.meta?.author ?? ""} onChange={(e) => actions.setMeta("author", e.target.value)} className={FIELD} />
          </label>
        </div>

        <div className="grid grid-cols-[minmax(0,1fr)_auto] items-end gap-2">
          <label className="block min-w-0">
            <span className={LABEL}>{t.extends.label}</span>
            <select value={document.extends ?? "builtin:default"} onChange={(e) => actions.setExtends(e.target.value)} className={FIELD}>
              {EXTENDS_VALUES.map((value) => (
                <option key={value} value={value}>
                  {t.extends.options[value as keyof typeof t.extends.options] ?? value}
                </option>
              ))}
              {document.extends !== undefined && !EXTENDS_VALUES.includes(document.extends) && (
                <option value={document.extends}>{document.extends}</option>
              )}
            </select>
          </label>
          <div className="flex items-center gap-1.5">
            <button type="button" onClick={actions.importFile} className={TOOL}>
              <Upload size={13} aria-hidden />
              {t.actions.import}
            </button>
          </div>
        </div>

        <div>
          <span className={LABEL}>{t.layer.label}</span>
          <div role="group" aria-label={t.layer.label} className="grid grid-cols-3 rounded-md p-0.5 shadow-[inset_0_0_0_1px_var(--line-strong)]">
            {(["tokens", "dark", "light"] as const).map((key) => (
              <button
                key={key}
                type="button"
                aria-pressed={layer === key}
                onClick={() => actions.setLayer(key)}
                title={t.layer.hints[key]}
                className={cn(SEGMENT, layer === key ? "bg-accent-soft text-accent-ink" : "text-muted hover:text-fg")}
              >
                {t.layer[key]}
              </button>
            ))}
          </div>
        </div>

        <div>
          <span className={LABEL}>{t.shape.label}</span>
          <div role="group" aria-label={t.shape.label} className="grid grid-cols-4 gap-1">
            {SHAPE_PRESETS.map((preset) => (
              <button
                key={preset}
                type="button"
                onClick={() => actions.applyShape(preset)}
                title={t.shape.hint}
                className={cn(TOOL, "h-7 gap-1 px-1.5 text-[11.5px]")}
              >
                <span
                  aria-hidden
                  className="h-3 w-4 border border-current"
                  style={{ borderRadius: { square: 0, soft: 3, rounded: 5, pill: 999 }[preset] }}
                />
                {t.shape[preset]}
              </button>
            ))}
          </div>
        </div>

        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={() => actions.exportFile("diff")}
            title={t.actions.exportDiffHint}
            className={cn(TOOL, "flex-1 bg-accent text-accent-fg shadow-none hover:bg-accent hover:text-accent-fg hover:brightness-110")}
          >
            <Download size={13} aria-hidden />
            {t.actions.exportDiff}
          </button>
          <button type="button" onClick={() => actions.exportFile("full")} title={t.actions.exportFullHint} className={cn(TOOL, "flex-1")}>
            <Download size={13} aria-hidden />
            {t.actions.exportFull}
          </button>
          <button type="button" onClick={actions.copyJson} className={cn(TOOL, "w-8 px-0")} title={t.actions.copyJson} aria-label={t.actions.copyJson}>
            <Copy size={14} />
          </button>
          <button type="button" onClick={actions.reset} className={cn(TOOL, "w-8 px-0")} title={t.actions.reset} aria-label={t.actions.reset}>
            <RotateCcw size={14} />
          </button>
        </div>

        {diagnostics.length > 0 && (
          <div className="rounded-md shadow-[inset_0_0_0_1px_var(--line-strong)]">
            <button
              type="button"
              aria-expanded={diagnosticsOpen}
              onClick={() => setDiagnosticsOpen((open) => !open)}
              className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left text-[12px] text-warn"
            >
              <AlertTriangle size={13} aria-hidden />
              <span className="flex-1">{t.diagnostics.title(diagnostics.length)}</span>
              <ChevronRight size={13} aria-hidden className={cn("transition-transform", diagnosticsOpen && "rotate-90")} />
            </button>
            {diagnosticsOpen && (
              <ul className="max-h-40 divide-y divide-line overflow-y-auto border-t border-line [scrollbar-width:thin]">
                {diagnostics.map((diagnostic, index) => (
                  <li key={`${diagnostic.path}-${diagnostic.kind}-${index}`} className="space-y-0.5 px-2.5 py-1.5">
                    <div className="flex items-center gap-1.5">
                      <span className="rounded-[3px] bg-inset px-1 font-mono text-[9px] uppercase tracking-wider text-muted">
                        {t.diagnostics.kinds[diagnostic.kind]}
                      </span>
                      <code className="min-w-0 truncate font-mono text-[10.5px] text-fg">{diagnostic.path}</code>
                    </div>
                    <p className="text-[11px] leading-snug text-subtle">{diagnostic.message}</p>
                  </li>
                ))}
              </ul>
            )}
          </div>
        )}

        <div className="relative">
          <Search size={14} aria-hidden className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-subtle" />
          <input
            type="search"
            value={search}
            onChange={(e) => onSearch(e.target.value)}
            placeholder={t.searchPlaceholder}
            aria-label={t.searchPlaceholder}
            className={cn(FIELD, "pl-8 pr-8 [&::-webkit-search-cancel-button]:hidden")}
          />
          {search && (
            <button
              type="button"
              onClick={() => onSearch("")}
              aria-label={t.clearSearch}
              className="absolute right-1.5 top-1/2 grid h-5 w-5 -translate-y-1/2 place-items-center rounded text-subtle hover:bg-inset hover:text-fg"
            >
              <X size={12} />
            </button>
          )}
        </div>
      </div>

      <div
        className="min-h-0 flex-1 overflow-y-auto overscroll-contain [scrollbar-width:thin]"
        onFocusCapture={(event) => {
          const path = (event.target as HTMLElement).closest<HTMLElement>("[data-token-row-path]")?.dataset.tokenRowPath;
          if (path) onRowFocus(path);
        }}
      >
        {groups.length === 0 && <p className="px-3 py-10 text-center text-xs text-subtle">{t.noTokens}</p>}
        {groups.map(({ group, specs }) => (
          <details key={group} open className="group/g">
            <summary className="sticky top-0 z-10 flex cursor-pointer list-none items-center justify-between border-b border-line bg-sunken px-3 py-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted hover:text-fg [&::-webkit-details-marker]:hidden">
              <span className="flex items-center gap-2">
                <span aria-hidden className="inline-block text-subtle transition-transform group-open/g:rotate-90">
                  ›
                </span>
                {t.groups[group as keyof typeof t.groups] ?? group}
              </span>
              <span className="num text-subtle">{String(specs.length).padStart(2, "0")}</span>
            </summary>
            {specs.map((spec) => (
              <GpuiTokenRow
                key={`${layer}:${spec.path}`}
                spec={spec}
                raw={tokenValue(document, layer, spec.path)}
                resolved={resolved.get(spec.path)}
                focused={focusedPath === spec.path}
                rowRef={(node) => {
                  rowRefs.current[spec.path] = node;
                }}
                onSet={actions.setToken}
                onRemove={actions.removeToken}
                t={t}
              />
            ))}
          </details>
        ))}
      </div>
    </div>
  );
}
