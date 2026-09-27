import { Check } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { pricingVote } from "@/i18n/messages/pricingVote";

export type Plan = "lifetime" | "subscription";

/** Option ids must match VALID_OPTIONS in api/pricing-vote.ts. */
export const POLL_OPTIONS: Record<Plan, { id: string; label: string }[]> = {
  lifetime: [
    { id: "lt-69", label: "¥69" },
    { id: "lt-99", label: "¥99" },
    { id: "lt-129", label: "¥129" },
    { id: "lt-199", label: "¥199" },
  ],
  subscription: [
    { id: "sub-3", label: "¥3" },
    { id: "sub-6", label: "¥6" },
    { id: "sub-10", label: "¥10" },
    { id: "sub-15plus", label: "¥15+" },
  ],
};

interface Props {
  lang: Lang;
  plan: Plan;
  index: number;
  results: Record<string, number>;
  total: number;
  chosen: string | null;
  busy: boolean;
  onVote: (plan: Plan, option: string) => void;
}

export default function PollCard({ lang, plan, index, results, total, chosen, busy, onVote }: Props) {
  const t = pricingVote[lang];
  const voted = chosen !== null;
  const headingId = `poll-${plan}`;

  return (
    <section className="cell flex flex-col" aria-labelledby={headingId} aria-busy={busy}>
      <p className="eyebrow-plain">
        <span className="num">{String(index + 1).padStart(2, "0")}</span> · {t.pollTag[plan]}
      </p>
      <h3 id={headingId} className="mt-3 text-lg font-semibold tracking-tight">
        {t.polls[plan]}
      </h3>
      <div className="mt-6 flex flex-col gap-2">
        {POLL_OPTIONS[plan].map((opt) => {
          const count = results[opt.id] || 0;
          const pct = total === 0 ? 0 : Math.round((count / total) * 100);
          const isChosen = chosen === opt.id;
          return (
            <button
              key={opt.id}
              type="button"
              onClick={() => onVote(plan, opt.id)}
              disabled={voted || busy}
              aria-pressed={isChosen}
              className={`group relative flex h-12 w-full items-center justify-between gap-3 overflow-hidden rounded-[8px] px-4 text-left transition-[box-shadow,opacity] duration-200 ${
                isChosen
                  ? "shadow-[0_0_0_1px_var(--accent)_inset,0_0_0_4px_var(--accent-soft)]"
                  : voted
                    ? "cursor-default opacity-60 shadow-[0_0_0_1px_var(--line)_inset]"
                    : "shadow-[0_0_0_1px_var(--line-strong)_inset] hover:shadow-[0_0_0_1px_var(--fg-subtle)_inset] disabled:cursor-wait"
              }`}
            >
              <span
                aria-hidden
                className={`absolute inset-y-0 left-0 transition-[width] duration-700 ease-[cubic-bezier(0.22,1,0.36,1)] ${
                  isChosen ? "bg-accent/20" : "bg-inset"
                }`}
                style={{ width: `${pct}%` }}
              />
              <span className="relative flex items-center gap-2 font-mono text-[15px] font-medium">
                <span className="num">{opt.label}</span>
                {isChosen && (
                  <span className="inline-flex items-center gap-1 font-sans text-[11px] font-medium text-accent-ink">
                    <Check size={13} aria-hidden />
                    {t.yourVote}
                  </span>
                )}
              </span>
              <span className="num relative font-mono text-xs text-muted">
                {t.votes(count)} · {pct}%
              </span>
            </button>
          );
        })}
      </div>
      <p className="num mt-4 font-mono text-xs text-subtle">{t.totalVotes(total)}</p>
    </section>
  );
}
