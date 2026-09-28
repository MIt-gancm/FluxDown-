/**
 * 社区功能投票:GET/POST /api/feature-vote。
 * - 投票 / 取消:{ action: "vote" | "unvote", featureId },乐观更新,失败回滚;已投记录存 localStorage。
 * - 提案:{ action: "propose", title, description } → { featureId },刷新后跳到新提案所在页。
 */
import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { Calendar, ChevronLeft, ChevronRight, MessageSquare, Plus, ThumbsUp } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import { roadmap } from "@/i18n/messages/roadmap";
import { cn } from "@/lib/utils";
import { Notice, Placeholder, Spinner, formatDate } from "./shared";

interface Feature {
  id: number;
  title: string;
  description: string;
  createdAt: string;
  votes: number;
  comments: number;
}

interface FeatureList {
  features: Feature[];
  totalVotes: number;
}

const STORAGE_KEY = "fluxdown-feature-votes";
const PAGE_SIZE = 10;

/** 页码窗口:页数少时全列,否则 1 … 当前±1 … 末页(-1 表示省略号)。 */
function pageWindow(current: number, total: number): number[] {
  if (total <= 7) return Array.from({ length: total }, (_, i) => i + 1);
  const pages = [...new Set([1, total, current - 1, current, current + 1])]
    .filter((p) => p >= 1 && p <= total)
    .sort((a, b) => a - b);
  return pages.flatMap((p, i) => (i > 0 && p - pages[i - 1] > 1 ? [-1, p] : [p]));
}

function loadVoted(): Set<number> {
  try {
    const arr: unknown = JSON.parse(localStorage.getItem(STORAGE_KEY) ?? "[]");
    if (Array.isArray(arr)) return new Set(arr.filter((n): n is number => typeof n === "number"));
  } catch {
    // localStorage 不可用或内容损坏
  }
  return new Set();
}

function saveVoted(ids: Set<number>) {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify([...ids]));
  } catch {
    // localStorage 不可用
  }
}

function withDelta(list: FeatureList, id: number, delta: number): FeatureList {
  return {
    totalVotes: list.totalVotes + delta,
    features: list.features.map((f) => (f.id === id ? { ...f, votes: Math.max(0, f.votes + delta) } : f)),
  };
}

