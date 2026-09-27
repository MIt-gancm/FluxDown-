/** 主题截图大图:原生模态框,←/→ 切换变体(含 GPUI 截图),Esc / 遮罩关闭。 */
import { useEffect } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { markets } from "@/i18n/messages/markets";
import type { Lang } from "@/i18n/config";
import { cn } from "@/lib/utils";
import { previewSlides, themeAssetUrl, type MarketTheme } from "./theme-data";
import { DialogClose, MarketDialog } from "./MarketUI";

export interface LightboxState {
  theme: MarketTheme;
  /** previewSlides 下标(变体在前,与 orderedVariants 下标一致) */
  index: number;
}

export function ThemeLightbox({
  state,
  onClose,
  onNavigate,
  t,
  closeLabel,
}: {
  state: LightboxState;
  onClose: () => void;
  onNavigate: (index: number) => void;
  t: (typeof markets)[Lang]["themes"];
  closeLabel: string;
}) {
  const slides = previewSlides(state.theme);
  const slide = slides[state.index]!;
  const slideName = (variant: string | null) => (variant === null ? t.gpui.screenshotLabel : (t.variant[variant] ?? variant));
  const hasPrev = state.index > 0;
  const hasNext = state.index < slides.length - 1;

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "ArrowLeft" && hasPrev) onNavigate(state.index - 1);
      if (e.key === "ArrowRight" && hasNext) onNavigate(state.index + 1);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [state.index, hasPrev, hasNext, onNavigate]);

  const navButton =
    "absolute top-1/2 grid h-10 w-10 -translate-y-1/2 place-items-center rounded-md bg-[oklch(0.15_0.01_265/0.7)] text-white transition-colors hover:bg-[oklch(0.15_0.01_265/0.9)] focus-visible:outline-2 focus-visible:outline-accent";

  return (
    <MarketDialog onClose={onClose} labelledBy="theme-lightbox-title" className="max-w-5xl">
      <figure className="overflow-hidden rounded-[var(--radius)] border border-line-strong bg-elev shadow-[var(--shadow-window)]">
        <figcaption className="flex items-center justify-between gap-3 border-b border-line py-2 pl-4 pr-2">
          <div className="min-w-0 truncate text-sm">
            <span id="theme-lightbox-title" className="font-semibold text-fg">
              {state.theme.name}
            </span>
            <span className="ml-2 font-mono text-xs text-subtle">{slideName(slide.variant)}</span>
          </div>
          <DialogClose label={closeLabel} onClick={onClose} />
        </figcaption>
        <div className="relative bg-sunken">
          <img
            src={themeAssetUrl(slide.screenshot)}
            alt={t.screenshotAlt(state.theme.name, slideName(slide.variant))}
            className="block h-auto max-h-[calc(100dvh-160px)] w-full select-none object-contain"
            draggable={false}
          />
          {hasPrev && (
            <button type="button" onClick={() => onNavigate(state.index - 1)} aria-label={t.previous} className={cn(navButton, "left-3")}>
              <ChevronLeft size={18} />
            </button>
          )}
          {hasNext && (
            <button type="button" onClick={() => onNavigate(state.index + 1)} aria-label={t.next} className={cn(navButton, "right-3")}>
              <ChevronRight size={18} />
            </button>
          )}
        </div>
        {slides.length > 1 && (
          <div className="flex items-center justify-center gap-2 border-t border-line py-2.5">
            {slides.map((s, i) => (
              <button
                key={s.variant ?? "gpui"}
                type="button"
                onClick={() => onNavigate(i)}
                aria-label={t.showVariant(slideName(s.variant))}
                aria-pressed={i === state.index}
                className={cn(
                  "h-1.5 rounded-full transition-all focus-visible:outline-2 focus-visible:outline-accent",
                  i === state.index ? "w-6 bg-accent" : "w-1.5 bg-line-strong hover:bg-subtle",
                )}
              />
            ))}
          </div>
        )}
      </figure>
    </MarketDialog>
  );
}
