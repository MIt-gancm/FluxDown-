import { GITHUB_TOKEN } from "astro:env/server";
import { isRecordComment, type RecordComment } from "@/lib/record-filter";

let ownerLogin: string | null = null;
let inflight: Promise<string> | null = null;

/**
 * GITHUB_TOKEN 持有者的登录名（记录评论的唯一可信作者）。
 * 成功后进程内缓存；失败抛错，调用方不得退化为「信任所有评论」。
 */
export function getTokenOwnerLogin(): Promise<string> {
  if (ownerLogin) return Promise.resolve(ownerLogin);
  inflight ??= (async () => {
    try {
      const res = await fetch("https://api.github.com/user", {
        headers: {
          Authorization: `Bearer ${GITHUB_TOKEN}`,
          Accept: "application/vnd.github+json",
          "X-GitHub-Api-Version": "2022-11-28",
        },
        signal: AbortSignal.timeout(8000),
      });
      if (!res.ok) throw new Error(`GitHub /user ${res.status}`);
      const data = (await res.json()) as { login?: string };
      if (!data.login) throw new Error("GitHub /user returned no login");
      ownerLogin = data.login;
      return ownerLogin;
    } finally {
      inflight = null;
    }
  })();
  return inflight;
}

/** 只保留 token 持有者发出、首行为固定标题的记录评论；无法确认 owner 时抛错。 */
export async function filterTrustedRecords<T extends RecordComment>(
  comments: T[],
  headings: readonly string[],
): Promise<T[]> {
  const owner = await getTokenOwnerLogin();
  return comments.filter((c) => isRecordComment(c, owner, headings));
}