export default function FeatureVote({ lang, onOpen }: { lang: Lang; onOpen: (n: number) => void }) {
  const t = roadmap[lang].vote;
  const replies = feedback[lang].issue.replies;
  const [data, setData] = useState<FeatureList | null>(null);
  const [loadError, setLoadError] = useState(false);
  const [voted, setVoted] = useState<Set<number>>(() => new Set());
  const [pendingId, setPendingId] = useState<number | null>(null);
  const [flash, setFlash] = useState<{ text: string; tone: "ok" | "error" } | null>(null);
  const [proposeOpen, setProposeOpen] = useState(false);
  const [title, setTitle] = useState("");
  const [desc, setDesc] = useState("");
  const [proposing, setProposing] = useState(false);
  const [newId, setNewId] = useState<number | null>(null);
  const [page, setPage] = useState(1);
  const flashTimer = useRef<number>(undefined);
  const listRef = useRef<HTMLOListElement>(null);

  const showFlash = useCallback((text: string, tone: "ok" | "error") => {
    setFlash({ text, tone });
    window.clearTimeout(flashTimer.current);
    flashTimer.current = window.setTimeout(() => setFlash(null), 4000);
  }, []);

  const refetch = useCallback(async (bust = false) => {
    const res = await fetch(bust ? `/api/feature-vote?t=${Date.now()}` : "/api/feature-vote");
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    const fresh = (await res.json()) as FeatureList;
    setData(fresh);
    return fresh;
  }, []);

  useEffect(() => {
    setVoted(loadVoted());
    refetch().catch(() => setLoadError(true));
    return () => window.clearTimeout(flashTimer.current);
  }, [refetch]);

  const goPage = (p: number) => {
    setPage(p);
    listRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  const toggleVote = async (feature: Feature) => {
    if (pendingId !== null) return;
    const had = voted.has(feature.id);
    const delta = had ? -1 : 1;
    const flip = (ids: Set<number>, on: boolean) => {
      const next = new Set(ids);
      if (on) next.add(feature.id);
      else next.delete(feature.id);
      saveVoted(next);
      return next;
    };

    setPendingId(feature.id);
    setData((prev) => (prev ? withDelta(prev, feature.id, delta) : prev));
    setVoted((ids) => flip(ids, !had));
    let ok = false;
    try {
      const res = await fetch("/api/feature-vote", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: had ? "unvote" : "vote", featureId: feature.id }),
      });
      ok = res.ok;
      if (!ok) showFlash(res.status === 429 ? t.rateLimited : t.voteError, "error");
    } catch {
      showFlash(t.voteError, "error");
    }
    if (ok) {
      showFlash(had ? t.unvoteSuccess : t.voteSuccess, "ok");
      // POST 已使服务端缓存失效,拉取权威票数
      refetch(true).catch(() => {});
    } else {
      setData((prev) => (prev ? withDelta(prev, feature.id, -delta) : prev));
      setVoted((ids) => flip(ids, had));
    }
    setPendingId(null);
  };

  const propose = async () => {
    const trimmed = title.trim();
    if (!trimmed || proposing) return;
    setProposing(true);
    try {
      const res = await fetch("/api/feature-vote", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action: "propose", title: trimmed, description: desc.trim() }),
      });
      if (!res.ok) {
        showFlash(res.status === 429 ? t.rateLimited : t.proposeError, "error");
        return;
      }
      const { featureId } = (await res.json()) as { featureId: number };
      showFlash(t.proposeSuccess, "ok");
      setTitle("");
      setDesc("");
      setProposeOpen(false);
      setNewId(featureId);
      const fresh = await refetch(true).catch(() => null);
      const idx = fresh?.features.findIndex((f) => f.id === featureId) ?? -1;
      if (idx >= 0) goPage(Math.floor(idx / PAGE_SIZE) + 1);
    } catch {
      showFlash(t.proposeError, "error");
    } finally {
      setProposing(false);
    }
  };

  const maxVotes = data ? Math.max(1, ...data.features.map((f) => f.votes)) : 1;
  const totalPages = data ? Math.max(1, Math.ceil(data.features.length / PAGE_SIZE)) : 1;
  // 列表缩短时夹紧页码,而不是把读者踢回第一页
  const safePage = Math.min(page, totalPages);
  const start = (safePage - 1) * PAGE_SIZE;

  return (
    <div className="flex flex-col gap-6">
      {proposeOpen ? (
        <form
          className="fig"
          onSubmit={(e) => {
            e.preventDefault();
            propose();
          }}
        >
          <div className="fig-bar">
            <span>new</span>
            <span>{t.proposeButton}</span>
          </div>
          <div className="flex flex-col gap-3 p-5">
            <label htmlFor="fv-title" className="sr-only">
              {t.proposeTitleLabel}
            </label>
            <input
              id="fv-title"
              type="text"
              required
              autoFocus
              maxLength={80}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
              placeholder={t.proposeTitlePlaceholder}
              className="field"
            />
            <label htmlFor="fv-desc" className="sr-only">
              {t.proposeDescLabel}
            </label>
            <textarea
              id="fv-desc"
              rows={3}
              maxLength={1000}
              value={desc}
              onChange={(e) => setDesc(e.target.value)}
              placeholder={t.proposeDescPlaceholder}
              className="field resize-y"
            />
            <div className="flex items-center justify-end gap-2">
              <button type="button" className="btn btn-ghost btn-sm" onClick={() => setProposeOpen(false)}>
                {t.proposeCancel}
              </button>
              <button type="submit" className="btn btn-primary btn-sm" disabled={!title.trim() || proposing}>
                {proposing && <Spinner />}
                {proposing ? t.proposing : t.proposeSubmit}
              </button>
            </div>
          </div>
        </form>
      ) : (
        <button type="button" className="cm-dashed cm-propose" onClick={() => setProposeOpen(true)}>
          <Plus aria-hidden className="size-4" />
          {t.proposeButton}
        </button>
      )}

      <div aria-live="polite" className="min-h-5 text-center">
        {flash && <Notice tone={flash.tone}>{flash.text}</Notice>}
      </div>

      {loadError ? (
        <Placeholder tone="error">{t.loadError}</Placeholder>
      ) : !data ? (
        <Placeholder tone="loading">{t.loading}</Placeholder>
      ) : data.features.length === 0 ? (
        <Placeholder>{t.empty}</Placeholder>
      ) : (
        <>
          <ol ref={listRef} className="cells scroll-mt-28 border border-line" start={start + 1}>
            {data.features.slice(start, start + PAGE_SIZE).map((f, i) => {
              const rank = start + i + 1;
              const isVoted = voted.has(f.id);
              const pct = f.votes > 0 ? Math.max(Math.round((f.votes / maxVotes) * 100), 4) : 0;
              return (
                <li
                  key={f.id}
                  className={cn("cm-feature spot", newId === f.id && "is-new")}
                  style={{ "--pct": `${pct}%` } as CSSProperties}
                >
                  <span className={cn("cm-rank mono num", rank <= 3 && f.votes > 0 && "is-top")}>
                    <span className="sr-only">{t.rank} </span>
                    {String(rank).padStart(2, "0")}
                  </span>
                  <button type="button" className="min-w-0 flex-1 text-left" onClick={() => onOpen(f.id)}>
                    <span className="block truncate text-[15px] font-medium text-fg">{f.title}</span>
                    {f.description && (
                      <span className="mt-1 line-clamp-2 text-[13px] leading-relaxed text-muted">{f.description}</span>
                    )}
                    <span className="mono num mt-2 flex items-center gap-3 text-[11px] text-subtle">
                      <span className="inline-flex items-center gap-1">
                        <Calendar aria-hidden className="size-3" />
                        {formatDate(f.createdAt, lang)}
                      </span>
                      <span className="text-subtle">#{f.id}</span>
                      {f.comments > 0 && (
                        <span className="inline-flex items-center gap-1">
                          <MessageSquare aria-hidden className="size-3" />
                          {replies(f.comments)}
                        </span>
                      )}
                    </span>
                  </button>
                  <button
                    type="button"
                    className={cn("cm-vote mono num", isVoted && "is-on")}
                    aria-pressed={isVoted}
                    aria-label={t.voteFor(f.title)}
                    disabled={pendingId === f.id}
                    onClick={() => toggleVote(f)}
                  >
                    <ThumbsUp aria-hidden className="size-3.5" fill={isVoted ? "currentColor" : "none"} />
                    {f.votes}
                  </button>
                </li>
              );
            })}
          </ol>

          {totalPages > 1 && (
            <nav aria-label={t.pagination} className="flex items-center justify-center gap-1.5">
              <button
                type="button"
                className="cm-page"
                aria-label={t.prevPage}
                disabled={safePage <= 1}
                onClick={() => goPage(safePage - 1)}
              >
                <ChevronLeft aria-hidden className="size-4" />
              </button>
              {pageWindow(safePage, totalPages).map((p, i) =>
                p === -1 ? (
                  <span key={`gap-${i}`} aria-hidden className="w-6 text-center text-sm text-subtle">
                    …
                  </span>
                ) : (
                  <button
                    key={p}
                    type="button"
                    className={cn("cm-page mono num", p === safePage && "is-active")}
                    aria-label={t.page(p)}
                    aria-current={p === safePage ? "page" : undefined}
                    onClick={() => goPage(p)}
                  >
                    {p}
                  </button>
                ),
              )}
              <button
                type="button"
                className="cm-page"
                aria-label={t.nextPage}
                disabled={safePage >= totalPages}
                onClick={() => goPage(safePage + 1)}
              >
                <ChevronRight aria-hidden className="size-4" />
              </button>
            </nav>
          )}

          <p className="mono num text-center text-xs tracking-wide text-subtle uppercase">
            {t.stats(data.features.length, data.totalVotes)}
          </p>
        </>
      )}
    </div>
  );
}
