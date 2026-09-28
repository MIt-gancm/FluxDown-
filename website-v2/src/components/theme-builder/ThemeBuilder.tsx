/**
 * 主题构建器岛：顶部切换客户端目标（Flutter | GPUI）。
 *
 * Flutter 目标持有 FluxThemeJson 状态，组合编辑器、客户端预览与右键 token 检视器；状态迁移
 * (改名 / 作者 / 预设切换 / 颜色 / 数值 / 导入 / 重置)与旧版逐一对应，导出统一走
 * `exportFluxThemeJson`，因此同样的操作序列导出的 JSON 字节级一致(含键顺序)。
 * GPUI 目标见 `gpui/GpuiThemeBuilder`（`?client=gpui` 直达，`&market=<path>` 从主题市场导入，只消费一次）。
 * 两个目标常驻挂载、仅切换显隐，来回切换不丢任何一侧的编辑。
 */
import { useCallback, useEffect, useRef, useState, type ChangeEvent, type MouseEvent } from "react";
import { Monitor } from "lucide-react";
import {
  defaultDarkTheme,
  defaultLightTheme,
  exportFluxThemeJson,
  getPathValue,
  normalizeHex8,
  parseFluxThemeJson,
  setPathValue,
  withAlpha,
  withRgb,
  type FluxThemeAppearance,
  type FluxThemeJson,
} from "@/lib/theme-builder";
import { themeBuilder } from "@/i18n/messages/themeBuilder";
import type { Lang } from "@/i18n/config";
import { cn } from "@/lib/utils";
import { formatInspectorValue, hex8, safeFileName } from "./tokens";
import { EditorPanel, type EditorActions } from "./EditorPanel";
import { InspectorMenu, type InspectorState } from "./InspectorMenu";
import { PreviewWindow } from "./PreviewWindow";
import { StatusToast, type Status } from "./StatusToast";
import { GpuiThemeBuilder } from "./gpui/GpuiThemeBuilder";

const cloneTheme = (theme: FluxThemeJson): FluxThemeJson => JSON.parse(JSON.stringify(theme)) as FluxThemeJson;

type ClientTarget = "flutter" | "gpui";

