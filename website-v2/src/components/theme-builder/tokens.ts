/**
 * 主题 token → CSS 值的读取工具(预览渲染用),以及 token 路径标注。
 * 预览中每个视觉块通过 `data-token-paths` 绑定其使用的 token,供右键检视器读取。
 */
import {
  DEFAULT_METRICS,
  argbToCssRgba,
  getPathValue,
  normalizeHex8,
  type FluxThemeJson,
} from "@/lib/theme-builder";

/** 标注元素使用的 token 路径(右键检视器据此列出条目)。 */
export function tokenAttrs(...paths: string[]) {
  return { "data-token-paths": paths.join("|") };
}

export function formatInspectorValue(raw: unknown): string {
  if (typeof raw === "string") return raw;
  if (Array.isArray(raw)) return raw.join(", ");
  if (typeof raw === "number" || typeof raw === "boolean") return String(raw);
  if (raw === undefined || raw === null) return "undefined";
  return JSON.stringify(raw);
}

/** 颜色 token → CSS rgba();缺失时回退不透明黑。 */
export function rgba(theme: FluxThemeJson, path: string): string {
  const value = getPathValue(theme, path);
  return typeof value === "string" ? argbToCssRgba(value) : "rgba(0, 0, 0, 1)";
}

/** 颜色 token → 规范化 ARGB hex8;缺失时回退不透明黑。 */
export function hex8(theme: FluxThemeJson, path: string): string {
  const value = getPathValue(theme, path);
  return typeof value === "string" ? normalizeHex8(value) : "ff000000";
}

/** 读取 metric 数值,兼容缺失 `metrics` 段的旧 JSON(回退全局默认,镜像客户端行为)。 */
export function num(theme: FluxThemeJson, path: string): number {
  const value = getPathValue(theme, path);
  if (typeof value === "number" && !Number.isNaN(value)) return value;
  const fallback = getPathValue({ metrics: DEFAULT_METRICS }, path);
  return typeof fallback === "number" ? fallback : 0;
}

/** 基色 hex8 + 0–1 浮点 alpha → CSS rgba()(不经 0–255 量化,与客户端 withValues(alpha) 一致)。 */
export function rgbaWithAlpha(hex: string, alpha: number): string {
  const n = normalizeHex8(hex);
  const r = Number.parseInt(n.slice(2, 4), 16);
  const g = Number.parseInt(n.slice(4, 6), 16);
  const b = Number.parseInt(n.slice(6, 8), 16);
  return `rgba(${r}, ${g}, ${b}, ${alpha.toFixed(3)})`;
}

/** 颜色路径 + metric alpha 路径 → CSS rgba()。 */
export function alphaOf(theme: FluxThemeJson, colorPath: string, alphaPath: string): string {
  return rgbaWithAlpha(hex8(theme, colorPath), num(theme, alphaPath));
}

/** 分段配色的 token 路径:第 0 段用强调色,其余循环取 segmentPalette(与客户端分段条一致)。 */
export function segmentPath(theme: FluxThemeJson, segmentIndex: number): string {
  const len = theme.colors.segmentPalette.length;
  if (len === 0 || segmentIndex % (len + 1) === 0) return "colors.accent.color";
  return `colors.segmentPalette.${(segmentIndex - 1) % len}`;
}

/** 导出文件名:非单词字符折叠为下划线。 */
export function safeFileName(name: string): string {
  const cleaned = name.trim().replace(/[^\w\-]+/g, "_").toLowerCase();
  return cleaned || "flux_theme";
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1_073_741_824) return `${(bytes / 1_073_741_824).toFixed(1)} GB`;
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${bytes} B`;
}
