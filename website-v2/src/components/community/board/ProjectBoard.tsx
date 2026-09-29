/**
 * 反馈追踪看板:GET /api/project-board(GitHub Projects v2)。
 * 视图标签 = 项目里配置的视图(表格 / 看板);无视图配置时回退为 Status 分列看板。
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { Filter, Kanban, Search, Table2, X } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import { Placeholder, Tabs } from "../shared";
import BoardColumns from "./BoardColumns";
import BoardTable from "./BoardTable";
import { matchesFilter, matchesSearch, type BoardData } from "./model";
import { withBase } from "@/lib/base";

export default function ProjectBoard({ lang, onOpen }: { lang: Lang; onOpen: (n: number) => void }) {
  const t = feedback[lang].board;
  const [data, setData] = useState<BoardData | null>(null);
  const [error, setError] = useState(false);
  const [viewId, setViewId] = useState("");
  const [input, setInput] = useState("");
  const [query, setQuery] = useState("");
  const timer = useRef<number>(undefined);

  useEffect(() => {
    fetch(withBase("/api/project-board"))
      .then((r) => (r.ok ? (r.json() as Promise<BoardData>) : Promise.reject(r.status)))
      .then(setData)
      .catch(() => setError(true));
    return () => window.clearTimeout(timer.current);
  }, []);

  const views = data?.views ?? [];
  const view = views.find((v) => v.id === viewId) ?? views[0];

  const count = useMemo(
    () => (data ? data.allItems.filter((i) => (!view || matchesFilter(view.filter, i)) && matchesSearch(i, query)).length : 0),
    [data, view, query],
  );

  if (error) return <Placeholder tone="error">{t.error}</Placeholder>;
  if (!data) return <Placeholder tone="loading">{t.loading}</Placeholder>;

  const onSearch = (value: string) => {
    setInput(value);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setQuery(value.trim()), 400);
  };

  return (
    <div>
      <div className="flex flex-col gap-3 border-b border-line px-[clamp(20px,4vw,56px)] py-4 lg:flex-row lg:items-center">
        {views.length > 0 && (
          <Tabs
            label={t.views}
            idPrefix="board-view"
            className="cm-tabs-line overflow-x-auto"
            active={view.id}
            onChange={setViewId}
            tabs={views.map((v) => ({ key: v.id, label: v.name, icon: v.layout === "TABLE_LAYOUT" ? Table2 : Kanban }))}
          />
        )}
        <div className="relative lg:ml-auto lg:w-80">
          <Search aria-hidden className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-subtle" />
          <input
            type="search"
            aria-label={t.searchLabel}
            value={input}
            onChange={(e) => onSearch(e.target.value)}
            placeholder={t.searchPlaceholder}
            className="field pr-9 pl-9"
          />
          {input && (
            <button
              type="button"
              aria-label={t.searchClear}
              className="absolute top-1/2 right-2 grid size-6 -translate-y-1/2 place-items-center rounded text-subtle hover:text-fg"
              onClick={() => {
                window.clearTimeout(timer.current);
                setInput("");
                setQuery("");
              }}
            >
              <X aria-hidden className="size-3.5" />
            </button>
          )}
        </div>
      </div>

      <div className="mono flex flex-wrap items-center gap-3 px-[clamp(20px,4vw,56px)] py-3 text-[11px] tracking-wide text-subtle uppercase">
        {view?.filter && (
          <span className="inline-flex items-center gap-1.5">
            <Filter aria-hidden className="size-3" />
            <span className="sr-only">{t.filter}</span>
            <code className="rounded-sm bg-sunken px-1.5 py-0.5 text-accent-ink normal-case">{view.filter}</code>
          </span>
        )}
        <span className="num" aria-live="polite">
          {t.items(count)}
        </span>
      </div>

      <div
        role={views.length ? "tabpanel" : undefined}
        id={view ? `board-view-panel-${view.id}` : undefined}
        aria-labelledby={view ? `board-view-tab-${view.id}` : undefined}
        className="border-t border-line"
      >
        {view?.layout === "TABLE_LAYOUT" ? (
          <BoardTable key={view.id} view={view} data={data} lang={lang} query={query} onOpen={onOpen} />
        ) : (
          <BoardColumns data={data} view={view} lang={lang} query={query} onOpen={onOpen} />
        )}
      </div>
    </div>
  );
}
