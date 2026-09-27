import type { CSSProperties } from "react";
import { ArrowUpRight, Puzzle, ShieldAlert } from "lucide-react";
import type { markets } from "@/i18n/messages/markets";
import type { Lang } from "@/i18n/config";
import { cn } from "@/lib/utils";
import { yankState, type PluginGroup } from "./plugin-data";

type PluginMessages = (typeof markets)[Lang]["plugins"];

export function PermissionBadge({ perm, t }: { perm: string; t: PluginMessages }) {
  const title = perm === "ffmpeg" || perm === "ytdlp" ? t.permission[perm] : undefined;
  return (
    <span
      title={title}
      className="inline-flex h-5 items-center gap-1 rounded-[4px] px-1.5 font-mono text-[10.5px] text-warn shadow-[inset_0_0_0_1px_color-mix(in_oklch,var(--warn)_40%,transparent)]"
    >
      <ShieldAlert size={11} aria-hidden />
      {perm}
    </span>
  );
}

export function YankedBadge({ yanked, t }: { yanked?: string; t: PluginMessages }) {
  const state = yankState(yanked);
  if (!state) return null;
  return (
    <span className="inline-flex h-5 items-center rounded-[4px] bg-[color-mix(in_oklch,var(--danger)_12%,transparent)] px-1.5 font-mono text-[10.5px] uppercase tracking-wider text-danger">
      {t.yanked[state]}
    </span>
  );
}

export function TagChip({ tag }: { tag: string }) {
  return (
    <span className="inline-flex h-5 items-center rounded-[4px] px-1.5 font-mono text-[10.5px] text-subtle shadow-[inset_0_0_0_1px_var(--line)]">
      {tag}
    </span>
  );
}

export function PluginCard({
  group,
  index,
  onOpen,
  t,
}: {
  group: PluginGroup;
  index: number;
  onOpen: () => void;
  t: PluginMessages;
}) {
  const p = group.latest;
  const name = p.name || group.pluginId;
  const yanked = yankState(p.yanked);

  return (
    <button
      type="button"
      onClick={onOpen}
      data-reveal
      style={{ "--d": index % 6 } as CSSProperties}
      className="spot cell group flex h-full flex-col text-left outline-none transition-colors hover:bg-elev focus-visible:bg-elev focus-visible:shadow-[inset_0_0_0_1px_var(--accent)]"
    >
      <div className="flex items-start justify-between gap-3">
        <span
          className={cn(
            "grid h-9 w-9 shrink-0 place-items-center rounded-md shadow-[inset_0_0_0_1px_var(--line-strong)]",
            yanked ? "text-danger" : "text-accent-ink",
          )}
        >
          <Puzzle size={17} aria-hidden />
        </span>
        <span className="flex items-center gap-2 font-mono text-[11px] text-subtle">
          <span className="num">v{p.version}</span>
          <ArrowUpRight
            size={14}
            aria-hidden
            className="transition-transform duration-200 group-hover:-translate-y-0.5 group-hover:translate-x-0.5 group-hover:text-accent-ink"
          />
        </span>
      </div>

      <h3 className="mt-5 truncate text-[15px] font-semibold tracking-[-0.01em] text-fg">{name}</h3>
      <span className="mt-0.5 truncate font-mono text-[11.5px] text-subtle">{group.pluginId}</span>

      {yanked && (
        <div className="mt-3">
          <YankedBadge yanked={p.yanked} t={t} />
        </div>
      )}

      {p.description && (
        <p className="mt-3 line-clamp-3 flex-1 text-[13px] leading-relaxed text-muted">{p.description}</p>
      )}

      <div className="mt-4 flex flex-wrap items-center gap-1.5">
        {(p.permissions ?? []).map((perm) => (
          <PermissionBadge key={perm} perm={perm} t={t} />
        ))}
        {(p.tags ?? []).slice(0, 3).map((tag) => (
          <TagChip key={tag} tag={tag} />
        ))}
      </div>

      <div className="mt-5 flex items-center justify-between gap-2 border-t border-dashed border-line pt-3 font-mono text-[11px] text-subtle">
        <span className="truncate">@{p.author || t.unknownAuthor}</span>
        <span className="num shrink-0">{t.versions(group.versions.length)}</span>
      </div>
    </button>
  );
}
