/**
 * 站点挂载前缀(astro.config 的 `base`,构建期由 `SITE_BASE` 注入)。
 *
 * 站内代码一律以「无前缀的站点路径」(`/download/`、`/api/release`)为内部表示,
 * 只在输出边界(href / src / fetch / redirect)经 `withBase` 补前缀;
 * 读取当前 URL(`Astro.url.pathname` / `location.pathname`)时先 `stripBase`。
 * 根部署时 `BASE` 为空串,两个函数都是恒等变换。
 */
export const BASE: string = import.meta.env.BASE_URL.replace(/\/+$/, "");

/** 站内绝对路径补上挂载前缀;外链、协议相对 URL、相对路径与已带前缀的路径原样返回。 */
export function withBase(path: string): string {
  if (!BASE || !path.startsWith("/") || path.startsWith("//")) return path;
  if (path === BASE || path.startsWith(`${BASE}/`) || path.startsWith(`${BASE}?`) || path.startsWith(`${BASE}#`)) {
    return path;
  }
  return `${BASE}${path}`;
}

/** 去掉路径的挂载前缀,得到站点路径(始终以 `/` 开头)。 */
export function stripBase(pathname: string): string {
  if (!BASE) return pathname || "/";
  if (pathname === BASE) return "/";
  return pathname.startsWith(`${BASE}/`) ? pathname.slice(BASE.length) : pathname || "/";
}
