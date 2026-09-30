/**
 * 排除域名的归一化与匹配。后台拦截判定、popup「排除当前站点」、设置页
 * 必须共用同一套规则，否则三处对同一条规则的解读会不一致。
 */

/**
 * 把用户输入（`example.com`、`*.example.com`、`https://Example.com/path`）
 * 归一化为小写主机名；无法解析或为空时返回 null。
 */
export function normalizeDomain(input: string): string | null {
  let text = input.trim().toLowerCase();
  if (!text) return null;
  text = text.replace(/^\*\./, "");
  let host: string;
  try {
    host = new URL(text.includes("://") ? text : `http://${text}`).hostname;
  } catch {
    return null;
  }
  host = host.replace(/^\.+/, "").replace(/\.+$/, "");
  return host || null;
}

/** 归一化并去重整个列表，丢弃无效项（保持首次出现顺序）。 */
export function normalizeDomainList(list: readonly string[]): string[] {
  const seen = new Set<string>();
  for (const raw of list) {
    const d = normalizeDomain(raw);
    if (d) seen.add(d);
  }
  return [...seen];
}

/** host 等于规则域名，或是其子域名（按 DNS 标签边界，`t.co` 不会命中 `microsoft.com`）。 */
export function isHostExcluded(host: string, rule: string): boolean {
  const h = host.toLowerCase();
  return h === rule || h.endsWith(`.${rule}`);
}

/** 任一规则命中即为真。规则须已归一化（loadSettings 读取时会归一化）。 */
export function isDomainExcluded(
  host: string,
  rules: readonly string[],
): boolean {
  return rules.some((rule) => isHostExcluded(host, rule));
}
