/**
 * 全站交互与动效(单一入口,Layout 引入一次)。
 *
 * - `[data-reveal]`         进入视口淡入上浮(可用 style="--d:N" 错峰)
 * - `[data-scramble]`       进入视口时字符解码动画(eyebrow / 标签)
 * - `[data-count]`          数字滚动到目标值(data-count="113638")
 * - `.rule`                 首次进入视口加 `.is-in`:扫描光 + 准星锁定(样式见 global.css)
 * - `.spot`                 指针聚光(写入 --mx / --my;`.spot-edge` 同时点亮边框)
 * - `[data-theme-toggle]`   主题切换,支持时以点击点为圆心做 view transition 揭示
 * - 键盘:`D` 跳下载页,`⌘K` / `Ctrl+K` / `/` 打开命令面板(派发 `palette:open`)
 */

const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

/* ── Reveal ─────────────────────────────────────────────── */
const revealObserver = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const el = entry.target as HTMLElement;
      el.classList.add("is-in");
      revealObserver.unobserve(el);
      if (el.hasAttribute("data-scramble")) scramble(el);
      if (el.hasAttribute("data-count")) countUp(el);
    }
  },
  { rootMargin: "0px 0px -8% 0px", threshold: 0.08 },
);

const OBSERVED = "[data-reveal], [data-scramble], [data-count], .rule";

export function observeAll(root: ParentNode = document) {
  root
    .querySelectorAll<HTMLElement>("[data-reveal]:not(.is-in), [data-scramble], [data-count], .rule:not(.is-in)")
    .forEach((el) => revealObserver.observe(el));
}

/* ── Scramble ───────────────────────────────────────────── */
const GLYPHS = "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789#%&*+=<>/";

function scramble(el: HTMLElement) {
  if (reduced || el.dataset.scrambled) return;
  el.dataset.scrambled = "1";
  const target = el.textContent ?? "";
  // CJK 字符不参与乱码(视觉噪声过大),只做逐字显现
  const frames = 18;
  let frame = 0;
  const tick = () => {
    frame++;
    const progress = frame / frames;
    const revealed = Math.floor(target.length * progress);
    let out = "";
    for (let i = 0; i < target.length; i++) {
      const ch = target[i] ?? "";
      if (i < revealed || ch === " " || /[\u3000-\u9fff]/.test(ch)) out += ch;
      else out += GLYPHS[Math.floor(Math.random() * GLYPHS.length)];
    }
    el.textContent = out;
    if (frame < frames) requestAnimationFrame(tick);
    else el.textContent = target;
  };
  requestAnimationFrame(tick);
}

/* ── Count up ───────────────────────────────────────────── */
function countUp(el: HTMLElement) {
  const target = Number(el.dataset.count);
  if (!Number.isFinite(target)) return;
  const format = new Intl.NumberFormat(document.documentElement.lang);
  if (reduced) {
    el.textContent = format.format(target);
    return;
  }
  const start = performance.now();
  const duration = 1400;
  const step = (now: number) => {
    const t = Math.min(1, (now - start) / duration);
    const eased = 1 - Math.pow(1 - t, 4);
    el.textContent = format.format(Math.round(target * eased));
    if (t < 1) requestAnimationFrame(step);
  };
  requestAnimationFrame(step);
}

/* ── Pointer spotlight ──────────────────────────────────── */
let spotFrame = 0;
document.addEventListener(
  "pointermove",
  (event) => {
    if (spotFrame || event.pointerType !== "mouse") return;
    spotFrame = requestAnimationFrame(() => {
      spotFrame = 0;
      const el = (event.target as Element | null)?.closest<HTMLElement>(".spot");
      if (!el) return;
      const rect = el.getBoundingClientRect();
      el.style.setProperty("--mx", `${event.clientX - rect.left}px`);
      el.style.setProperty("--my", `${event.clientY - rect.top}px`);
    });
  },
  { passive: true },
);

/* ── Theme toggle ───────────────────────────────────────── */
function applyTheme(next: "light" | "dark") {
  document.documentElement.dataset.theme = next;
  try {
    localStorage.setItem("fluxdown-theme", next);
  } catch {
    // 隐私模式:仅本次会话生效
  }
  window.dispatchEvent(new CustomEvent("theme-change", { detail: { theme: next } }));
}

document.addEventListener("click", (event) => {
  const trigger = (event.target as Element | null)?.closest("[data-theme-toggle]");
  if (!trigger) return;
  const next = document.documentElement.dataset.theme === "dark" ? "light" : "dark";
  if (reduced || !document.startViewTransition) {
    applyTheme(next);
    return;
  }
  const x = event.clientX || window.innerWidth / 2;
  const y = event.clientY || 0;
  const radius = Math.hypot(Math.max(x, innerWidth - x), Math.max(y, innerHeight - y));
  document.documentElement.classList.add("theme-vt");
  const transition = document.startViewTransition(() => applyTheme(next));
  transition.ready.then(() => {
    document.documentElement.animate(
      { clipPath: [`circle(0 at ${x}px ${y}px)`, `circle(${radius}px at ${x}px ${y}px)`] },
      { duration: 520, easing: "cubic-bezier(0.22, 1, 0.36, 1)", pseudoElement: "::view-transition-new(root)" },
    );
  });
  transition.finished.finally(() => document.documentElement.classList.remove("theme-vt"));
});

/* ── Keyboard ───────────────────────────────────────────── */
document.addEventListener("keydown", (event) => {
  const target = event.target as HTMLElement | null;
  const typing =
    target?.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target?.tagName ?? "");
  if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "k") {
    event.preventDefault();
    window.dispatchEvent(new CustomEvent("palette:open"));
    return;
  }
  if (typing || event.metaKey || event.ctrlKey || event.altKey) return;
  if (event.key === "/") {
    event.preventDefault();
    window.dispatchEvent(new CustomEvent("palette:open"));
  } else if (event.key.toLowerCase() === "d") {
    const link = document.querySelector<HTMLAnchorElement>("[data-shortcut='d']");
    if (link) link.click();
  }
});

/* ── Header state ───────────────────────────────────────── */
const header = document.querySelector<HTMLElement>(".site-header");
if (header) {
  const sync = () => header.toggleAttribute("data-scrolled", window.scrollY > 8);
  sync();
  window.addEventListener("scroll", sync, { passive: true });
}

observeAll();
// React islands 挂载后可能插入新的 [data-reveal];轻量监听 DOM 增量
new MutationObserver((records) => {
  for (const record of records) {
    record.addedNodes.forEach((node) => {
      if (node instanceof HTMLElement) {
        if (node.matches(OBSERVED)) revealObserver.observe(node);
        observeAll(node);
      }
    });
  }
}).observe(document.body, { childList: true, subtree: true });
