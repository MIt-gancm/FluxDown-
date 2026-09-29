import { useEffect, useState } from "react";
import { ArrowUpRight } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { sponsor } from "@/i18n/messages/sponsor";
import { SPONSOR_WALL_URL, fmtCents, fmtWallDate, type WallSponsor } from "./wall";
import { withBase } from "@/lib/base";

/** Latest sponsors from the GitHub wall: top 3 by amount as "special thanks", the rest newest first. */
export default function SponsorWall({ lang }: { lang: Lang }) {
  const t = sponsor[lang];
  const [sponsors, setSponsors] = useState<WallSponsor[] | null>(null);

  useEffect(() => {
    let alive = true;
    fetch(withBase("/api/sponsor/list"))
      .then((r) => (r.ok ? r.json() : null))
      .then((d: { sponsors?: WallSponsor[] } | null) => {
        if (alive) setSponsors(Array.isArray(d?.sponsors) ? d.sponsors : []);
      })
      .catch(() => alive && setSponsors([]));
    return () => {
      alive = false;
    };
  }, []);

  const list = sponsors ?? [];
  const featuredIdx = list
    .map((_, i) => i)
    .sort((a, b) => list[b]!.amountCents - list[a]!.amountCents)
    .slice(0, 3);
  const featuredSet = new Set(featuredIdx);
  const featured = featuredIdx.map((i) => list[i]!);
  const rest = list.filter((_, i) => !featuredSet.has(i));

  return (
    <div className="flex h-full flex-col">
      <div className="mb-6 flex flex-wrap items-end justify-between gap-3">
        <div className="flex flex-col gap-3">
          <span className="eyebrow">{t.wall.eyebrow}</span>
          <h2 id="sponsor-wall-heading" className="h3">
            {t.wall.title}
          </h2>
        </div>
        <a href={SPONSOR_WALL_URL} target="_blank" rel="noopener noreferrer" className="link inline-flex items-center gap-1 text-[13px]">
          {t.wallLink}
          <ArrowUpRight size={13} aria-hidden />
        </a>
      </div>

      {sponsors === null ? (
        <div role="status" aria-label={t.wall.loading} className="flex flex-col gap-2">
          <div className="grid grid-cols-3 gap-2">
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-36 animate-pulse rounded-[6px] bg-inset" />
            ))}
          </div>
          {[0, 1, 2].map((i) => (
            <div key={i} className="h-12 animate-pulse rounded-[6px] bg-inset" />
          ))}
        </div>
      ) : list.length === 0 ? (
        <p className="flex flex-1 items-center justify-center border border-dashed border-line-strong px-6 py-14 text-center text-sm text-muted">
          {t.wall.empty}
        </p>
      ) : (
        <>
          <p className="eyebrow-plain mb-3">{t.wall.featured}</p>
          <ul className="cells grid-cols-3 border border-line">
            {featured.map((s, i) => (
              <li key={`f-${s.name}-${s.date}-${i}`} className="flex min-w-0 flex-col items-center px-2 py-5 text-center sm:px-3">
                <SponsorAvatar sponsor={s} size={48} />
                <span className="mt-2.5 max-w-full truncate text-[13px] font-medium" title={s.name}>
                  {s.name}
                </span>
                <span className="num mt-1 flex flex-wrap justify-center gap-x-1.5 font-mono text-[11px] text-subtle">
                  {s.amountCents > 0 && <span className="text-accent-ink">{fmtCents(s.amountCents)}</span>}
                  <span>{fmtWallDate(s.date)}</span>
                </span>
                {s.message && (
                  <p className="mt-2 line-clamp-3 w-full whitespace-pre-line break-words text-[11.5px] leading-relaxed text-muted" title={s.message}>
                    “{s.message}”
                  </p>
                )}
              </li>
            ))}
          </ul>

          {rest.length > 0 && (
            <ul className="mt-6 divide-y divide-line border-y border-line">
              {rest.map((s, i) => (
                <li key={`${s.name}-${s.date}-${i}`} className="py-3">
                  <div className="flex items-center gap-3">
                    <SponsorAvatar sponsor={s} size={30} />
                    <div className="min-w-0 flex-1">
                      <span className="block truncate text-sm" title={s.name}>
                        {s.name}
                      </span>
                      <span className="num block font-mono text-[11px] text-subtle">{fmtWallDate(s.date)}</span>
                    </div>
                    {s.amountCents > 0 && (
                      <span className="num shrink-0 font-mono text-xs font-medium text-accent-ink">{fmtCents(s.amountCents)}</span>
                    )}
                  </div>
                  {s.message && (
                    <p className="mt-1.5 line-clamp-2 whitespace-pre-line break-words pl-[42px] text-xs leading-relaxed text-muted" title={s.message}>
                      “{s.message}”
                    </p>
                  )}
                </li>
              ))}
            </ul>
          )}
        </>
      )}
    </div>
  );
}

function SponsorAvatar({ sponsor: s, size }: { sponsor: WallSponsor; size: number }) {
  if (s.avatar) {
    return (
      <img
        src={s.avatar}
        alt=""
        width={size}
        height={size}
        loading="lazy"
        referrerPolicy="no-referrer"
        className="shrink-0 rounded-full bg-inset object-cover shadow-[0_0_0_1px_var(--line)]"
        style={{ width: size, height: size }}
      />
    );
  }
  return (
    <span
      aria-hidden
      className="grid shrink-0 place-items-center rounded-full bg-inset font-mono font-medium text-muted shadow-[0_0_0_1px_var(--line)_inset]"
      style={{ width: size, height: size, fontSize: size * 0.38 }}
    >
      {s.name.slice(0, 1).toUpperCase()}
    </span>
  );
}
