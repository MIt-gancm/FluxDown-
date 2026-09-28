/**
 * GPUI 主题编辑器：持有 `ThemeDocument`，编辑层（shared / dark / light）与预览模式，
 * 每次变更后重新 resolve（与 Rust 同算法）驱动 GpuiPreview 的 CSS 变量；导入走 `parseTheme`
 * （GPUI v1 / v2 与 Flutter FluxThemeJson），导出走 `exportTheme`（diff / full）。
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ChangeEvent, type MouseEvent } from "react";
import { removeToken, setToken } from "@/lib/gpui-theme/document";
import { exportTheme } from "@/lib/gpui-theme/export";
import { parseTheme } from "@/lib/gpui-theme/parse";
import { tokenSpec } from "@/lib/gpui-theme/registry";
import { resolveTheme, sameDiagnostic } from "@/lib/gpui-theme/resolve";
import {
  THEME_FORMAT,
  THEME_SCHEMA_VERSION,
  cloneDocument,
  emptyDocument,
  type Diagnostic,
  type ThemeDocument,
  type ThemeMode,
  type TokenLayer,
} from "@/lib/gpui-theme/types";
import { colorHex } from "@/lib/gpui-theme/value";
import { themeAssetUrl } from "@/components/markets/theme-data";
import { themeBuilder } from "@/i18n/messages/themeBuilder";
import type { Lang } from "@/i18n/config";
import { safeFileName } from "../tokens";
import { InspectorMenu, type InspectorState } from "../InspectorMenu";
import { StatusToast, type Status } from "../StatusToast";
import { GpuiEditorPanel, type GpuiEditorActions } from "./GpuiEditorPanel";
import { GpuiPreview } from "./GpuiPreview";
import { SHAPE_PRESET_VALUES, displayValue } from "./model";

/** 主题市场仓库内的 GPUI 主题路径（同 `/api/themes/*` 代理白名单的 json 子集）。 */
const MARKET_PATH = /^themes\/[\w.-]+\/[\w.-]+\.json$/;

function initialDocument(name: string): ThemeDocument {
  return {
    ...emptyDocument(),
    format: THEME_FORMAT,
    schemaVersion: THEME_SCHEMA_VERSION,
    meta: { name, extra: {} },
    extends: "builtin:default",
  };
}

