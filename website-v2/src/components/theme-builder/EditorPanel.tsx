/** 左侧编辑器:主题元信息、基础预设、导入/导出/复制/重置、token 搜索与分组列表。 */
import { useMemo, type RefObject } from "react";
import { Copy, Download, Moon, RotateCcw, Search, Sun, Upload, X } from "lucide-react";
import {
  TOKEN_GROUPS,
  getTokenDescriptors,
  type FluxThemeAppearance,
  type FluxThemeJson,
} from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";
import { cn } from "@/lib/utils";
import { hex8, num, tokenAttrs } from "./tokens";
import { ColorTokenRow, NumberTokenRow } from "./TokenRows";

type TbMessages = ThemeBuilderMessages["tb"];

export interface EditorActions {
  setName: (name: string) => void;
  setAuthor: (author: string) => void;
  applyAppearance: (appearance: FluxThemeAppearance) => void;
  updateHex: (path: string, value: string) => void;
  updateRgb: (path: string, value: string) => void;
  updateAlpha: (path: string, alpha: number) => void;
  updateNumber: (path: string, value: number) => void;
  importFile: () => void;
  exportFile: () => void;
  copyJson: () => void;
  reset: () => void;
}

const LABEL = "mb-1 block font-mono text-[10px] uppercase tracking-[0.12em] text-subtle";
const FIELD = "field h-8 text-[13px]";
const TOOL =
  "inline-flex h-8 items-center justify-center gap-1.5 rounded-md px-2.5 text-[12px] font-medium text-muted shadow-[inset_0_0_0_1px_var(--line-strong)] transition-colors hover:bg-inset hover:text-fg focus-visible:outline-2 focus-visible:outline-accent";

export function EditorPanel({
  theme,
  t,
  search,
  onSearch,
  focusedPath,
  onRowFocus,
  rowRefs,
  actions,
}: {
  theme: FluxThemeJson;
  t: TbMessages;
  search: string;
  onSearch: (value: string) => void;
  focusedPath: string | null;
  onRowFocus: (path: string) => void;
  rowRefs: RefObject<Record<string, HTMLDivElement | null>>;
  actions: EditorActions;
}) {
  const descriptors = useMemo(() => getTokenDescriptors(theme), [theme]);
  const groups = useMemo(() => {
    const query = search.trim().toLowerCase();
    return TOKEN_GROUPS.map((group) => ({
      group,
      tokens: descriptors.filter(
        (token) =>
          token.groupKey === group.key &&
          (!query ||
            token.path.toLowerCase().includes(query) ||
            token.label.toLowerCase().includes(query) ||
            group.key.toLowerCase().includes(query)),
      ),
    })).filter((entry) => entry.tokens.length > 0);
  }, [descriptors, search]);

  return (
    <div className="flex min-h-0 flex-col border-r border-line bg-bg" {...tokenAttrs("colors.surface.surface1", "colors.border.default")}>
      <div className="space-y-3 border-b border-line p-3" {...tokenAttrs("name", "author", "appearance")}>
        <div className="grid grid-cols-2 gap-2">
          <label className="block">
            <span className={LABEL}>{t.meta.name}</span>
            <input value={theme.name} onChange={(e) => actions.setName(e.target.value)} className={FIELD} />
          </label>
          <label className="block">
            <span className={LABEL}>{t.meta.author}</span>
            <input value={theme.author ?? ""} onChange={(e) => actions.setAuthor(e.target.value)} className={FIELD} />
          </label>
        </div>

        <div className="flex items-center gap-2">
          <div
            role="group"
            aria-label={t.appearance.label}
            className="grid flex-1 grid-cols-2 rounded-md p-0.5 shadow-[inset_0_0_0_1px_var(--line-strong)]"
            {...tokenAttrs("appearance")}
          >
            {(["dark", "light"] as const).map((appearance) => {
              const Icon = appearance === "dark" ? Moon : Sun;
              const active = theme.appearance === appearance;
              return (
                <button
                  key={appearance}
                  type="button"
                  aria-pressed={active}
                  onClick={() => actions.applyAppearance(appearance)}
                  className={cn(
                    "inline-flex h-7 items-center justify-center gap-1.5 rounded-[5px] text-[12px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent",
                    active ? "bg-accent-soft text-accent-ink" : "text-muted hover:text-fg",
                  )}
                >
                  <Icon size={13} aria-hidden />
                  {t.appearance[appearance]}
                </button>
              );
            })}
          </div>
          <button type="button" onClick={actions.importFile} className={TOOL}>
            <Upload size={13} aria-hidden />
            {t.actions.import}
          </button>
          <button type="button" onClick={actions.exportFile} className={cn(TOOL, "bg-accent text-accent-fg shadow-none hover:bg-accent hover:text-accent-fg hover:brightness-110")}>
            <Download size={13} aria-hidden />
            {t.actions.export}
          </button>
        </div>

        <div className="flex items-center gap-1.5">
          <div className="relative flex-1">
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
          <button type="button" onClick={actions.copyJson} className={cn(TOOL, "w-8 px-0")} title={t.actions.copyJson} aria-label={t.actions.copyJson}>
            <Copy size={14} />
          </button>
          <button type="button" onClick={actions.reset} className={cn(TOOL, "w-8 px-0")} title={t.actions.reset} aria-label={t.actions.reset}>
            <RotateCcw size={14} />
          </button>
        </div>
      </div>

      <div
        className="min-h-0 [scrollbar-width:thin] flex-1 overflow-y-auto overscroll-contain"
        onFocusCapture={(event) => {
          const path = (event.target as HTMLElement).closest<HTMLElement>("[data-token-row-path]")?.dataset.tokenRowPath;
          if (path) onRowFocus(path);
        }}
      >
        {groups.length === 0 && <p className="px-3 py-10 text-center text-xs text-subtle">{t.noTokens}</p>}
        {groups.map(({ group, tokens }) => (
          <details key={group.key} open className="group/g">
            <summary className="sticky top-0 z-10 flex cursor-pointer list-none items-center justify-between border-b border-line bg-sunken px-3 py-2 font-mono text-[10.5px] uppercase tracking-[0.12em] text-muted hover:text-fg [&::-webkit-details-marker]:hidden">
              <span className="flex items-center gap-2">
                <span aria-hidden className="inline-block text-subtle transition-transform group-open/g:rotate-90">›</span>
                {t.groups[group.key]}
              </span>
              <span className="num text-subtle">{String(tokens.length).padStart(2, "0")}</span>
            </summary>
            {tokens.map((token) =>
              token.kind === "number" ? (
                <NumberTokenRow
                  key={token.path}
                  token={token}
                  value={num(theme, token.path)}
                  onChange={actions.updateNumber}
                  focused={focusedPath === token.path}
                  rowRef={(node) => {
                    rowRefs.current[token.path] = node;
                  }}
                  t={t}
                />
              ) : (
                <ColorTokenRow
                  key={token.path}
                  token={token}
                  value={hex8(theme, token.path)}
                  onHexChange={actions.updateHex}
                  onRgbChange={actions.updateRgb}
                  onAlphaChange={actions.updateAlpha}
                  focused={focusedPath === token.path}
                  rowRef={(node) => {
                    rowRefs.current[token.path] = node;
                  }}
                  t={t}
                />
              ),
            )}
          </details>
        ))}
      </div>
    </div>
  );
}
