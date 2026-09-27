/**
 * 文档正文渐进增强(无依赖):
 * - h2/h3/h4 追加 `#` 锚点链接;
 * - 代码块包一层带语言标签与「复制」按钮的外框;
 * - 右侧目录 scroll-spy(`[data-toc-link]` → `aria-current="location"`);
 * - 移动抽屉内点击链接后自动收起。
 * 文案由 `[data-docs-article]` 上的 data-* 属性下发(页面按 URL 语言直出)。
 */

function enhanceHeadings(article: HTMLElement) {
  const prefix = article.dataset.anchorLabel ?? "";
  for (const heading of article.querySelectorAll<HTMLElement>(".docs-prose :is(h2, h3, h4)[id]")) {
    if (heading.querySelector(".h-anchor")) continue;
    const link = document.createElement("a");
    link.className = "h-anchor";
    link.href = `#${heading.id}`;
    link.setAttribute("aria-label", `${prefix}${heading.textContent?.trim() ?? ""}`);
    link.textContent = "#";
    heading.append(link);
  }
}

async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    return false;
  }
}

function enhanceCode(article: HTMLElement) {
  const copyLabel = article.dataset.copyLabel ?? "Copy";
  const copyAria = article.dataset.copyAria ?? copyLabel;
  const copiedLabel = article.dataset.copiedLabel ?? "Copied";
  for (const pre of article.querySelectorAll<HTMLPreElement>(".docs-prose pre")) {
    if (pre.parentElement?.classList.contains("code-block")) continue;
    const wrap = document.createElement("div");
    wrap.className = "code-block";
    const head = document.createElement("div");
    head.className = "code-head";
    const lang = document.createElement("span");
    lang.className = "code-lang";
    const language = pre.dataset.language;
    lang.textContent = language && language !== "plaintext" ? language : "text";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "code-copy";
    button.setAttribute("aria-label", copyAria);
    button.textContent = copyLabel;
    let timer = 0;
    button.addEventListener("click", async () => {
      const ok = await copyText(pre.querySelector("code")?.innerText ?? pre.innerText);
      if (!ok) return;
      button.textContent = copiedLabel;
      button.dataset.copied = "";
      window.clearTimeout(timer);
      timer = window.setTimeout(() => {
        button.textContent = copyLabel;
        delete button.dataset.copied;
      }, 1600);
    });
    head.append(lang, button);
    pre.replaceWith(wrap);
    wrap.append(head, pre);
  }
}

function scrollSpy() {
  const links = [...document.querySelectorAll<HTMLAnchorElement>("[data-toc-link]")];
  if (links.length === 0) return;
  const targets = links
    .map((link) => document.getElementById(link.dataset.tocLink ?? ""))
    .filter((el): el is HTMLElement => el !== null);
  let current = "";
  let frame = 0;

  const update = () => {
    frame = 0;
    const offset = parseFloat(getComputedStyle(document.documentElement).scrollPaddingTop) || 80;
    let id = targets[0]?.id ?? "";
    for (const el of targets) {
      if (el.getBoundingClientRect().top - offset - 8 <= 0) id = el.id;
      else break;
    }
    // 滚到页底时高亮最后一节(短小节无法顶到阈值)
    if (window.innerHeight + window.scrollY >= document.documentElement.scrollHeight - 2) {
      id = targets[targets.length - 1]?.id ?? id;
    }
    if (id === current) return;
    current = id;
    for (const link of links) {
      if (link.dataset.tocLink === id) link.setAttribute("aria-current", "location");
      else link.removeAttribute("aria-current");
    }
  };

  const schedule = () => {
    if (!frame) frame = requestAnimationFrame(update);
  };
  window.addEventListener("scroll", schedule, { passive: true });
  window.addEventListener("resize", schedule, { passive: true });
  update();
}

function drawerAutoClose() {
  const drawer = document.getElementById("docs-drawer");
  if (!drawer) return;
  drawer.addEventListener("click", (event) => {
    const link = (event.target as HTMLElement).closest("a[href]");
    if (link && "hidePopover" in drawer && drawer.matches(":popover-open")) drawer.hidePopover();
  });
}

const article = document.querySelector<HTMLElement>("[data-docs-article]");
if (article) {
  enhanceHeadings(article);
  enhanceCode(article);
}
scrollSpy();
drawerAutoClose();
