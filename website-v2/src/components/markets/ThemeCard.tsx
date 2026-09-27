/** 主题卡片:截图(可放大)+ 由主题 token 渲染的调色板 + 变体切换 + 每变体下载;带 GPUI 资源时附徽标、GPUI 下载与编辑器深链接。 */
import { useEffect, useRef, useState, type CSSProperties } from "react";
import { Download, PencilRuler, ZoomIn } from "lucide-react";
import type { markets } from "@/i18n/messages/markets";
import type { Lang } from "@/i18n/config";
import { argbToCssRgba, getPathValue, type FluxThemeJson } from "@/lib/theme-builder";
import { cn } from "@/lib/utils";
import {
  downloadThemeFile,
  gpuiAsset,
  gpuiEditorHref,
  loadThemeTokens,
  orderedVariants,
  themeAssetUrl,
  type MarketTheme,
} from "./theme-data";

type ThemeMessages = (typeof markets)[Lang]["themes"];

const PALETTE_PATHS = [
  "colors.surface.background",
  "colors.surface.surface1",
  "colors.surface.surface2",
  "colors.surface.surface3",
  "colors.text.primary",
  "colors.text.muted",
  "colors.accent.color",
  "colors.status.success",
  "colors.status.warning",
  "colors.status.error",
] as const;

/** 进入视口后才拉取主题 JSON(每路径缓存一次),避免首屏为所有卡片发请求。 */
function useThemeTokens(path: string) {
  const ref = useRef<HTMLDivElement>(null);
  const [visible, setVisible] = useState(false);
  const [tokens, setTokens] = useState<{ path: string; theme: FluxThemeJson | null } | null>(null);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          setVisible(true);
          io.disconnect();
        }
      },
      { rootMargin: "200px" },
    );
    io.observe(el);
    return () => io.disconnect();
  }, []);

  useEffect(() => {
    if (!visible) return;
    let alive = true;
    loadThemeTokens(path)
      .then((theme) => alive && setTokens({ path, theme }))
      .catch(() => alive && setTokens({ path, theme: null }));
    return () => {
      alive = false;
    };
  }, [visible, path]);

  return { ref, theme: tokens?.path === path ? tokens.theme : undefined };
}

function Palette({ path, label }: { path: string; label: string }) {
  const { ref, theme } = useThemeTokens(path);
  if (theme === null) return <div ref={ref} />;
  return (
    <div ref={ref} className="mt-4">
      <div className="eyebrow-plain mb-2 text-[10px]">{label}</div>
      <div className="overflow-hidden rounded-[4px] shadow-[0_0_0_1px_var(--line-strong)]">
        <div className="flex h-7">
          {PALETTE_PATHS.map((p) => {
            const value = theme ? getPathValue(theme, p) : undefined;
            return typeof value === "string" ? (
              <span
                key={p}
                title={`${p.slice("colors.".length)} · #${value}`}
                className="flex-1"
                style={{ backgroundColor: argbToCssRgba(value) }}
              />
            ) : (
              <span key={p} className="flex-1 animate-pulse bg-inset" />
            );
          })}
        </div>
        <div className="flex h-1.5" aria-hidden>
          {(theme?.colors.segmentPalette ?? []).map((c, i) => (
            <span key={i} className="flex-1" style={{ backgroundColor: argbToCssRgba(c) }} />
          ))}
        </div>
      </div>
    </div>
  );
}

