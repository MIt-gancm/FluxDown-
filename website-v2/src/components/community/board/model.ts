/** GET /api/project-board 的数据形状与客户端筛选工具(GitHub Projects v2 视图)。 */
import type { IssueState } from "../shared";

export interface ViewConfig {
  id: string;
  name: string;
  number: number;
  layout: "TABLE_LAYOUT" | "BOARD_LAYOUT";
  filter: string;
  visibleFields: string[];
  groupByField: string | null;
  hasGroupBy: boolean;
}

export interface BoardItem {
  id: string;
  issueNumber: number;
  title: string;
  state: "OPEN" | "CLOSED";
  stateReason: string | null;
  labels: { name: string; color: string }[];
  createdAt: string;
  url: string;
  comments: number;
  statusId: string | null;
  statusName: string | null;
  fieldValues: Record<string, { optionId: string; optionName: string }>;
}

export interface BoardColumn {
  id: string;
  name: string;
  color: string;
  items: BoardItem[];
}

export interface BoardData {
  views: ViewConfig[];
  columns: BoardColumn[];
  noStatusItems: BoardItem[];
  allItems: BoardItem[];
  totalItems: number;
  projectTitle: string;
  cachedAt: string;
  singleSelectFields?: { id: string; name: string; options: { id: string; name: string; color: string }[] }[];
}

/** GitHub Projects 选项色 → 站点色板(随明暗主题自动适配)。 */
const OPTION_COLORS: Record<string, string> = {
  GRAY: "var(--fg-subtle)",
  BLUE: "var(--accent)",
  GREEN: "var(--ok)",
  YELLOW: "var(--warn)",
  ORANGE: "oklch(0.7 0.17 50)",
  RED: "var(--danger)",
  PINK: "oklch(0.68 0.19 350)",
  PURPLE: "oklch(0.62 0.2 300)",
};

export const optionColor = (color: string) => OPTION_COLORS[color] ?? OPTION_COLORS.GRAY;

export function itemState(item: BoardItem): IssueState {
  if (item.state === "OPEN") return "open";
  return item.stateReason === "NOT_PLANNED" ? "not_planned" : "completed";
}

/** 标题或编号包含关键字。 */
export function matchesSearch(item: BoardItem, q: string): boolean {
  if (!q) return true;
  const lower = q.toLowerCase().replace(/^#/, "");
  return String(item.issueNumber).includes(lower) || item.title.toLowerCase().includes(lower);
}

/** 按 GitHub 视图 filter 字符串(`label:x is:open no:status status:y`)在客户端过滤。 */
export function matchesFilter(filter: string, item: BoardItem): boolean {
  if (!filter.trim()) return true;
  return filter
    .trim()
    .split(/\s+/)
    .every((part) => {
      const colon = part.indexOf(":");
      if (colon === -1) return true;
      const key = part.slice(0, colon).toLowerCase();
      const val = part.slice(colon + 1);
      switch (key) {
        case "label":
          return item.labels.some((l) => l.name === val);
        case "is":
          if (val === "open") return item.state === "OPEN";
          if (val === "closed") return item.state === "CLOSED";
          return true;
        case "no":
          return val === "status" ? item.statusId === null : true;
        case "status":
          return item.statusName !== null && item.statusName.toLowerCase() === val.toLowerCase();
        default:
          return true;
      }
    });
}
