/**
 * 社区平台投票:GET /api/vote → { results, total };POST { option } → { message: "voted" | "already_voted" }。
 * 每位访客一票,选择记录在 localStorage。
 */
import { useEffect, useState, type CSSProperties } from "react";
import { CircleCheck, Megaphone, MessageCircle, MessagesSquare, type LucideIcon } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { polls } from "@/i18n/messages/polls";
import { cn } from "@/lib/utils";
import { Notice, Placeholder, Spinner } from "./shared";

type Option = "wechat" | "qq" | "official-account";

const STORAGE_KEY = "fluxdown-voted-community";

const OPTIONS: { key: Option; copy: "wechat" | "qq" | "officialAccount"; icon: LucideIcon; color: string }[] = [
  { key: "wechat", copy: "wechat", icon: MessageCircle, color: "var(--ok)" },
  { key: "qq", copy: "qq", icon: MessagesSquare, color: "var(--cyan)" },
  { key: "official-account", copy: "officialAccount", icon: Megaphone, color: "var(--danger)" },
];

export default function PlatformPoll({ lang }: { lang: Lang }) {
  const t = polls[lang].platform;
  const [results, setResults] = useState<{ results: Record<string, number>; total: number } | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [votedFor, setVotedFor] = useState<Option | null>(null);
  const [submitting, setSubmitting] = useState<Option | null>(null);
  const [flash, setFlash] = useState<{ text: string; tone: "ok" | "error" } | null>(null);

  useEffect(() => {
    try {
      const saved = localStorage.getItem(STORAGE_KEY);
      if (saved) setVotedFor(saved as Option);
    } catch {
      // localStorage 不可用
    }
    fetch("/api/vote")
      .then((r) => (r.ok ? r.json() : Promise.reject(r.status)))
      .then(setResults)
      .catch(() => setLoadError(true));
  }, []);

  const vote = async (option: Option) => {
    if (votedFor || submitting) return;
    setSubmitting(option);
    setFlash(null);
    try {
      const res = await fetch("/api/vote", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ option }),
      });
      if (!res.ok) {
        setFlash({ text: res.status === 429 ? t.rateLimited : t.error, tone: "error" });
        return;
      }
      const data = (await res.json()) as { message?: string };
      setVotedFor(option);
      if (data.message === "already_voted") {
        setFlash({ text: t.alreadyVoted, tone: "ok" });
      } else {
        setFlash({ text: t.success, tone: "ok" });
        setResults((prev) =>
          prev
            ? { results: { ...prev.results, [option]: (prev.results[option] ?? 0) + 1 }, total: prev.total + 1 }
            : prev,
        );
      }
      try {
        localStorage.setItem(STORAGE_KEY, option);
      } catch {
        // localStorage 不可用
      }
    } catch {
      setFlash({ text: t.error, tone: "error" });
    } finally {
      setSubmitting(null);
    }
  };

  if (loadError) return <Placeholder tone="error">{t.loadError}</Placeholder>;
  if (!results) return <Placeholder tone="loading">{t.loading}</Placeholder>;

  return (
    <>
      <div className="cells md:grid-cols-3">
        {OPTIONS.map((opt) => {
          const count = results.results[opt.key] ?? 0;
          const pct = results.total ? Math.round((count / results.total) * 100) : 0;
          const mine = votedFor === opt.key;
          const Icon = opt.icon;
          return (
            <button
              key={opt.key}
              type="button"
              style={{ "--c": opt.color, "--pct": `${pct}%` } as CSSProperties}
              className={cn("cm-poll cell spot", mine && "is-mine", votedFor && !mine && "is-dim")}
              aria-pressed={mine}
              disabled={!!votedFor || !!submitting}
              onClick={() => vote(opt.key)}
            >
              <span className="flex items-center justify-between">
                <span className="cm-poll-icon">
                  <Icon aria-hidden className="size-5" />
                </span>
                {mine && (
                  <span className="mono inline-flex items-center gap-1.5 text-[11px] tracking-[0.12em] uppercase" style={{ color: opt.color }}>
                    <CircleCheck aria-hidden className="size-3.5" />
                    {t.yourVote}
                  </span>
                )}
              </span>
              <span className="mt-6 flex flex-col gap-1.5 text-left">
                <span className="text-lg font-medium tracking-tight text-fg">{t.options[opt.copy].name}</span>
                <span className="text-sm leading-relaxed text-muted">{t.options[opt.copy].desc}</span>
              </span>
              <span className="mt-auto flex flex-col gap-2 pt-8">
                <span className="mono num flex items-baseline justify-between text-xs text-muted">
                  <span>{t.votes(count)}</span>
                  <span className="text-2xl font-medium tracking-tight" style={{ color: opt.color }}>
                    {pct}%
                  </span>
                </span>
                <span className="cm-meter" aria-hidden>
                  <span />
                </span>
              </span>
              {!votedFor && (
                <span className="btn btn-secondary btn-sm mt-5 w-full">
                  {submitting === opt.key && <Spinner />}
                  {submitting === opt.key ? t.submitting : t.submitVote}
                </span>
              )}
            </button>
          );
        })}
      </div>
      <div className="flex flex-col items-center gap-3 border-t border-line px-6 py-6">
        <p className="mono num text-xs tracking-wide text-subtle uppercase">{t.totalVotes(results.total)}</p>
        <div aria-live="polite">{flash && <Notice tone={flash.tone}>{flash.text}</Notice>}</div>
      </div>
    </>
  );
}
