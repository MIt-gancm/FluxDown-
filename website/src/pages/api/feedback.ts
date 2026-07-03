/**
 * POST /api/feedback
 *
 * 接收用户反馈/功能建议，代理创建 GitHub Issue 到私有仓库。
 * 服务端持有 GITHUB_TOKEN，前端无需暴露凭据。
 *
 * 请求体:
 * {
 *   type: "feature" | "bug" | "other" | "docs",
 *   title: string,
 *   description: string,
 *   contact?: string     // 可选的联系方式（邮箱等）
 *   pagePath?: string    // 可选，docs 反馈关联的页面路径（需以 /docs/ 开头且 ≤200 字符，校验失败则静默忽略）
 * }
 *
 * 防滥用:
 * - 基于 IP 的简易速率限制（每 IP 每分钟最多 3 次）
 * - 内容长度限制
 */

import type { APIRoute } from "astro";
import { GITHUB_TOKEN, GITHUB_REPO } from "astro:env/server";

export const prerender = false;

// ---------- Rate Limit（内存，Vercel Serverless 冷启动后重置） ----------

const rateLimitMap = new Map<string, { count: number; resetAt: number }>();
const RATE_LIMIT_WINDOW = 60_000; // 1 分钟
const RATE_LIMIT_MAX = 3; // 每窗口最多 3 次

function isRateLimited(ip: string): boolean {
  const now = Date.now();
  const entry = rateLimitMap.get(ip);

  if (!entry || now > entry.resetAt) {
    rateLimitMap.set(ip, { count: 1, resetAt: now + RATE_LIMIT_WINDOW });
    return false;
  }

  entry.count += 1;
  return entry.count > RATE_LIMIT_MAX;
}

// 定期清理过期条目（防内存泄漏）
setInterval(() => {
  const now = Date.now();
  for (const [ip, entry] of rateLimitMap) {
    if (now > entry.resetAt) rateLimitMap.delete(ip);
  }
}, 5 * 60_000);

// ---------- 类型映射 ----------

const TYPE_LABELS: Record<string, string> = {
  feature: "enhancement",
  bug: "bug",
  other: "feedback",
  docs: "docs",
};

const TYPE_EMOJI: Record<string, string> = {
  feature: "\u2728", // ✨
  bug: "\uD83D\uDC1B", // 🐛
  other: "\uD83D\uDCAC", // 💬
  docs: "\uD83D\uDCD6", // 📖
};

// ---------- Handler ----------

export const POST: APIRoute = async ({ request, clientAddress }) => {
  // Astro SSR 的 clientAddress 由适配器（Vercel）从底层正确解析
  const ip = clientAddress || "unknown";

  // 速率限制
  if (isRateLimited(ip)) {
    return new Response(
      JSON.stringify({ error: "Too many requests. Please try again later." }),
      { status: 429, headers: { "Content-Type": "application/json" } },
    );
  }

  // Token 检查
  if (!GITHUB_TOKEN) {
    return new Response(
      JSON.stringify({ error: "Server misconfigured: missing GITHUB_TOKEN" }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }

  // 解析请求体
  let body: {
    type?: string;
    title?: string;
    description?: string;
    contact?: string;
    pagePath?: string;
  };

  try {
    body = await request.json();
  } catch {
    return new Response(JSON.stringify({ error: "Invalid JSON body" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  const { type, title, description, contact, pagePath } = body;

  // 验证必填字段
  if (!type || !title || !description) {
    return new Response(
      JSON.stringify({
        error: "Missing required fields: type, title, description",
      }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  // 验证 type 取值
  if (!["feature", "bug", "other", "docs"].includes(type)) {
    return new Response(
      JSON.stringify({
        error: "Invalid type. Must be: feature, bug, other, or docs",
      }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  // 内容长度限制
  if (title.length > 200) {
    return new Response(
      JSON.stringify({ error: "Title too long (max 200 characters)" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  if (description.length > 5000) {
    return new Response(
      JSON.stringify({ error: "Description too long (max 5000 characters)" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  // 可选字段：pagePath（docs 反馈关联的页面路径）。校验失败静默忽略而非 400，
  // 因为它只是补充上下文，不应阻塞用户提交反馈本身。
  const safePagePath =
    typeof pagePath === "string" &&
    pagePath.startsWith("/docs/") &&
    pagePath.length <= 200
      ? pagePath
      : undefined;

  // 构造 Issue 内容
  const emoji = TYPE_EMOJI[type] || "\uD83D\uDCAC";
  const label = TYPE_LABELS[type] || "feedback";

  const issueTitle = `${emoji} [Website Feedback] ${title}`;
  const issueBody = [
    `## ${type === "feature" ? "Feature Request" : type === "bug" ? "Bug Report" : "Feedback"}`,
    "",
    description,
    "",
    "---",
    "",
    `**Type:** ${type}`,
    safePagePath ? `**页面**: ${safePagePath}` : null,
    contact ? `**Contact:** ${contact}` : null,
    `**Source:** Website feedback form`,
    `**Submitted:** ${new Date().toISOString()}`,
    `**IP:** \`${ip}\``,
  ]
    .filter(Boolean)
    .join("\n");

  try {
    const res = await fetch(
      `https://api.github.com/repos/${GITHUB_REPO}/issues`,
      {
        method: "POST",
        headers: {
          Authorization: `Bearer ${GITHUB_TOKEN}`,
          Accept: "application/vnd.github+json",
          "X-GitHub-Api-Version": "2022-11-28",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          title: issueTitle,
          body: issueBody,
          labels: ["user-feedback", label],
        }),
      },
    );

    if (!res.ok) {
      const text = await res.text();
      console.error(`GitHub API error: ${res.status}`, text);
      return new Response(
        JSON.stringify({ error: "Failed to submit feedback" }),
        { status: 502, headers: { "Content-Type": "application/json" } },
      );
    }

    const issue = await res.json();

    return new Response(
      JSON.stringify({
        success: true,
        message: "Feedback submitted successfully",
        issueNumber: issue.number,
      }),
      {
        status: 201,
        headers: { "Content-Type": "application/json" },
      },
    );
  } catch (err) {
    console.error("Failed to create GitHub issue:", err);
    return new Response(JSON.stringify({ error: "Internal server error" }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }
};
