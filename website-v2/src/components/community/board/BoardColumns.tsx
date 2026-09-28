/** 看板视图:按视图的 Group-by 字段(缺省 Status)分列,横向滚动。 */
import { useMemo } from "react";
import { MessageSquare } from "lucide-react";
import type { Lang } from "@/i18n/config";
import { feedback } from "@/i18n/messages/feedback";
import { LabelChip, StateIcon, formatDate, timeAgo } from "../shared";
import {
  itemState,
  matchesFilter,
  matchesSearch,
  optionColor,
  type BoardColumn,
  type BoardData,
  type BoardItem,
  type ViewConfig,
} from "./model";

function Card({ item, lang, onOpen }: { item: BoardItem; lang: Lang; onOpen: (n: number) => void }) {
  return (
    <button type="button" className="cm-card spot" onClick={() => onOpen(item.issueNumber)}>
      <span className="flex items-start gap-2">
        <StateIcon state={itemState(item)} className="mt-0.5" />
        <span className="line-clamp-2 text-sm leading-snug text-fg">{item.title}</span>
      </span>
      {item.labels.length > 0 && (
        <span className="flex flex-wrap gap-1">
          {item.labels.map((l) => (
            <LabelChip key={l.name} name={l.name} color={l.color} />
          ))}
        </span>
      )}
      <span className="mono num flex items-center gap-3 text-[11px] text-subtle">
        <span>#{item.issueNumber}</span>
        <time dateTime={item.createdAt} title={formatDate(item.createdAt, lang)}>
          {timeAgo(item.createdAt, lang)}
        </time>
        {item.comments > 0 && (
          <span className="inline-flex items-center gap-1">
            <MessageSquare aria-hidden className="size-3" />
            {item.comments}
          </span>
        )}
      </span>
    </button>
  );
}

export default function BoardColumns({
  data,
  view,
  lang,
  query,
  onOpen,
}: {
  data: BoardData;
  /** 缺省时为无视图配置的回退看板:仅按关键字过滤。 */
  view?: ViewConfig;
  lang: Lang;
  query: string;
  onOpen: (n: number) => void;
}) {
  const t = feedback[lang].board;

  const columns = useMemo<BoardColumn[]>(() => {
    const keep = (item: BoardItem) => (!view || matchesFilter(view.filter, item)) && matchesSearch(item, query);
    const fieldName = view?.groupByField ?? "Status";
    const field = view ? data.singleSelectFields?.find((f) => f.name === fieldName) : undefined;

    if (!field) {
      const cols = data.noStatusItems.length
        ? [...data.columns, { id: "none", name: t.noStatus, color: "GRAY", items: data.noStatusItems }]
        : data.columns;
      return cols.map((c) => ({ ...c, items: c.items.filter(keep) }));
    }

    const items = data.allItems.filter(keep);
    const result: BoardColumn[] = field.options.map((opt) => ({
      id: opt.id,
      name: opt.name,
      color: opt.color,
      items: items.filter((i) => i.fieldValues?.[fieldName]?.optionId === opt.id),
    }));
    const unassigned = items.filter((i) => !i.fieldValues?.[fieldName]);
    if (unassigned.length) result.unshift({ id: "__none__", name: t.noStatus, color: "GRAY", items: unassigned });
    return result;
  }, [data, view, query, t.noStatus]);

  return (
    <div className="cm-columns overflow-x-auto pb-2">
      <div className="flex w-max gap-px bg-line">
        {columns.map((col) => (
          <section key={col.id} aria-label={col.name} className="flex w-[min(78vw,288px)] flex-col bg-bg">
            <header className="flex h-11 items-center gap-2 border-b border-line px-4">
              <span className="size-2 rounded-full" style={{ background: optionColor(col.color) }} />
              <h3 className="flex-1 truncate text-sm font-medium text-fg">{col.name}</h3>
              <span className="mono num text-xs text-subtle">{col.items.length}</span>
            </header>
            <div className="flex flex-1 flex-col gap-2 p-3">
              {col.items.length === 0 ? (
                <p className="cm-dashed py-6 text-center text-xs text-subtle">{t.empty}</p>
              ) : (
                col.items.map((item) => <Card key={item.id} item={item} lang={lang} onOpen={onOpen} />)
              )}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
