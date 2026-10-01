/**
 * GPUI 主题编辑器的纯函数工具：resolve 结果 → CSS 变量、token 来源判定、引用候选与形状预设。
 * 预览只读 CSS 变量（`var(--gt-colors-primary)`），因此编辑后重新 resolve 即全量生效。
 */
import type { CSSProperties } from "react";
import { REGISTRY, TOKENS, type TokenKind, type TokenSpec } from "@/lib/gpui-theme/registry";
import type { ResolvedTokens } from "@/lib/gpui-theme/resolve";
import type { Json, TokenValue } from "@/lib/gpui-theme/types";
import { colorHex, f32Shortest, tokenValueJson } from "@/lib/gpui-theme/value";

/** token 路径 → CSS 自定义属性名：`colors.primary` → `--gt-colors-primary`。 */
export function cssVar(path: string): string {
  return `--gt-${path.replaceAll(".", "-")}`;
}

const MONO_STACK = 'ui-monospace, SFMono-Regular, Menlo, Consolas, "DejaVu Sans Mono", monospace';

function cssValue(spec: TokenSpec, value: TokenValue): string {
  switch (value.type) {
    case "color":
      return colorHex(value.color);
    case "number":
      return spec.kind === "fontWeight" || spec.kind === "number" ? String(f32Shortest(value.value)) : `${f32Shortest(value.value)}px`;
    case "font":
      if (Object.hasOwn(REGISTRY.fontSentinels, value.value)) {
        return value.value === "system-ui" ? "system-ui, sans-serif" : MONO_STACK;
      }
      return `"${value.value.replaceAll('"', "")}", ${spec.path === "typography.mono" ? MONO_STACK : "system-ui, sans-serif"}`;
    case "shadow":
      return value.layers.length === 0
        ? "none"
        : value.layers
            .map(
              (layer) =>
                `${layer.inset ? "inset " : ""}${f32Shortest(layer.x)}px ${f32Shortest(layer.y)}px ${f32Shortest(layer.blur)}px ${f32Shortest(layer.spread)}px ${colorHex(layer.color)}`,
            )
            .join(", ");
  }
}

/** 全部 token 的 CSS 变量（挂在预览根节点）。 */
export function cssVariables(tokens: ResolvedTokens): CSSProperties {
  const style: Record<string, string> = {};
  for (const spec of TOKENS) {
    const value = tokens.get(spec.path);
    if (value) style[cssVar(spec.path)] = cssValue(spec, value);
  }
  return style as CSSProperties;
}

/** 检视器 / 继承提示中展示的值。 */
export function displayValue(value: TokenValue | undefined): string {
  if (!value) return "—";
  const json = tokenValueJson(value);
  return typeof json === "string" ? json : JSON.stringify(json);
}

export type TokenSource = "inherit" | "ref" | "literal";

export function sourceOf(raw: Json | undefined): TokenSource {
  if (raw === undefined) return "inherit";
  return typeof raw === "string" && raw.startsWith("{") ? "ref" : "literal";
}

const NUMERIC: readonly TokenKind[] = ["length", "radius", "fontWeight", "number"];

/** 可被 `spec` 引用的 token（类型兼容：数值类互通），不含自身。 */
export function refCandidates(spec: TokenSpec): string[] {
  const numeric = NUMERIC.includes(spec.kind);
  return TOKENS.filter(
    (candidate) =>
      candidate.path !== spec.path &&
      (numeric ? NUMERIC.includes(candidate.kind) : candidate.kind === spec.kind),
  ).map((candidate) => candidate.path);
}

/** `{path}` / `{path}/NN` → 组成部分；非法时返回 `undefined`。 */
export function splitRef(raw: string): { path: string; opacity?: number } | undefined {
  const match = /^\{([A-Za-z0-9.]+)\}(?:\/(\d+(?:\.\d+)?))?$/.exec(raw);
  if (!match) return undefined;
  return { path: match[1], opacity: match[2] === undefined ? undefined : Number(match[2]) };
}

export type ShapePreset = "square" | "soft" | "rounded" | "pill";
export const SHAPE_PRESETS: readonly ShapePreset[] = ["square", "soft", "rounded", "pill"];

/** 形状预设：只把这些值批量写入 `tokens` 共享层（不是持久概念，写后可逐项再改）。 */
export const SHAPE_PRESET_VALUES: Record<ShapePreset, Record<string, Json>> = {
  square: {
    "radius.sm": 0,
    "radius.md": 0,
    "radius.lg": 0,
    "radius.xl": 0,
    "components.button.radius": "{radius.md}",
    "components.navItem.radius": "{radius.md}",
    "components.tab.radius": "{radius.md}",
    "components.badge.radius": "{radius.sm}",
    "components.progress.radius": "{radius.none}",
  },
  soft: {
    "radius.sm": 3,
    "radius.md": 6,
    "radius.lg": 8,
    "radius.xl": 12,
    "components.button.radius": "{radius.md}",
    "components.navItem.radius": "{radius.md}",
    "components.tab.radius": "{radius.md}",
    "components.badge.radius": "{radius.full}",
    "components.progress.radius": "{radius.full}",
  },
  rounded: {
    "radius.sm": 6,
    "radius.md": 10,
    "radius.lg": 14,
    "radius.xl": 20,
    "components.button.radius": "{radius.md}",
    "components.navItem.radius": "{radius.md}",
    "components.tab.radius": "{radius.md}",
    "components.badge.radius": "{radius.full}",
    "components.progress.radius": "{radius.full}",
  },
  pill: {
    "radius.sm": 8,
    "radius.md": 14,
    "radius.lg": 18,
    "radius.xl": 24,
    "components.button.radius": "{radius.full}",
    "components.navItem.radius": "{radius.full}",
    "components.tab.radius": "{radius.full}",
    "components.badge.radius": "{radius.full}",
    "components.progress.radius": "{radius.full}",
  },
};

/** 预览中引用 token 的 CSS：`v("colors.primary")` → `var(--gt-colors-primary)`。 */
export function v(path: string): string {
  return `var(${cssVar(path)})`;
}
