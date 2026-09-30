/**
 * 赞助名录评论（由本站 / 迁移脚本生成）的固定格式：
 *   ### [<img src="AVATAR" …>] 💖 NAME
 *   [> message…]
 *   `¥AMOUNT` · YYYY-MM-DD [· 来源]
 * 金额与日期只从最后一行解析，名称行里的内容无法伪造。
 */

export interface WallSponsor {
  name: string;
  avatar: string | null;
  amountCents: number;
  date: string; // YYYY-MM-DD (sponsor time, Asia/Shanghai)
  message: string | null;
}

export interface ParsedSponsor extends WallSponsor {
  ts: number; // epoch ms for ordering
}

const HEADING_RE = /^###\s+(?:<img[^>]*src="([^"]+)"[^>]*>\s*)?💖\s*(.+?)\s*$/;
const AMOUNT_RE = /`¥\s*([\d.]+)`/;
const DATE_RE = /(\d{4}-\d{2}-\d{2})/;

export function parseSponsorComment(c: {
  body?: string;
  created_at?: string;
}): ParsedSponsor | null {
  const lines = (c.body ?? "").replace(/\r\n?/g, "\n").trim().split("\n");
  const heading = lines[0]?.match(HEADING_RE);
  if (!heading) return null;

  const name = (heading[2] ?? "").replace(/\u200b/g, "").trim();
  if (!name) return null;

  const footer = lines.length > 1 ? lines[lines.length - 1]! : "";
  const amountRaw = footer.match(AMOUNT_RE)?.[1];
  const amountCents = amountRaw ? Math.round(parseFloat(amountRaw) * 100) : 0;

  const message =
    lines
      .slice(1, -1)
      .filter((l) => /^>\s?/.test(l))
      .map((l) => l.replace(/^>\s?/, ""))
      .join("\n")
      .trim() || null;

  const createdAt = c.created_at ?? "";
  const date = footer.match(DATE_RE)?.[1] ?? createdAt.slice(0, 10);
  // 迁移评论的 created_at 是迁移时间，正文日期才是真实赞助时间；同日多笔用评论时间细分。
  const dayTs = Date.parse(date);
  const ts = Number.isFinite(dayTs)
    ? dayTs + ((Date.parse(createdAt) || 0) % 86_400_000)
    : Date.parse(createdAt) || 0;

  return { name, avatar: heading[1] ?? null, amountCents, date, message, ts };
}
