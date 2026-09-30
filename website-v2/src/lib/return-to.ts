/**
 * returnTo 只允许站内绝对路径。浏览器会把 `\` 当作 `/`、并忽略制表符/换行，
 * 所以 `/\evil.com`、`/\t/evil.com` 也会被解析成跨域地址，须一并拒绝。
 */
export function isSafeLocalPath(raw: string | null | undefined): raw is string {
  if (!raw || !raw.startsWith("/") || raw.startsWith("//")) return false;
  // eslint-disable-next-line no-control-regex
  if (/[\\\u0000-\u001f\u007f]/.test(raw)) return false;
  try {
    const u = new URL(raw, "https://x.invalid");
    return u.origin === "https://x.invalid";
  } catch {
    return false;
  }
}
