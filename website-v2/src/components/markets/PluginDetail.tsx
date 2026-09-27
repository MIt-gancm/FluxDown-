/** 插件详情弹层:元信息、权限说明、安装指引与可追溯的版本历史时间线。 */
import { useState } from "react";
import { ArrowUpRight, Check, Copy, Download, Puzzle } from "lucide-react";
import type { markets } from "@/i18n/messages/markets";
import type { Lang } from "@/i18n/config";
import { cn } from "@/lib/utils";
import { bestMirror, commitsUrl, isoDate, shortHash, type PluginGroup } from "./plugin-data";
import { DialogClose, MarketDialog } from "./MarketUI";
import { PermissionBadge, TagChip, YankedBadge } from "./PluginCard";

type Messages = (typeof markets)[Lang];

export function PluginDetail({
  group,
  onClose,
  t,
  closeLabel,
}: {
  group: PluginGroup;
  onClose: () => void;
  t: Messages["plugins"];
  closeLabel: string;
}) {
  const p = group.latest;
  const name = p.name || group.pluginId;
  const titleId = `plugin-detail-${group.pluginId.replace(/[^\w-]/g, "-")}`;
  const [copied, setCopied] = useState(false);

  const copyId = async () => {
    try {
      await navigator.clipboard.writeText(group.pluginId);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      // 剪贴板不可用(非安全上下文等)时静默:ID 仍以可选中文本呈现。
    }
  };

  return (
    <MarketDialog onClose={onClose} labelledBy={titleId} className="max-w-2xl">
      <div className="flex max-h-[calc(100dvh-32px)] flex-col overflow-hidden rounded-[var(--radius)] border border-line-strong bg-elev shadow-[var(--shadow-window)]">
        <div className="fig-bar shrink-0">
          <span>plugin</span>
          <span className="normal-case">{group.pluginId}</span>
        </div>

        <div className="overflow-y-auto overscroll-contain">
          <header className="flex items-start gap-4 p-6 pb-0 sm:p-8 sm:pb-0">
            <span className="grid h-11 w-11 shrink-0 place-items-center rounded-md text-accent-ink shadow-[inset_0_0_0_1px_var(--line-strong)]">
              <Puzzle size={21} aria-hidden />
            </span>
            <div className="min-w-0 flex-1">
              <div className="flex flex-wrap items-center gap-2">
                <h2 id={titleId} className="text-xl font-semibold tracking-[-0.02em] text-fg">
                  {name}
                </h2>
                <span className="num font-mono text-xs text-subtle">v{p.version}</span>
                <YankedBadge yanked={p.yanked} t={t} />
              </div>
              <button
                type="button"
                onClick={copyId}
                className="mt-1 inline-flex max-w-full items-center gap-1.5 rounded font-mono text-xs text-subtle transition-colors hover:text-fg focus-visible:outline-2 focus-visible:outline-accent"
                aria-label={t.detail.copyId}
              >
                <span className="truncate">{group.pluginId}</span>
                {copied ? <Check size={12} className="text-ok" /> : <Copy size={12} />}
                <span className="sr-only" aria-live="polite">
                  {copied ? t.detail.copied : ""}
                </span>
              </button>
            </div>
            <DialogClose label={closeLabel} onClick={onClose} />
          </header>

          <div className="space-y-6 p-6 sm:p-8">
            {p.description && <p className="text-[14.5px] leading-relaxed text-muted">{p.description}</p>}

            {(p.tags ?? []).length > 0 && (
              <div className="flex flex-wrap gap-1.5">
                {(p.tags ?? []).map((tag) => (
                  <TagChip key={tag} tag={tag} />
                ))}
              </div>
            )}

            <dl className="cells grid-cols-2 border border-line text-[13px] [&>div]:bg-elev">
              <div className="p-3.5">
                <dt className="eyebrow-plain">{t.detail.author}</dt>
                <dd className="mt-1 truncate text-fg">@{p.author || t.unknownAuthor}</dd>
              </div>
              <div className="p-3.5">
                <dt className="eyebrow-plain">{t.detail.minApp}</dt>
                <dd className="num mt-1 font-mono text-fg">{p.minAppVersion ? `v${p.minAppVersion}` : "—"}</dd>
              </div>
              {p.homepage && (
                <div className="col-span-2 p-3.5">
                  <dt className="eyebrow-plain">{t.detail.homepage}</dt>
                  <dd className="mt-1">
                    <a href={p.homepage} target="_blank" rel="noopener noreferrer" className="link break-all">
                      {p.homepage}
                    </a>
                  </dd>
                </div>
              )}
            </dl>

            {(p.permissions ?? []).length > 0 && (
              <section>
                <h3 className="eyebrow-plain mb-2.5">{t.detail.permissions}</h3>
                <ul className="space-y-2">
                  {(p.permissions ?? []).map((perm) => (
                    <li key={perm} className="flex flex-wrap items-center gap-2 text-[13px] text-muted">
                      <PermissionBadge perm={perm} t={t} />
                      {(perm === "ffmpeg" || perm === "ytdlp") && <span>{t.permission[perm]}</span>}
                    </li>
                  ))}
                </ul>
              </section>
            )}

            <section className="border-l-2 border-accent bg-accent-soft px-4 py-3.5">
              <h3 className="eyebrow mb-1.5">{t.detail.install}</h3>
              <p className="text-[13px] leading-relaxed text-muted">{t.detail.installHint(group.pluginId)}</p>
            </section>

            <section>
              <div className="mb-4 flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-fg">{t.detail.history}</h3>
                <a
                  href={commitsUrl(group.pluginId)}
                  target="_blank"
                  rel="noopener noreferrer"
                  className="link inline-flex items-center gap-1 text-xs"
                >
                  {t.detail.viewCommits}
                  <ArrowUpRight size={12} aria-hidden />
                </a>
              </div>

              <ol className="relative ml-1 space-y-5 border-l border-line pl-6">
                {group.versions.map((v, i) => {
                  const mirror = bestMirror(v);
                  const date = isoDate(v.publishTime);
                  return (
                    <li key={`${v.version}-${v.sequence}`} className="relative">
                      <span
                        aria-hidden
                        className={cn(
                          "absolute -left-[29px] top-1.5 h-[9px] w-[9px] rotate-45 border",
                          i === 0 ? "border-accent bg-accent" : "border-line-strong bg-elev",
                        )}
                      />
                      <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex flex-wrap items-center gap-2">
                          <span className="num font-mono text-sm font-semibold text-fg">v{v.version}</span>
                          {i === 0 && (
                            <span className="inline-flex h-5 items-center rounded-[4px] bg-accent-soft px-1.5 font-mono text-[10.5px] uppercase tracking-wider text-accent-ink">
                              {t.detail.latest}
                            </span>
                          )}
                          <YankedBadge yanked={v.yanked} t={t} />
                        </div>
                        {date && <time dateTime={v.publishTime} className="num font-mono text-[11px] text-subtle">{date}</time>}
                      </div>
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 font-mono text-[11px] text-subtle">
                        <span className="num">seq #{v.sequence}</span>
                        {v.minAppVersion && <span className="num">min v{v.minAppVersion}</span>}
                        <span className="truncate" title={v.contentHash}>
                          sha256:{shortHash(v.contentHash)}
                        </span>
                      </div>
                      {mirror && (
                        <a
                          href={mirror}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="link mt-2 inline-flex items-center gap-1 text-xs"
                        >
                          <Download size={12} aria-hidden />
                          {t.detail.download}
                        </a>
                      )}
                    </li>
                  );
                })}
              </ol>
            </section>
          </div>
        </div>
      </div>
    </MarketDialog>
  );
}
