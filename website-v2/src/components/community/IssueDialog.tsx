/**
 * Issue 详情对话框(原生 <dialog>:top layer、Esc 关闭、焦点陷阱)。
 * 数据:GET /api/issues/:n;回复:POST /api/issues/:n/comments { body }(仅 open 状态可回复)。
 */
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import {
  Annoyed,
  Calendar,
  Clock,
  Eye,
  Heart,
  Laugh,
  Mail,
  MessageSquare,
  PartyPopper,
  Rocket,
  Send,
  ThumbsDown,
  ThumbsUp,
  User,
  X,
  type LucideIcon,
} from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import { cn } from "@/lib/utils";
import { renderMarkdown } from "./markdown";
import { LabelChip, Notice, Placeholder, Spinner, StateBadge, formatDate, timeAgo, type IssueState } from "./shared";
import { withBase } from "@/lib/base";

interface Reactions {
  "+1": number;
  "-1": number;
  laugh: number;
  hooray: number;
  confused: number;
  heart: number;
  rocket: number;
  eyes: number;
}

interface IssueData {
  number: number;
  title: string;
  state: string;
  close_reason: "completed" | "not_planned" | "duplicate" | null;
  labels: { name: string; color: string }[];
  created_at: string;
  comments_count: number;
  user: { login: string };
  description: string;
  body_raw: string;
  metadata: { type: string | null; contact: string | null; submitted_at: string | null } | null;
  is_feedback_format: boolean;
  reactions: Reactions;
}

interface CommentData {
  id: number;
  user: { login: string };
  body: string;
  created_at: string;
  reactions: Reactions;
}

interface IssueDetail {
  issue: IssueData;
  comments: CommentData[];
}

const REACTIONS: [keyof Reactions, LucideIcon][] = [
  ["+1", ThumbsUp],
  ["heart", Heart],
  ["rocket", Rocket],
  ["eyes", Eye],
  ["hooray", PartyPopper],
  ["laugh", Laugh],
  ["confused", Annoyed],
  ["-1", ThumbsDown],
];

const REPLY_MAX = 2000;

function ReactionsBar({ reactions }: { reactions: Reactions }) {
  const visible = REACTIONS.filter(([key]) => reactions[key] > 0);
  if (visible.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 px-4 pb-3">
      {visible.map(([key, Icon]) => (
        <span key={key} className="chip num h-6 gap-1 px-2 text-[11px]">
          <Icon aria-hidden className="size-3" />
          {reactions[key]}
        </span>
      ))}
    </div>
  );
}

function Post({
  who,
  developer,
  date,
  body,
  lang,
  children,
}: {
  who: string;
  developer?: boolean;
  date: string;
  body: string;
  lang: Lang;
  children?: ReactNode;
}) {
  return (
    <article className={cn("border border-line bg-elev", developer && "cm-post-dev")}>
      <header className="flex items-center gap-2 border-b border-line bg-sunken px-4 py-2">
        <span
          className={cn(
            "grid size-5 place-items-center rounded-full",
            developer ? "bg-accent-soft text-accent-ink" : "bg-inset text-subtle",
          )}
        >
          <User aria-hidden className="size-3" />
        </span>
        <span className={cn("text-xs font-medium", developer ? "text-accent-ink" : "text-fg")}>{who}</span>
        <time dateTime={date} title={formatDate(date, lang, true)} className="mono text-[11px] text-subtle">
          {timeAgo(date, lang)}
        </time>
      </header>
      <div className="cm-md px-4 py-3" dangerouslySetInnerHTML={{ __html: renderMarkdown(body) }} />
      {children}
    </article>
  );
}

