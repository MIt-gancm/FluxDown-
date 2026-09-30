/**
 * 以 issue 评论为存储的记录（投票 / 赞助名录）只信任记录写入者发出的评论：
 * 任何 GitHub 用户都能在公开 issue 下评论，不校验作者就会被伪造计数。
 */

export interface RecordComment {
  body?: string | null;
  user?: { login?: string | null } | null;
}

/** 站点自己在访客回复评论里写入的标记（见 issues/[number]/comments.ts）。 */
const VISITOR_REPLY_MARK = "Website visitor reply";

/**
 * 评论是否为 ownerLogin 发出、且首行（去掉前导空行后）严格等于某个固定标题。
 * 首行固定可以排除访客回复等夹带 ```json 的其他评论。
 */
export function isRecordComment(
  c: RecordComment,
  ownerLogin: string,
  headings: readonly string[],
): boolean {
  const login = c.user?.login;
  if (!login || login.toLowerCase() !== ownerLogin.toLowerCase()) return false;
  const body = c.body ?? "";
  if (body.includes(VISITOR_REPLY_MARK)) return false;
  const firstLine = body.replace(/^\s+/, "").split(/\r?\n/, 1)[0]?.trim() ?? "";
  return headings.includes(firstLine);
}

export function selectRecordBodies(
  comments: readonly RecordComment[],
  ownerLogin: string,
  headings: readonly string[],
): string[] {
  return comments
    .filter((c) => isRecordComment(c, ownerLogin, headings))
    .map((c) => c.body ?? "");
}
