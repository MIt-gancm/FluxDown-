/**
 * GET /api/issues?state=open&label=enhancement&page=1&per_page=15
 *
 * 获取 user-feedback 标签的 GitHub Issues 列表（分页）。
 * 排除带有 subscription 标签的 Issue。
 *
 * Query params:
 *   state    - "open" | "closed" | "all"，默认 "all"
 *   label    - 额外筛选标签（enhancement / bug / feedback），可选
 *   page     - 页码，从 1 开始，默认 1
 *   per_page - 每页条数，默认 15，最大 50
 *
 * 返回格式:
 * {
 *   issues: [ { number, title, state, labels, created_at, updated_at, comments, user, body_preview } ],
 *   page, per_page, has_more, total_shown
 * }
 */

import type { APIRoute } from "astro";

export const prerender = false;

const GITHUB_REPO = import.meta.env.GITHUB_REPO || "user/x_down";
const GITHUB_TOKEN = import.meta.env.GITHUB_TOKEN || "";

// ── 缓存 ──
interface CacheEntry {
  data: FilteredIssue[];
  timestamp: number;
}

const cache = new Map<string, CacheEntry>();
const CACHE_TTL = 120_000; // 2 分钟

// ── 类型 ──

interface GitHubLabel {
  id: number;
  name: string;
  color: string;
  description: string | null;
}

interface GitHubUser {
  login: string;
  avatar_url: string;
  html_url: string;
}

interface GitHubIssue {
  number: number;
  title: string;
  state: string;
  state_reason: string | null; // "completed" | "not_planned" | "reopened" | null
  labels: GitHubLabel[];
  created_at: string;
  updated_at: string;
  comments: number;
  user: GitHubUser;
  body: string | null;
  pull_request?: unknown;
}

/**
 * 归一化的关闭原因:
 *   "completed"   - 已完成（Close as completed）
 *   "not_planned" - 不采纳（Close as not planned）
 *   "duplicate"   - 重复项（有 duplicate 标签）
 *   null          - 未关闭 / 未知
 */
type CloseReason = "completed" | "not_planned" | "duplicate" | null;

interface FilteredIssue {
  number: number;
  title: string;
  state: string;
  close_reason: CloseReason;
  labels: { name: string; color: string }[];
  created_at: string;
  updated_at: string;
  comments: number;
  user: { login: string; avatar_url: string };
  body_preview: string;
}

/** 推导关闭原因：优先检测 duplicate 标签，再看 state_reason */
function deriveCloseReason(issue: GitHubIssue): CloseReason {
  if (issue.state !== "closed") return null;
  // duplicate 标签优先级最高
  if (issue.labels.some((l) => l.name.toLowerCase() === "duplicate")) return "duplicate";
  if (issue.state_reason === "completed") return "completed";
  if (issue.state_reason === "not_planned") return "not_planned";
  // 默认按 completed 处理（老版本 API 可能没有 state_reason）
  return "completed";
}

/** 解析 GitHub Link header 中的 next URL */
function parseLinkNext(header: string | null): string | null {
  if (!header) return null;
  const match = header.match(/<([^>]+)>;\s*rel="next"/);
  return match ? match[1] : null;
}

/** 拉取 GitHub Issues（自动分页），只获取带 user-feedback 标签的 */
async function fetchAllFeedbackIssues(state: string): Promise<GitHubIssue[]> {
  const all: GitHubIssue[] = [];
  // GitHub API: labels 参数筛选带 user-feedback 的 issue
  let url: string | null =
    `https://api.github.com/repos/${GITHUB_REPO}/issues?labels=user-feedback&state=${state}&per_page=100&sort=created&direction=desc`;

  while (url) {
    const res = await fetch(url, {
      headers: {
        Authorization: `Bearer ${GITHUB_TOKEN}`,
        Accept: "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
      },
    });

    if (!res.ok) {
      throw new Error(`GitHub API ${res.status}: ${await res.text()}`);
    }

    const page: GitHubIssue[] = await res.json();
    all.push(...page);

    url = parseLinkNext(res.headers.get("Link"));
  }

  return all;
}

