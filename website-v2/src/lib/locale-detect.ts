/**
 * 服务端 Accept-Language 解析,判定规则与 Layout.astro 首帧内联脚本的浏览器语言判定对齐:
 * 按优先级顺序遍历,zh 前缀 → zh,en 前缀 → en,都不匹配回退 en。
 */
export function parseAcceptLanguage(header: string | null): "en" | "zh" {
  if (!header) return "en";
  const tags = header
    .split(",")
    .map((part) => {
      const [tag, ...params] = part.trim().split(";");
      const q = params
        .map((p) => p.trim())
        .find((p) => p.startsWith("q="));
      return { tag: (tag ?? "").trim().toLowerCase(), q: q ? Number.parseFloat(q.slice(2)) : 1 };
    })
    .filter((t) => t.tag.length > 0)
    .sort((a, b) => b.q - a.q);
  for (const { tag } of tags) {
    if (tag.startsWith("zh")) return "zh";
    if (tag.startsWith("en")) return "en";
  }
  return "en";
}
