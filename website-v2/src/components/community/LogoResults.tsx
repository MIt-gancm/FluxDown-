/**
 * Logo 投票结果:GET /api/logo-vote(已按票数降序)。
 * 投票已结束 —— /api/logo-vote POST 与 /api/logo-submit 均恒返回 403,因此只展示最终排名。
 */
import { useEffect, useState, type CSSProperties } from "react";
import { ImageOff } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { polls } from "@/i18n/messages/polls";
import { cn } from "@/lib/utils";
import { Placeholder } from "./shared";
import { withBase } from "@/lib/base";

interface Logo {
  id: string;
  filename: string;
  submitterName: string;
  description: string;
  uploadedAt: string;
  votes: number;
  isBuiltin: boolean;
  imageUrl?: string;
}

function LogoImage({ src, alt }: { src: string | null; alt: string }) {
  const [broken, setBroken] = useState(false);
  if (!src || broken) return <ImageOff aria-hidden className="size-8 text-subtle" />;
  return (
    <img
      src={src}
      alt={alt}
      loading="lazy"
      decoding="async"
      className="max-h-36 max-w-[88%] object-contain transition-transform duration-300 group-hover:scale-[1.04]"
      onError={() => setBroken(true)}
    />
  );
}

export default function LogoResults({ lang }: { lang: Lang }) {
  const t = polls[lang].logo;
  const [logos, setLogos] = useState<Logo[] | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    fetch(withBase("/api/logo-vote"))
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then((json: unknown) => {
        const list = Array.isArray(json) ? json : (json as { logos?: unknown })?.logos;
        setLogos(Array.isArray(list) ? (list as Logo[]) : []);
      })
      .catch(() => setError(true));
  }, []);

  if (error) return <Placeholder tone="error">{t.loadError}</Placeholder>;

  if (!logos) {
    return (
      <div role="status" aria-label={t.loading} className="cells grid-cols-1 min-[480px]:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
        {Array.from({ length: 8 }, (_, i) => (
          <div key={i} className="animate-pulse">
            <div className="h-44 bg-sunken" />
            <div className="flex flex-col gap-2 p-4">
              <div className="h-3 w-2/3 rounded-sm bg-inset" />
              <div className="h-3 w-1/3 rounded-sm bg-inset" />
            </div>
          </div>
        ))}
      </div>
    );
  }

  if (logos.length === 0) return <Placeholder>{t.noLogos}</Placeholder>;

  const totalVotes = logos.reduce((sum, l) => sum + l.votes, 0);

  return (
    <>
      <div className="mono num flex flex-wrap items-center gap-x-6 gap-y-1 border-b border-line px-[clamp(20px,4vw,56px)] py-3 text-[11px] tracking-[0.12em] text-subtle uppercase">
        <span>{t.total(logos.length)}</span>
        <span>{t.totalVotes(totalVotes)}</span>
      </div>
      <ol className="cells grid-cols-1 min-[480px]:grid-cols-2 md:grid-cols-3 lg:grid-cols-4">
        {logos.map((logo, i) => {
          const rank = i + 1;
          const src = logo.isBuiltin ? withBase(`/logos/${logo.filename}`) : (logo.imageUrl ?? null);
          return (
            <li key={logo.id} data-reveal style={{ "--d": Math.min(i, 8) } as CSSProperties} className={cn("group cm-logo", rank === 1 && "is-top")}>
              <div className="cm-checker relative grid h-44 place-items-center overflow-hidden border-b border-line">
                {rank <= 10 && (
                  <span className="cm-logo-rank mono num" aria-label={t.rank(rank)}>
                    {String(rank).padStart(2, "0")}
                  </span>
                )}
                <span className={cn("chip absolute top-3 right-3 h-5 px-2 text-[10px]", logo.isBuiltin && "text-accent-ink")}>
                  {logo.isBuiltin ? t.builtin : t.community}
                </span>
                <LogoImage src={src} alt={t.alt(rank)} />
              </div>
              <div className="flex flex-col gap-1.5 p-4">
                <span className="text-xs text-subtle">{t.uploadedBy(logo.submitterName || t.anonymous)}</span>
                {logo.description && <p className="line-clamp-2 text-xs leading-relaxed text-muted">{logo.description}</p>}
                <span className="mono num mt-1 text-sm font-medium text-fg">{t.votes(logo.votes)}</span>
              </div>
            </li>
          );
        })}
      </ol>
    </>
  );
}