export default function IssueDialog({
  lang,
  issueNumber,
  onClose,
}: {
  lang: Lang;
  issueNumber: number | null;
  onClose: () => void;
}) {
  const t = feedback[lang];
  const dialogRef = useRef<HTMLDialogElement>(null);
  const [data, setData] = useState<IssueDetail | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");
  const [reply, setReply] = useState("");
  const [replyStatus, setReplyStatus] = useState<"idle" | "sending" | "success" | "error">("idle");
  const [replyError, setReplyError] = useState("");

  const fetchDetail = useCallback(
    async (num: number, quiet = false) => {
      if (!quiet) {
        setLoading(true);
        setData(null);
      }
      setError("");
      try {
        const res = await fetch(withBase(`/api/issues/${num}`));
        if (res.status === 404) {
          setError(t.issue.notFound);
          return;
        }
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        setData((await res.json()) as IssueDetail);
      } catch {
        setError(t.issue.error);
      } finally {
        setLoading(false);
      }
    },
    [t],
  );

  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;
    setReply("");
    setReplyStatus("idle");
    setReplyError("");
    if (issueNumber === null) {
      if (dialog.open) dialog.close();
      return;
    }
    if (!dialog.open) dialog.showModal();
    fetchDetail(issueNumber);
  }, [issueNumber, fetchDetail]);

  const sendReply = useCallback(async () => {
    const body = reply.trim();
    if (!body || issueNumber === null || replyStatus === "sending") return;
    setReplyStatus("sending");
    setReplyError("");
    try {
      const res = await fetch(withBase(`/api/issues/${issueNumber}/comments`), {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ body }),
      });
      if (!res.ok) {
        setReplyStatus("error");
        setReplyError(res.status === 429 ? t.issue.replyRateLimited : t.issue.replyError);
        return;
      }
      setReplyStatus("success");
      setReply("");
      fetchDetail(issueNumber, true);
      setTimeout(() => setReplyStatus((s) => (s === "success" ? "idle" : s)), 3000);
    } catch {
      setReplyStatus("error");
      setReplyError(t.issue.replyError);
    }
  }, [reply, issueNumber, replyStatus, t, fetchDetail]);

  const issue = data?.issue;
  const state: IssueState = !issue || issue.state === "open" ? "open" : (issue.close_reason ?? "completed");
  const stateLabel = {
    open: t.issue.open,
    completed: t.issue.completed,
    not_planned: t.issue.notPlanned,
    duplicate: t.issue.duplicate,
  }[state];
  const labelName = (name: string) => t.labels[name as keyof typeof t.labels] ?? name;
  const metaType = issue?.metadata?.type
    ? (t.issue.metaType[issue.metadata.type as keyof typeof t.issue.metaType] ?? issue.metadata.type)
    : null;

  return (
    <dialog
      ref={dialogRef}
      className="cm-dialog"
      aria-labelledby="issue-dialog-title"
      onClose={onClose}
      onClick={(e) => {
        if (e.target === dialogRef.current) dialogRef.current.close();
      }}
    >
      <div className="cm-dialog-panel">
        <div className="flex h-11 shrink-0 items-center gap-3 border-b border-line bg-sunken px-4">
          <MessageSquare aria-hidden className="size-4 text-subtle" />
          <span className="mono num text-xs tracking-wide text-muted">#{issue?.number ?? issueNumber}</span>
          <button
            type="button"
            className="btn btn-ghost btn-sm ml-auto size-8 px-0"
            aria-label={t.issue.close}
            onClick={() => dialogRef.current?.close()}
          >
            <X aria-hidden className="size-4" />
          </button>
        </div>

        <div className="min-h-0 flex-1 overflow-y-auto px-5 py-5 sm:px-6">
          {loading && <Placeholder tone="loading">{t.issue.loading}</Placeholder>}
          {error && !loading && <Placeholder tone="error">{error}</Placeholder>}
          {issue && data && !loading && (
            <>
              <div className="mb-6 flex flex-col gap-3">
                <h2 id="issue-dialog-title" className="text-lg leading-snug font-semibold tracking-tight text-fg">
                  {issue.title}
                </h2>
                <div className="flex flex-wrap items-center gap-2">
                  <StateBadge state={state} label={stateLabel} />
                  {issue.labels.map((label) => (
                    <LabelChip key={label.name} name={labelName(label.name)} color={label.color} />
                  ))}
                </div>
                <div className="mono flex flex-wrap items-center gap-x-4 gap-y-1 text-[11px] text-subtle">
                  <span className="inline-flex items-center gap-1">
                    <Clock aria-hidden className="size-3" />
                    {formatDate(issue.created_at, lang, true)}
                  </span>
                  {issue.comments_count > 0 && (
                    <span className="num inline-flex items-center gap-1">
                      <MessageSquare aria-hidden className="size-3" />
                      {t.issue.replies(issue.comments_count)}
                    </span>
                  )}
                </div>
              </div>

              <Post
                who={t.issue.user}
                date={issue.created_at}
                body={issue.is_feedback_format ? issue.description : issue.body_raw}
                lang={lang}
              >
                {issue.is_feedback_format && issue.metadata && (
                  <div className="mono flex flex-wrap items-center gap-x-4 gap-y-1.5 border-t border-line px-4 py-2.5 text-[11px] text-subtle">
                    {metaType && (
                      <span>
                        <span className="text-muted">{t.issue.typeLabel}:</span> {metaType}
                      </span>
                    )}
                    {issue.metadata.contact && (
                      <span className="inline-flex items-center gap-1">
                        <Mail aria-hidden className="size-3" />
                        {issue.metadata.contact}
                      </span>
                    )}
                    {issue.metadata.submitted_at && (
                      <span className="inline-flex items-center gap-1">
                        <Calendar aria-hidden className="size-3" />
                        {formatDate(issue.metadata.submitted_at, lang, true)}
                      </span>
                    )}
                  </div>
                )}
                <ReactionsBar reactions={issue.reactions} />
              </Post>

              <div className="mt-6 flex flex-col gap-3">
                {data.comments.length > 0 ? (
                  <>
                    <h3 className="eyebrow-plain">{t.issue.replies(data.comments.length)}</h3>
                    <div className="cm-thread flex flex-col gap-3">
                      {data.comments.map((c) => {
                        const developer = c.user.login !== issue.user.login;
                        return (
                          <Post
                            key={c.id}
                            who={developer ? t.issue.developer : t.issue.user}
                            developer={developer}
                            date={c.created_at}
                            body={c.body}
                            lang={lang}
                          >
                            <ReactionsBar reactions={c.reactions} />
                          </Post>
                        );
                      })}
                    </div>
                  </>
                ) : (
                  <p className="py-4 text-center text-xs text-subtle">{t.issue.noComments}</p>
                )}
              </div>
            </>
          )}
        </div>

        {issue && !loading && issue.state === "open" && (
          <form
            className="shrink-0 border-t border-line bg-sunken px-5 py-4 sm:px-6"
            onSubmit={(e) => {
              e.preventDefault();
              sendReply();
            }}
          >
            <label htmlFor="issue-reply" className="sr-only">
              {t.issue.replyLabel}
            </label>
            <textarea
              id="issue-reply"
              rows={2}
              maxLength={REPLY_MAX}
              value={reply}
              onChange={(e) => setReply(e.target.value)}
              placeholder={t.issue.replyPlaceholder}
              className="field resize-none"
              onKeyDown={(e) => {
                if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
                  e.preventDefault();
                  sendReply();
                }
              }}
            />
            <div className="mt-2 flex flex-wrap items-center gap-3">
              <span className="mono num text-[11px] text-subtle">
                {reply.length}/{REPLY_MAX}
              </span>
              <span className="hidden items-center gap-1 text-[11px] text-subtle sm:inline-flex">
                <span className="kbd">⌘</span>
                <span className="kbd">↵</span>
                {t.issue.replyHint}
              </span>
              <div className="ml-auto flex items-center gap-3">
                {replyStatus === "success" && <Notice tone="ok">{t.issue.replySuccess}</Notice>}
                {replyStatus === "error" && <Notice tone="error">{replyError}</Notice>}
                <button
                  type="submit"
                  className="btn btn-primary btn-sm"
                  disabled={!reply.trim() || replyStatus === "sending"}
                >
                  {replyStatus === "sending" ? <Spinner /> : <Send aria-hidden className="size-3.5" />}
                  {replyStatus === "sending" ? t.issue.replySending : t.issue.replySend}
                </button>
              </div>
            </div>
          </form>
        )}
      </div>
    </dialog>
  );
}
