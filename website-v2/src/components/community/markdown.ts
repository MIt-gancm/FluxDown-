/**
 * Issue / 评论正文的极简 Markdown 渲染:先整体 HTML 转义,再识别粗体、行内代码、
 * `##`/`###` 标题、`- ` 列表与普通段落。输出只含固定标签,样式由 `.cm-md` 提供。
 */
export function renderMarkdown(md: string): string {
  const escaped = md.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  const inline = (s: string) =>
    s.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>").replace(/`([^`]+)`/g, "<code>$1</code>");

  const out: string[] = [];
  let list: string[] = [];
  const flushList = () => {
    if (list.length) out.push(`<ul>${list.join("")}</ul>`);
    list = [];
  };

  for (const raw of escaped.split(/\r?\n/)) {
    const line = raw.trimEnd();
    const item = line.match(/^\s*[-*] (.+)$/);
    if (item) {
      list.push(`<li>${inline(item[1])}</li>`);
      continue;
    }
    flushList();
    if (!line.trim()) continue;
    const heading = line.match(/^(#{2,3}) (.+)$/);
    if (heading) out.push(heading[1].length === 2 ? `<h3>${inline(heading[2])}</h3>` : `<h4>${inline(heading[2])}</h4>`);
    else out.push(`<p>${inline(line)}</p>`);
  }
  flushList();
  return out.join("");
}
