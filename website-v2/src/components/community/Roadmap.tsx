/** 作者维护的路线图:GET /api/roadmap(GitHub Issues + roadmap:* 标签),三列 计划中 → 实施中 → 已完成。 */
import { useEffect, useState, type CSSProperties } from "react";
import { CircleCheck, CircleDashed, Clock, LoaderCircle, MessageSquare, type LucideIcon } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import { roadmap } from "@/i18n/messages/roadmap";
import { LabelChip, Placeholder, formatDate } from "./shared";

type Status = "planned" | "in-progress" | "done";

interface RoadmapItem {
  id: number;
  title: string;
  description: string;
  status: Status;
  url: string;
  labels: { name: string; color: string }[];
  createdAt: string;
  updatedAt: string;
  comments: number;
}

interface RoadmapData {
  columns: { status: Status; items: RoadmapItem[] }[];
  counts: Record<Status, number>;
  total: number;
  updatedAt: string;
}

const STATUS: Record<Status, { key: "planned" | "inProgress" | "done"; icon: LucideIcon; color: string }> = {
  planned: { key: "planned", icon: CircleDashed, color: "var(--fg-subtle)" },
  "in-progress": { key: "inProgress", icon: LoaderCircle, color: "var(--warn)" },
  done: { key: "done", icon: CircleCheck, color: "var(--ok)" },
};

export default function Roadmap({ lang, onOpen }: { lang: Lang; onOpen: (n: number) => void }) {
  const t = roadmap[lang].board;
  const replies = feedback[lang].issue.replies;
  const [data, setData] = useState<RoadmapData | null>(null);
  const [error, setError] = useState(false);

  useEffect(() => {
    fetch("/api/roadmap")
      .then((r) => (r.ok ? (r.json() as Promise<RoadmapData>) : Promise.reject(r.status)))
      .then(setData)
      .catch(() => setError(true));
  }, []);

  if (error) return <Placeholder tone="error">{t.loadError}</Placeholder>;
  if (!data) return <Placeholder tone="loading">{t.loading}</Placeholder>;
  if (data.total === 0) return <Placeholder>{t.empty}</Placeholder>;

  return (
    <div className="cells lg:grid-cols-3">
      {data.columns.map((col) => {
        const meta = STATUS[col.status];
        const Icon = meta.icon;
        return (
          <section key={col.status} aria-labelledby={`roadmap-${col.status}`} className="flex flex-col">
            <header className="flex h-12 items-center gap-2.5 border-b border-line px-5" style={{ color: meta.color }}>
              <Icon aria-hidden className="size-4" />
              <h3 id={`roadmap-${col.status}`} className="mono text-xs font-medium tracking-[0.12em] text-fg uppercase">
                {t.status[meta.key]}
              </h3>
              <span className="mono num ml-auto text-xs">{String(col.items.length).padStart(2, "0")}</span>
            </header>
            <div className="flex flex-1 flex-col gap-2.5 p-4">
              {col.items.length === 0 ? (
                <p className="cm-dashed py-10 text-center text-xs text-subtle">{t.columnEmpty}</p>
              ) : (
                col.items.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    style={{ "--c": meta.color } as CSSProperties}
                    className="cm-card cm-card-accent spot"
                    onClick={() => onOpen(item.id)}
                  >
                    <span className="text-sm leading-snug font-medium text-fg">{item.title}</span>
                    {item.description && (
                      <span className="line-clamp-2 text-xs leading-relaxed text-muted">{item.description}</span>
                    )}
                    {item.labels.length > 0 && (
                      <span className="flex flex-wrap gap-1">
                        {item.labels.slice(0, 4).map((l) => (
                          <LabelChip key={l.name} name={l.name} color={l.color} />
                        ))}
                      </span>
                    )}
                    <span className="mono num flex items-center gap-3 text-[11px] text-subtle">
                      <span className="inline-flex items-center gap-1">
                        <Clock aria-hidden className="size-3" />
                        <span className="sr-only">{t.updated}</span>
                        {formatDate(item.updatedAt, lang)}
                      </span>
                      {item.comments > 0 && (
                        <span className="inline-flex items-center gap-1">
                          <MessageSquare aria-hidden className="size-3" />
                          {replies(item.comments)}
                        </span>
                      )}
                    </span>
                  </button>
                ))
              )}
            </div>
          </section>
        );
      })}
    </div>
  );
}
