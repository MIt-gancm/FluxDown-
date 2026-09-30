/**
 * 客户端 IP：站点前置 Cloudflare，优先取 CF-Connecting-IP。
 * 不采用 X-Forwarded-For 首值（Cloudflare 对已有 XFF 是追加，首值客户端可控，
 * Astro 的 clientAddress 取的正是该值），仅在无 CF 头时回落到 clientAddress。
 */
export function getClientIp(request: Request, clientAddress?: string | null): string {
  const cf = request.headers.get("cf-connecting-ip")?.trim();
  if (cf && cf.length <= 45 && /^[0-9a-fA-F:.]+$/.test(cf)) return cf;
  return clientAddress || "unknown";
}
