/** 右键 token 检视器:列出被点击区块绑定的 token 路径与当前值,可复制或跳转编辑。 */
import { useEffect, useRef } from "react";
import { Copy, PencilLine } from "lucide-react";
import { argbToCssRgba } from "@/lib/theme-builder";
import type { ThemeBuilderMessages } from "@/i18n/messages/themeBuilder";

export interface InspectorEntry {
  path: string;
  value: string;
  /** 色块 CSS 颜色；缺省时按 Flutter ARGB hex8 识别 `value`。 */
  swatch?: string;
}

export interface InspectorState {
  x: number;
  y: number;
  entries: InspectorEntry[];
}

const MINI =
  "inline-flex h-6 items-center gap-1 rounded-[4px] px-1.5 font-mono text-[10px] text-muted shadow-[inset_0_0_0_1px_var(--line-strong)] transition-colors hover:bg-inset hover:text-fg focus-visible:outline-2 focus-visible:outline-accent";

export function InspectorMenu({
  state,
  onClose,
  onCopy,
  onReveal,
  t,
}: {
  state: InspectorState;
  onClose: () => void;
  onCopy: (value: string) => void;
  onReveal: (path: string) => void;
  t: ThemeBuilderMessages["tb"];
}) {
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button")?.focus({ preventScroll: true });
    const onPointer = (e: PointerEvent) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    const onScroll = (e: Event) => {
      if (!ref.current?.contains(e.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", onPointer, true);
    window.addEventListener("keydown", onKey);
    window.addEventListener("scroll", onScroll, true);
    window.addEventListener("resize", onClose);
    return () => {
      window.removeEventListener("pointerdown", onPointer, true);
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("scroll", onScroll, true);
      window.removeEventListener("resize", onClose);
    };
  }, [onClose]);

  return (
    <div
      ref={ref}
      role="dialog"
      aria-label={t.context.title}
      className="fixed z-[100] w-[320px] overflow-hidden rounded-[var(--radius)] border border-line-strong bg-elev text-fg shadow-[var(--shadow-window)] transition-[opacity,translate] duration-150 starting:-translate-y-1 starting:opacity-0"
      style={{ left: state.x, top: state.y }}
    >
      <div className="fig-bar">
        <span>{t.context.title}</span>
        <span className="num">{String(state.entries.length).padStart(2, "0")}</span>
      </div>
      <ul className="max-h-[280px] divide-y divide-line overflow-y-auto [scrollbar-width:thin]">
        {state.entries.map((entry) => {
          const color = entry.swatch ?? (/^[0-9a-f]{8}$/i.test(entry.value) ? argbToCssRgba(entry.value) : null);
          return (
            <li key={entry.path} className="space-y-1.5 px-3 py-2.5">
              <div className="flex items-center gap-2">
                {color && (
                  <span
                    className="h-3 w-3 shrink-0 rounded-[3px] shadow-[inset_0_0_0_1px_var(--line-strong)]"
                    style={{ backgroundColor: color }}
                  />
                )}
                <code className="min-w-0 flex-1 truncate font-mono text-[11px] text-muted">{entry.path}</code>
              </div>
              <div className="flex items-center justify-between gap-2">
                <code className="min-w-0 truncate font-mono text-xs text-fg">{entry.value}</code>
                <div className="flex shrink-0 items-center gap-1">
                  <button type="button" onClick={() => onReveal(entry.path)} className={MINI} aria-label={`${t.context.reveal}: ${entry.path}`} title={t.context.reveal}>
                    <PencilLine size={11} aria-hidden />
                  </button>
                  <button type="button" onClick={() => onCopy(entry.path)} className={MINI}>
                    <Copy size={10} aria-hidden />
                    {t.context.copyPath}
                  </button>
                  <button type="button" onClick={() => onCopy(entry.value)} className={MINI}>
                    <Copy size={10} aria-hidden />
                    {t.context.copyValue}
                  </button>
                </div>
              </div>
            </li>
          );
        })}
      </ul>
    </div>
  );
}
