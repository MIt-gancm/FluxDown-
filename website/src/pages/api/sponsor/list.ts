/**
 * GET /api/sponsor/list
 *
 * 从 GitHub 赞助名录 issue（SPONSOR_WALL_REPO#SPONSOR_WALL_ISSUE）的评论
 * 解析赞助者，返回「最新 20 位」并按 赞助时间降序 排列。
 *
 * 评论均由本站/迁移脚本生成，格式固定：
 *   ### [<img src="AVATAR" …>] 💖 NAME
 *   [> message…]
 *   `¥AMOUNT` · YYYY-MM-DD [· 来源]
 *
 * 缓存：内存 5 分钟 + Cache-Control，GitHub 失败时回退陈旧缓存。
 */

import type { APIRoute } from "astro";
import {
  GITHUB_TOKEN,
  SPONSOR_WALL_REPO,
  SPONSOR_WALL_ISSUE,
} from "astro:env/server";

import { getTokenOwnerLogin } from "@/lib/gh-owner";
import type { RecordComment } from "@/lib/record-filter";
import { parseSponsorComment, type ParsedSponsor, type WallSponsor } from "@/lib/sponsor-record";

export const prerender = false;

const JSON_HEADERS = {
  "Content-Type": "application/json",
  "Cache-Control": "public, max-age=300",
};

const LATEST_COUNT = 20;
const CACHE_TTL = 5 * 60_000;

export type { WallSponsor };

// ---------- Cache ----------

let cache: { sponsors: WallSponsor[]; expiry: number } | null = null;

async function fetchSponsors(): Promise<WallSponsor[]> {
  const all: ParsedSponsor[] = [];
  const owner = (await getTokenOwnerLogin()).toLowerCase();
  // 只信任 token 持有者（本站 / 迁移脚本）发出的评论；首行格式由 parseSponsorComment 校验。
  const trusted = (c: RecordComment) =>
    c.user?.login?.toLowerCase() === owner &&
    !(c.body ?? "").includes("Website visitor reply");
  // 名录量级很小；最多翻 3 页（300 条）足够覆盖「最新 20」。
  for (let page = 1; page <= 3; page += 1) {
    const res = await fetch(
      `https://api.github.com/repos/${SPONSOR_WALL_REPO}/issues/${SPONSOR_WALL_ISSUE}/comments?per_page=100&page=${page}`,
      {
        headers: {
          Authorization: `Bearer ${GITHUB_TOKEN}`,
          Accept: "application/vnd.github+json",
          "X-GitHub-Api-Version": "2022-11-28",
        },
      },
    );
    if (!res.ok) {
      throw new Error(`GitHub ${res.status}`);
    }
    const comments = (await res.json()) as (RecordComment & {
      created_at?: string;
    })[];
    for (const c of comments) {
      if (!trusted(c)) continue;
      const parsed = parseSponsorComment(c as { body?: string; created_at?: string });
      if (parsed) all.push(parsed);
    }
    if (comments.length < 100) break;
  }

  // 最新 20 位，按赞助时间降序展示（名副其实的「最新赞助」，不按金额排序）。
  return all
    .sort((a, b) => b.ts - a.ts)
    .slice(0, LATEST_COUNT)
    .map(({ ts: _ts, ...s }) => s);
}

export const GET: APIRoute = async () => {
  const now = Date.now();
  if (cache && cache.expiry > now) {
    return new Response(JSON.stringify({ sponsors: cache.sponsors }), {
      status: 200,
      headers: JSON_HEADERS,
    });
  }

  try {
    const sponsors = await fetchSponsors();
    cache = { sponsors, expiry: now + CACHE_TTL };
    return new Response(JSON.stringify({ sponsors }), {
      status: 200,
      headers: JSON_HEADERS,
    });
  } catch (e) {
    console.error("[sponsor/list] fetch failed:", e);
    // 陈旧缓存优于空白。
    return new Response(
      JSON.stringify({ sponsors: cache?.sponsors ?? [] }),
      { status: 200, headers: JSON_HEADERS },
    );
  }
};