/**
 * 从 feedback body 中提取纯描述文本（去掉 ## 标题 和 --- 之后的元数据区）。
 * Body 格式:
 *   ## Feature Request\n\n描述内容\n\n---\n\n**Type:** ...\n**IP:** ...
 */
function extractDescription(body: string | null): string {
  if (!body) return "";
  // 以独立的 --- 分割线为界，取前半部分
  const sepIdx = body.indexOf("\n---\n");
  const content = sepIdx >= 0 ? body.slice(0, sepIdx) : body;
  // 移除开头的 ## 标题行
  return content.replace(/^##\s+.*$/gm, "").trim();
}

/** 截取描述前 N 字符作为预览 */
function truncateBody(body: string | null, maxLen: number = 200): string {
  const desc = extractDescription(body);
  if (!desc) return "";
  if (desc.length <= maxLen) return desc;
  return desc.slice(0, maxLen) + "...";
}

/** 获取经过过滤的 issue 列表（带缓存） */
async function getCachedIssues(state: string): Promise<FilteredIssue[]> {
  const cacheKey = `issues_${state}`;
  const cached = cache.get(cacheKey);

  if (cached && Date.now() - cached.timestamp < CACHE_TTL) {
    return cached.data;
  }

  const raw = await fetchAllFeedbackIssues(state);

  const filtered: FilteredIssue[] = raw
    // 排除 PR（GitHub Issues API 也会返回 PR）
    .filter((issue) => !issue.pull_request)
    // 排除带 subscription 标签的
    .filter((issue) => !issue.labels.some((l) => l.name === "subscription"))
    .map((issue) => ({
      number: issue.number,
      title: issue.title
        // 移除 emoji 前缀如 "✨ [Website Feedback] "
        .replace(/^[\u{1F300}-\u{1FAF6}\u{2600}-\u{27BF}\u{FE00}-\u{FE0F}\u{200D}\u{20E3}\u{E0020}-\u{E007F}]+\s*/u, "")
        .replace(/^\[Website Feedback\]\s*/i, ""),
      state: issue.state,
      close_reason: deriveCloseReason(issue),
      labels: issue.labels
        .filter((l) => l.name !== "user-feedback" && l.name.toLowerCase() !== "duplicate")
        .map((l) => ({ name: l.name, color: l.color })),
      created_at: issue.created_at,
      updated_at: issue.updated_at,
      comments: issue.comments,
      user: {
        login: issue.user.login,
        avatar_url: issue.user.avatar_url,
      },
      body_preview: truncateBody(issue.body),
    }));

  cache.set(cacheKey, { data: filtered, timestamp: Date.now() });
  return filtered;
}

export const GET: APIRoute = async ({ url }) => {
  const stateParam = url.searchParams.get("state")?.trim() || "all";
  const labelParam = url.searchParams.get("label")?.trim() || "";
  const page = Math.max(1, parseInt(url.searchParams.get("page") || "1", 10) || 1);
  const perPage = Math.min(
    50,
    Math.max(1, parseInt(url.searchParams.get("per_page") || "15", 10) || 15),
  );

  // 验证 state
  if (!["open", "closed", "all"].includes(stateParam)) {
    return new Response(
      JSON.stringify({ error: "Invalid state. Must be: open, closed, or all" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  if (!GITHUB_TOKEN) {
    return new Response(
      JSON.stringify({ error: "Server misconfigured: missing GITHUB_TOKEN" }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }

  try {
    let issues = await getCachedIssues(stateParam);

    // 额外标签筛选
    if (labelParam) {
      issues = issues.filter((issue) =>
        issue.labels.some((l) => l.name === labelParam),
      );
    }

    const start = (page - 1) * perPage;
    const sliced = issues.slice(start, start + perPage);

    return new Response(
      JSON.stringify({
        issues: sliced,
        page,
        per_page: perPage,
        has_more: start + perPage < issues.length,
        total_shown: issues.length,
      }),
      {
        status: 200,
        headers: {
          "Content-Type": "application/json",
          "Cache-Control": "public, s-maxage=120, stale-while-revalidate=300",
        },
      },
    );
  } catch (err) {
    console.error("Failed to fetch issues:", err);
    return new Response(
      JSON.stringify({ error: "Failed to fetch issues", detail: String(err) }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }
};