export default function ThemeBuilder({ lang }: { lang: Lang }) {
  const t = themeBuilder[lang];
  const [client, setClient] = useState<ClientTarget>("flutter");
  const [theme, setTheme] = useState<FluxThemeJson>(() => cloneTheme(defaultDarkTheme));
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const [menu, setMenu] = useState<InspectorState | null>(null);
  const [focusedPath, setFocusedPath] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const rowRefs = useRef<Record<string, HTMLDivElement | null>>({});

  // 静态页水合后再读查询参数，避免服务端 / 客户端首帧不一致。
  useEffect(() => {
    if (new URLSearchParams(window.location.search).get("client") === "gpui") setClient("gpui");
  }, []);

  const switchClient = useCallback((next: ClientTarget) => {
    setClient(next);
    setMenu(null);
    const url = new URL(window.location.href);
    if (next === "gpui") url.searchParams.set("client", "gpui");
    else url.searchParams.delete("client");
    window.history.replaceState(null, "", url);
  }, []);

  const applyAppearance = useCallback((appearance: FluxThemeAppearance) => {
    setTheme((prev) => {
      if (prev.appearance === appearance) return prev;
      const base = cloneTheme(appearance === "dark" ? defaultDarkTheme : defaultLightTheme);
      return { ...base, name: prev.name, author: prev.author };
    });
  }, []);

  const handleImport = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      if (!file) return;
      try {
        const imported = parseFluxThemeJson(await file.text());
        setTheme(imported);
        setStatus({ type: "success", text: t.tb.importSuccess(imported.name) });
      } catch (error) {
        setStatus({ type: "error", text: t.tb.importError(error instanceof Error ? error.message : String(error)) });
      } finally {
        event.target.value = "";
      }
    },
    [t],
  );

  const copyText = useCallback(
    async (value: string) => {
      try {
        await navigator.clipboard.writeText(value);
        setStatus({ type: "success", text: t.tb.copySuccess });
      } catch {
        setStatus({ type: "error", text: t.tb.copyError });
      }
    },
    [t],
  );

  const actions: EditorActions = {
    setName: (name) => setTheme((prev) => ({ ...prev, name })),
    setAuthor: (author) =>
      setTheme((prev) => (author.trim() ? { ...prev, author } : { ...prev, author: undefined })),
    applyAppearance,
    updateHex: (path, value) => setTheme((prev) => setPathValue(prev, path, normalizeHex8(value))),
    updateRgb: (path, value) => setTheme((prev) => setPathValue(prev, path, withRgb(hex8(prev, path), value))),
    updateAlpha: (path, alpha) => setTheme((prev) => setPathValue(prev, path, withAlpha(hex8(prev, path), alpha))),
    updateNumber: (path, value) => setTheme((prev) => setPathValue(prev, path, value)),
    importFile: () => fileInputRef.current?.click(),
    exportFile: () => {
      const url = URL.createObjectURL(new Blob([exportFluxThemeJson(theme)], { type: "application/json" }));
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${safeFileName(theme.name)}.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setStatus({ type: "success", text: t.tb.exportSuccess(theme.name) });
    },
    copyJson: () => void copyText(exportFluxThemeJson(theme)),
    reset: () => {
      setTheme(cloneTheme(defaultDarkTheme));
      setStatus({ type: "success", text: t.tb.resetSuccess });
    },
  };

  const openInspector = useCallback(
    (event: MouseEvent<HTMLElement>) => {
      const node = (event.target as HTMLElement).closest<HTMLElement>("[data-token-paths]");
      const paths = (node?.dataset.tokenPaths ?? "").split("|").map((p) => p.trim()).filter(Boolean);
      if (!node || paths.length === 0) return;
      event.preventDefault();
      // 键盘触发(ContextMenu 键 / Shift+F10)时坐标为 0,改用目标元素位置。
      const rect = node.getBoundingClientRect();
      const fromKeyboard = event.clientX === 0 && event.clientY === 0;
      const x = fromKeyboard ? rect.left + 8 : event.clientX;
      const y = fromKeyboard ? rect.top + 8 : event.clientY;
      setMenu({
        x: Math.max(8, Math.min(x, window.innerWidth - 332)),
        y: Math.max(8, Math.min(y, window.innerHeight - 340)),
        entries: [...new Set(paths)].map((path) => ({ path, value: formatInspectorValue(getPathValue(theme, path)) })),
      });
    },
    [theme],
  );

  const closeMenu = useCallback(() => setMenu(null), []);

  const revealToken = useCallback((path: string) => {
    setSearch(path);
    setMenu(null);
    let attempts = 0;
    const run = () => {
      const row = rowRefs.current[path];
      if (row) {
        row.scrollIntoView({ behavior: "smooth", block: "center" });
        const input = row.querySelector<HTMLInputElement>('input[type="text"], input[type="number"]');
        input?.focus({ preventScroll: true });
        input?.select();
        setFocusedPath(path);
        return;
      }
      if (attempts < 8) {
        attempts += 1;
        window.setTimeout(run, 50);
      }
    };
    window.setTimeout(run, 0);
  }, []);

  useEffect(() => {
    if (!focusedPath) return;
    const timer = window.setTimeout(() => setFocusedPath((cur) => (cur === focusedPath ? null : cur)), 1500);
    return () => window.clearTimeout(timer);
  }, [focusedPath]);

  useEffect(() => {
    if (!status) return;
    const timer = window.setTimeout(() => setStatus(null), 3600);
    return () => window.clearTimeout(timer);
  }, [status]);

  return (
    <>
      <div className="flex flex-col items-center gap-4 px-6 py-20 text-center lg:hidden">
        <span className="grid h-12 w-12 place-items-center rounded-md text-accent-ink shadow-[inset_0_0_0_1px_var(--line-strong)]">
          <Monitor size={22} aria-hidden />
        </span>
        <h2 className="h3">{t.tb.mobileTitle}</h2>
        <p className="max-w-sm text-sm leading-relaxed text-muted">{t.tb.mobileHint}</p>
      </div>

      <div className="hidden items-center gap-3 border-b border-line bg-bg px-3 py-2 lg:flex">
        <span className="font-mono text-[10px] uppercase tracking-[0.12em] text-subtle">{t.tb.client.label}</span>
        <div role="group" aria-label={t.tb.client.label} className="grid grid-cols-2 rounded-md p-0.5 shadow-[inset_0_0_0_1px_var(--line-strong)]">
          {(["flutter", "gpui"] as const).map((target) => (
            <button
              key={target}
              type="button"
              aria-pressed={client === target}
              onClick={() => switchClient(target)}
              className={cn(
                "inline-flex h-7 items-center justify-center rounded-[5px] px-3 text-[12px] font-medium transition-colors focus-visible:outline-2 focus-visible:outline-accent",
                client === target ? "bg-accent-soft text-accent-ink" : "text-muted hover:text-fg",
              )}
            >
              {t.tb.client[target]}
            </button>
          ))}
        </div>
        <span className="truncate text-[12px] text-subtle">{t.tb.client.hints[client]}</span>
      </div>

      {/* 两个目标常驻挂载、仅切换显隐，切换客户端不丢编辑状态；隐藏目标的检视器由各自关闭。 */}
      <div hidden={client !== "gpui"}>
        <GpuiThemeBuilder lang={lang} active={client === "gpui"} />
      </div>

      <div hidden={client !== "flutter"}>
        <div
          className="hidden h-[clamp(620px,calc(100dvh-var(--header-h)-48px),900px)] grid-cols-[372px_minmax(0,1fr)] lg:grid"
          onContextMenu={openInspector}
          data-token-paths="colors.surface.background"
        >
          <EditorPanel
            theme={theme}
            t={t.tb}
            search={search}
            onSearch={setSearch}
            focusedPath={focusedPath}
            onRowFocus={setFocusedPath}
            rowRefs={rowRefs}
            actions={actions}
          />

          <div className="relative flex min-h-0 min-w-0 flex-col bg-sunken">
            <div className="fig-bar shrink-0 bg-bg">
              <span>fig. {theme.appearance}</span>
              <span>{t.tb.rightClickHint}</span>
            </div>
            <div className="min-h-0 flex-1 bg-[radial-gradient(var(--line-strong)_1px,transparent_1px)] bg-[length:18px_18px] p-5 xl:p-7">
              <PreviewWindow
                theme={theme}
                t={t.mock}
                focusedPath={focusedPath}
                onToggleAppearance={() => applyAppearance(theme.appearance === "dark" ? "light" : "dark")}
              />
            </div>

            <StatusToast status={status} onDismiss={() => setStatus(null)} dismissLabel={t.tb.dismiss} />
          </div>
        </div>
      </div>

      <input ref={fileInputRef} type="file" accept=".json,application/json" className="hidden" onChange={handleImport} />

      {menu && client === "flutter" && (
        <InspectorMenu state={menu} onClose={closeMenu} onCopy={copyText} onReveal={revealToken} t={t.tb} />
      )}
    </>
  );
}