export function ThemeCard({
  theme,
  index,
  onPreview,
  t,
  lang,
}: {
  theme: MarketTheme;
  index: number;
  onPreview: (variantIndex: number) => void;
  t: ThemeMessages;
  lang: Lang;
}) {
  const variants = orderedVariants(theme);
  const [active, setActive] = useState(0);
  const [activeKey, cover] = variants[Math.min(active, variants.length - 1)]!;
  const variantName = (key: string) => t.variant[key] ?? key;
  const gpui = gpuiAsset(theme);

  return (
    <article
      data-reveal
      style={{ "--d": index % 6 } as CSSProperties}
      className="group flex flex-col transition-colors hover:bg-elev"
    >
      <button
        type="button"
        onClick={() => onPreview(active)}
        aria-label={t.preview(theme.name)}
        className="relative block aspect-[16/10] w-full cursor-zoom-in overflow-hidden border-b border-line bg-sunken outline-none focus-visible:shadow-[inset_0_0_0_2px_var(--accent)]"
      >
        <img
          src={themeAssetUrl(cover.screenshot)}
          alt={t.screenshotAlt(theme.name, variantName(activeKey))}
          loading={index < 3 ? "eager" : "lazy"}
          fetchPriority={index === 0 ? "high" : "auto"}
          decoding="async"
          className="absolute inset-0 h-full w-full object-cover transition-transform duration-500 ease-out group-hover:scale-[1.015]"
        />
        <span className="absolute bottom-2.5 right-2.5 inline-flex items-center gap-1 rounded-[4px] bg-[oklch(0.15_0.01_265/0.72)] px-2 py-1 font-mono text-[10.5px] text-white opacity-0 transition-opacity group-hover:opacity-100 group-focus-within:opacity-100">
          <ZoomIn size={12} aria-hidden />
          {t.clickToZoom}
        </span>
      </button>

      <div className="flex flex-1 flex-col p-5">
        <div className="flex items-baseline justify-between gap-2">
          <h3 className="truncate text-[15px] font-semibold tracking-[-0.01em] text-fg">{theme.name}</h3>
          <span className="flex shrink-0 items-baseline gap-2">
            {gpui && (
              <span
                title={t.gpui.badgeTitle}
                className="inline-flex h-5 items-center rounded-[4px] px-1.5 font-mono text-[10.5px] text-accent-ink shadow-[inset_0_0_0_1px_var(--line-strong)]"
              >
                {t.gpui.badge}
              </span>
            )}
            <span className="num font-mono text-[11px] text-subtle">v{theme.version}</span>
          </span>
        </div>
        <a
          href={`https://github.com/${theme.author}`}
          target="_blank"
          rel="noopener noreferrer"
          className="mt-0.5 self-start font-mono text-[11.5px] text-subtle transition-colors hover:text-accent-ink"
        >
          @{theme.author}
        </a>
        {theme.description && (
          <p className="mt-2.5 line-clamp-2 text-[13px] leading-relaxed text-muted">{theme.description}</p>
        )}

        <div className="mt-4 flex items-center justify-between gap-2">
          {variants.length > 1 ? (
            <div
              role="group"
              aria-label={t.variantsLabel}
              className="inline-flex rounded-md p-0.5 shadow-[inset_0_0_0_1px_var(--line-strong)]"
            >
              {variants.map(([vk], i) => (
                <button
                  key={vk}
                  type="button"
                  aria-pressed={i === active}
                  onClick={() => setActive(i)}
                  className={cn(
                    "h-6 rounded-[4px] px-2.5 font-mono text-[11px] transition-colors focus-visible:outline-2 focus-visible:outline-accent",
                    i === active ? "bg-inset text-fg" : "text-subtle hover:text-fg",
                  )}
                >
                  {variantName(vk)}
                </button>
              ))}
            </div>
          ) : (
            <span className="font-mono text-[11px] text-subtle">{variantName(activeKey)}</span>
          )}
          {theme.tags && theme.tags.length > 0 && (
            <div className="flex min-w-0 items-center gap-1 overflow-hidden">
              {theme.tags.slice(0, 2).map((tag) => (
                <span
                  key={tag}
                  className="inline-flex h-5 shrink-0 items-center rounded-[4px] px-1.5 font-mono text-[10.5px] text-subtle shadow-[inset_0_0_0_1px_var(--line)]"
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>

        <Palette path={cover.theme} label={t.palette} />

        <div className="mt-5 flex flex-1 flex-col justify-end gap-2">
          <div className="flex gap-2">
            {variants.map(([vk, asset]) => (
              <button
                key={vk}
                type="button"
                onClick={() => {
                  downloadThemeFile(asset.theme).catch(() => window.open(themeAssetUrl(asset.theme), "_blank"));
                }}
                className="btn btn-sm btn-secondary flex-1"
              >
                <Download size={14} aria-hidden />
                {variants.length > 1 ? t.downloadVariant(variantName(vk)) : t.download}
              </button>
            ))}
          </div>
          {gpui && (
            <div className="flex gap-2">
              <button
                type="button"
                onClick={() => {
                  downloadThemeFile(gpui.theme).catch(() => window.open(themeAssetUrl(gpui.theme), "_blank"));
                }}
                className="btn btn-sm btn-secondary flex-1"
              >
                <Download size={14} aria-hidden />
                {t.gpui.download}
              </button>
              <a
                href={gpuiEditorHref(gpui.theme, lang)}
                title={t.gpui.openInEditorTitle(theme.name)}
                className="btn btn-sm btn-ghost flex-1"
              >
                <PencilRuler size={14} aria-hidden />
                {t.gpui.openInEditor}
              </a>
            </div>
          )}
        </div>
      </div>
    </article>
  );
}