/** `active=false` 时由父级隐藏：保留全部编辑状态，但关闭并不再渲染右键检视器（其全局键盘 / 指针监听）。 */
export function GpuiThemeBuilder({ lang, active }: { lang: Lang; active: boolean }) {
  const t = themeBuilder[lang];
  const g = t.gpui;
  const [document, setDocument] = useState<ThemeDocument>(() => initialDocument(g.defaultName));
  const [layer, setLayer] = useState<TokenLayer>("tokens");
  const [mode, setMode] = useState<ThemeMode>("dark");
  const [importDiagnostics, setImportDiagnostics] = useState<Diagnostic[]>([]);
  const [search, setSearch] = useState("");
  const [status, setStatus] = useState<Status | null>(null);
  const [menu, setMenu] = useState<InspectorState | null>(null);
  const [focusedPath, setFocusedPath] = useState<string | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const rowRefs = useRef<Record<string, HTMLDivElement | null>>({});

  const dark = useMemo(() => resolveTheme(document, "dark"), [document]);
  const light = useMemo(() => resolveTheme(document, "light"), [document]);
  const preview = mode === "dark" ? dark : light;
  // shared 层编辑时展示预览模式下的值；dark / light 层展示该模式的值。
  const editing = layer === "light" ? light : layer === "dark" ? dark : preview;
  const diagnostics = useMemo(() => {
    const merged: Diagnostic[] = [];
    for (const diagnostic of [...importDiagnostics, ...dark.diagnostics, ...light.diagnostics]) {
      if (!merged.some((seen) => sameDiagnostic(seen, diagnostic))) merged.push(diagnostic);
    }
    return merged;
  }, [importDiagnostics, dark, light]);

  const update = useCallback((mutate: (next: ThemeDocument) => void) => {
    setDocument((prev) => {
      const next = cloneDocument(prev);
      mutate(next);
      return next;
    });
  }, []);

  const importText = useCallback(
    (text: string) => {
      const parsed = parseTheme(text);
      if (!parsed.ok) {
        setStatus({ type: "error", text: g.importError(parsed.error.message) });
        return;
      }
      setDocument(parsed.document);
      setImportDiagnostics(parsed.diagnostics);
      setStatus({ type: "success", text: g.importSuccess(parsed.document.meta?.name ?? g.defaultName, parsed.diagnostics.length) });
    },
    [g],
  );

  // 主题市场『在编辑器中打开』：?client=gpui&market=themes/<id>/gpui.json。导入成功或失败后
  // 从地址栏移除 market，只消费一次：刷新或分享当前地址不会再次覆盖用户编辑。
  useEffect(() => {
    const market = new URLSearchParams(window.location.search).get("market");
    if (!market || !MARKET_PATH.test(market)) return;
    const controller = new AbortController();
    const consume = () => {
      const url = new URL(window.location.href);
      url.searchParams.delete("market");
      window.history.replaceState(window.history.state, "", url);
    };
    setStatus({ type: "success", text: g.marketLoading });
    fetch(themeAssetUrl(market), { signal: controller.signal })
      .then((response) => (response.ok ? response.text() : Promise.reject(new Error(String(response.status)))))
      .then((text) => {
        if (controller.signal.aborted) return;
        consume();
        importText(text);
      })
      .catch((error: unknown) => {
        if (controller.signal.aborted) return;
        consume();
        setStatus({ type: "error", text: g.importError(error instanceof Error ? error.message : String(error)) });
      });
    return () => controller.abort();
  }, [g, importText]);

  useEffect(() => {
    if (!active) setMenu(null);
  }, [active]);

  const handleImport = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const file = event.target.files?.[0];
      if (!file) return;
      try {
        importText(await file.text());
      } finally {
        event.target.value = "";
      }
    },
    [importText],
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

  const actions: GpuiEditorActions = {
    setMeta: (key, value) =>
      update((next) => {
        next.meta ??= { extra: {} };
        next.meta[key] = key === "author" && !value.trim() ? undefined : value;
      }),
    setExtends: (value) =>
      update((next) => {
        next.extends = value;
      }),
    setLayer: (next) => {
      setLayer(next);
      if (next !== "tokens") setMode(next);
    },
    applyShape: (preset) => {
      update((next) => {
        for (const [path, value] of Object.entries(SHAPE_PRESET_VALUES[preset])) {
          setToken(next, "tokens", path, value);
          removeToken(next, "dark", path);
          removeToken(next, "light", path);
        }
      });
      setStatus({ type: "success", text: g.shape.applied(g.shape[preset]) });
    },
    setToken: (path, value) => update((next) => setToken(next, layer, path, value)),
    removeToken: (path) => update((next) => void removeToken(next, layer, path)),
    importFile: () => fileInputRef.current?.click(),
    exportFile: (exportMode) => {
      const name = document.meta?.name ?? g.defaultName;
      const url = URL.createObjectURL(new Blob([exportTheme(document, exportMode)], { type: "application/json" }));
      const anchor = window.document.createElement("a");
      anchor.href = url;
      anchor.download = `${safeFileName(name)}.gpui.json`;
      anchor.click();
      URL.revokeObjectURL(url);
      setStatus({ type: "success", text: g.exportSuccess(name) });
    },
    copyJson: () => void copyText(exportTheme(document, "diff")),
    reset: () => {
      setDocument(initialDocument(g.defaultName));
      setImportDiagnostics([]);
      setLayer("tokens");
      setStatus({ type: "success", text: g.resetSuccess });
    },
  };

  const openInspector = useCallback(
    (event: MouseEvent<HTMLElement>) => {
      const node = (event.target as HTMLElement).closest<HTMLElement>("[data-token-paths]");
      const paths = [...new Set((node?.dataset.tokenPaths ?? "").split("|").map((p) => p.trim()))].filter((p) => tokenSpec(p));
      if (!node || paths.length === 0) return;
      event.preventDefault();
      const rect = node.getBoundingClientRect();
      const fromKeyboard = event.clientX === 0 && event.clientY === 0;
      const x = fromKeyboard ? rect.left + 8 : event.clientX;
      const y = fromKeyboard ? rect.top + 8 : event.clientY;
      setMenu({
        x: Math.max(8, Math.min(x, window.innerWidth - 332)),
        y: Math.max(8, Math.min(y, window.innerHeight - 340)),
        entries: paths.map((path) => {
          const value = preview.tokens.get(path);
          return { path, value: displayValue(value), swatch: value?.type === "color" ? colorHex(value.color) : undefined };
        }),
      });
    },
    [preview],
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
        const input = row.querySelector<HTMLElement>("input, select, button");
        input?.focus({ preventScroll: true });
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
      <div className="hidden h-[clamp(620px,calc(100dvh-var(--header-h)-48px),900px)] grid-cols-[372px_minmax(0,1fr)] lg:grid">
        <GpuiEditorPanel
          document={document}
          layer={layer}
          resolved={editing.tokens}
          diagnostics={diagnostics}
          search={search}
          onSearch={setSearch}
          focusedPath={focusedPath}
          onRowFocus={setFocusedPath}
          rowRefs={rowRefs}
          actions={actions}
          t={g}
        />

        <div className="relative flex min-h-0 min-w-0 flex-col bg-sunken">
          <div className="fig-bar shrink-0 bg-bg">
            <span>
              fig. gpui · {mode} · {g.layer[layer]}
            </span>
            <span>{t.tb.rightClickHint}</span>
          </div>
          <div
            className="min-h-0 flex-1 bg-[radial-gradient(var(--line-strong)_1px,transparent_1px)] bg-[length:18px_18px] p-5 xl:p-7"
            onContextMenu={openInspector}
          >
            <GpuiPreview tokens={preview.tokens} mode={mode} t={t.gpuiMock} onToggleMode={() => setMode(mode === "dark" ? "light" : "dark")} />
          </div>
          <StatusToast status={status} onDismiss={() => setStatus(null)} dismissLabel={t.tb.dismiss} />
        </div>
      </div>

      <input ref={fileInputRef} type="file" accept=".json,application/json" className="hidden" onChange={handleImport} />

      {menu && active && <InspectorMenu state={menu} onClose={closeMenu} onCopy={copyText} onReveal={revealToken} t={t.tb} />}
    </>
  );
}
