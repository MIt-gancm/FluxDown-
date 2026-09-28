/**
 * token 值 wire 语法（颜色 / 数值 / 字体 / 阴影 / 引用）的解析与规范化输出，
 * 与 Rust `crates/theme/src/value.rs` 同规则：数值按 f32 存储，颜色输出小写
 * `#rrggbbaa`（通道 `round(c * 255)`）。
 */
import type { TokenKind } from "./registry";
import type { Json, JsonObject, Hsla, ShadowValue, TokenValue } from "./types";
import { isJsonObject } from "./types";

const f = Math.fround;

/** sRGB 通道（0~1，f32）。 */
export interface Rgb {
  r: number;
  g: number;
  b: number;
  a: number;
}

/** gpui `impl From<Rgba> for Hsla`，逐步 f32 舍入。 */
export function hslaFromRgb({ r, g, b, a }: Rgb): Hsla {
  const max = Math.max(r, Math.max(g, b));
  const min = Math.min(r, Math.min(g, b));
  const delta = f(max - min);
  const l = f(f(max + min) / 2);
  let s: number;
  if (l === 0 || l === 1) s = 0;
  else if (l < 0.5) s = f(delta / f(2 * l));
  else s = f(delta / f(2 - f(2 * l)));
  let h: number;
  if (delta === 0) {
    h = 0;
  } else if (max === r) {
    // f32::rem_euclid(6.0)
    const remainder = f(f(g - b) / delta) % 6;
    h = f((remainder < 0 ? f(remainder + 6) : remainder) / 6);
  } else if (max === g) {
    h = f(f(f(f(b - r) / delta) + 2) / 6);
  } else {
    h = f(f(f(f(r - g) / delta) + 4) / 6);
  }
  return { h, s, l, a };
}

/** gpui `impl From<Hsla> for Rgba`，逐步 f32 舍入。 */
export function rgbFromHsla({ h, s, l, a }: Hsla): Rgb {
  const c = f(f(1 - Math.abs(f(f(2 * l) - 1))) * s);
  const h6 = f(h * 6);
  const x = f(c * f(1 - Math.abs(f((h6 % 2) - 1))));
  const m = f(l - f(c / 2));
  const cm = f(c + m);
  const xm = f(x + m);
  let rgb: [number, number, number];
  switch (Math.floor(h6)) {
    case 0:
    case 6:
      rgb = [cm, xm, m];
      break;
    case 1:
      rgb = [xm, cm, m];
      break;
    case 2:
      rgb = [m, cm, xm];
      break;
    case 3:
      rgb = [m, xm, cm];
      break;
    case 4:
      rgb = [xm, m, cm];
      break;
    default:
      rgb = [cm, m, xm];
  }
  const clamp = (value: number) => Math.min(Math.max(value, 0), 1);
  return { r: clamp(rgb[0]), g: clamp(rgb[1]), b: clamp(rgb[2]), a };
}

/** gpui `rgba(0xRRGGBBAA)` → Hsla。 */
function hslaFromBytes(r: number, g: number, b: number, a: number): Hsla {
  return hslaFromRgb({ r: f(r / 255), g: f(g / 255), b: f(b / 255), a: f(a / 255) });
}

/** 阴影缺省颜色：黑色 46/255 alpha。 */
export const SHADOW_DEFAULT_COLOR: Hsla = hslaFromBytes(0, 0, 0, 0x2e);
/** gpui `Hsla::default()`。 */
export const TRANSPARENT: Hsla = { h: 0, s: 0, l: 0, a: 0 };

/** f32 的最短十进制表示（与 Rust `f32` 的 Display 一致，`0.38` 而不是 `0.3799999952316284`）。 */
export function f32Shortest(value: number): number {
  const single = Math.fround(value);
  if (!Number.isFinite(single) || Number.isInteger(single)) return single;
  for (let precision = 1; precision <= 9; precision += 1) {
    const candidate = Number(single.toPrecision(precision));
    if (Math.fround(candidate) === single) return candidate;
  }
  return single;
}

/** 数值 → wire JSON：整数输出为整数，其余取 f32 最短表示；非有限值为 `null`。 */
export function numberJson(value: number): Json {
  if (!Number.isFinite(value)) return null;
  return f32Shortest(value);
}

/** 颜色 → 小写 `#rrggbbaa`。 */
export function colorHex(color: Hsla): string {
  const rgb = rgbFromHsla(color);
  const channel = (value: number) =>
    Math.round(f(Math.min(Math.max(value, 0), 1) * 255))
      .toString(16)
      .padStart(2, "0");
  return `#${channel(rgb.r)}${channel(rgb.g)}${channel(rgb.b)}${channel(rgb.a)}`;
}

/** `#RRGGBB` / `#RRGGBBAA`（大小写不限）→ 颜色。 */
export function parseHexColor(text: string): Hsla | undefined {
  if (!text.startsWith("#")) return undefined;
  const hex = text.slice(1);
  if (!/^[0-9a-fA-F]*$/.test(hex) || (hex.length !== 6 && hex.length !== 8)) return undefined;
  const byte = (index: number) => Number.parseInt(hex.slice(index, index + 2), 16);
  return hslaFromBytes(byte(0), byte(2), byte(4), hex.length === 8 ? byte(6) : 255);
}

/** gpui `Hsla::opacity`：alpha 乘以 clamp(factor, 0, 1)。 */
export function withOpacity(color: Hsla, factor: number): Hsla {
  return { ...color, a: f(color.a * Math.min(Math.max(f(factor), 0), 1)) };
}

/** 在 sRGB 空间把 `from` 向 `to` 混合 `amount`（0 = from，1 = to），结果不透明。 */
export function mixSrgb(from: Hsla, to: Hsla, amount: number): Hsla {
  const a = rgbFromHsla(from);
  const b = rgbFromHsla(to);
  const t = Math.min(Math.max(f(amount), 0), 1);
  const lerp = (x: number, y: number) => f(x + f(f(y - x) * t));
  return hslaFromRgb({ r: lerp(a.r, b.r), g: lerp(a.g, b.g), b: lerp(a.b, b.b), a: 1 });
}

/** Rust `relative_luminance`（WCAG 相对亮度，逐步 f32；alpha 不参与）。 */
export function relativeLuminance(color: Hsla): number {
  const { r, g, b } = rgbFromHsla(color);
  const linear = (channel: number) =>
    channel <= f(0.03928) ? f(channel / f(12.92)) : f(Math.pow(f(f(channel + f(0.055)) / f(1.055)), f(2.4)));
  return f(f(f(f(0.2126) * linear(r)) + f(f(0.7152) * linear(g))) + f(f(0.0722) * linear(b)));
}

/** 相对亮度的黑白分界（Rust `CONTRAST_PIVOT`）。 */
const CONTRAST_PIVOT = f(0.17913);
const CONTRAST_STEP = f(0.01);

/** WCAG 对比度（参数为相对亮度，顺序无关）。 */
function contrastRatio(a: number, b: number): number {
  const [light, dark] = a > b ? [a, b] : [b, a];
  return f(f(light + f(0.05)) / f(dark + f(0.05)));
}

/**
 * 注册表 `contrast` 表达式（Rust `with_min_contrast`）：保持色相 / 饱和度 / alpha，沿亮度远离
 * `against`，直到对比度 ≥ `min` 或亮度到头；已达标原样返回。
 */
export function withMinContrast(color: Hsla, against: Hsla, min: number): Hsla {
  const background = relativeLuminance(against);
  const darken = background > CONTRAST_PIVOT;
  const target = f(min);
  let current = color;
  for (;;) {
    if (contrastRatio(relativeLuminance(current), background) >= target) return current;
    const next = darken ? Math.max(f(current.l - CONTRAST_STEP), 0) : Math.min(f(current.l + CONTRAST_STEP), 1);
    if (next === current.l) return current;
    current = { ...current, l: next };
  }
}

export function tokenValueJson(value: TokenValue): Json {
  switch (value.type) {
    case "color":
      return colorHex(value.color);
    case "number":
      return numberJson(value.value);
    case "font":
      return value.value;
    case "shadow":
      return value.layers.map(shadowJson);
  }
}

export function shadowJson(layer: ShadowValue): JsonObject {
  return {
    x: numberJson(layer.x),
    y: numberJson(layer.y),
    blur: numberJson(layer.blur),
    spread: numberJson(layer.spread),
    color: colorHex(layer.color),
    inset: layer.inset,
  };
}

/** 值能否填入 `kind` 类型的 token（数值类 kind 之间互通）。 */
export function fitsKind(value: TokenValue, kind: TokenKind): boolean {
  switch (value.type) {
    case "color":
      return kind === "color";
    case "number":
      return kind === "length" || kind === "radius" || kind === "fontWeight" || kind === "number";
    case "font":
      return kind === "fontFamily";
    case "shadow":
      return kind === "shadow";
  }
}

/** 引用：`{group.key}` 或 `{group.key}/NN`（NN 为 0~100 的不透明度百分比，乘到源 alpha 上）。 */
export interface Reference {
  path: string;
  /** 0 ~ 1。 */
  opacity?: number;
}

export type ColorSource = { type: "literal"; color: Hsla } | { type: "ref"; ref: Reference };

export interface WireShadow {
  x: number;
  y: number;
  blur: number;
  spread: number;
  color: ColorSource;
  inset: boolean;
}

export type WireValue =
  | { type: "literal"; value: TokenValue }
  | { type: "ref"; ref: Reference }
  | { type: "shadow"; layers: WireShadow[] };

type Outcome<T> = { ok: true; value: T } | { ok: false; error: string };

/** serde_json `Value` 的 Display（紧凑 JSON），用于诊断文案。 */
export function jsonDisplay(value: Json): string {
  return JSON.stringify(value);
}

/** Rust `str::parse::<f32>` 接受的语法。 */
const RUST_FLOAT = /^[+-]?(?:(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?|inf|infinity|nan)$/i;

function parseRustF32(text: string): number | undefined {
  if (!RUST_FLOAT.test(text)) return undefined;
  const lower = text.toLowerCase().replace(/^\+/, "");
  if (lower.endsWith("nan")) return Number.NaN;
  if (lower.endsWith("inf") || lower.endsWith("infinity")) return lower.startsWith("-") ? -Infinity : Infinity;
  return Math.fround(Number(text));
}

/** 字符串以 `{` 开头即按引用解析；返回 `undefined` 表示不是引用语法。 */
export function parseReference(text: string): Outcome<Reference> | undefined {
  if (!text.startsWith("{")) return undefined;
  const rest = text.slice(1);
  const close = rest.indexOf("}");
  if (close < 0) return { ok: false, error: `引用缺少右花括号：${text}` };
  const path = rest.slice(0, close);
  const tail = rest.slice(close + 1);
  const validPath = path.length > 0 && path.split(".").every((segment) => /^[A-Za-z0-9]+$/.test(segment));
  if (!validPath) return { ok: false, error: `引用路径非法：${text}` };
  if (tail.length === 0) return { ok: true, value: { path } };
  const percent = tail.startsWith("/") ? parseRustF32(tail.slice(1)) : undefined;
  if (percent === undefined) return { ok: false, error: `引用透明度后缀非法：${text}` };
  if (!(percent >= 0 && percent <= 100)) return { ok: false, error: `引用透明度须在 0~100：${text}` };
  return { ok: true, value: { path, opacity: Math.fround(percent / 100) } };
}

/** JSON number → f32；非数字或 f32 溢出为 `undefined`。 */
function f32Of(value: Json | undefined): number | undefined {
  if (typeof value !== "number") return undefined;
  const single = Math.fround(value);
  return Number.isFinite(single) ? single : undefined;
}

/** 按 token 类型解析 wire 值；错误信息用于诊断。 */
export function parseWire(kind: TokenKind, value: Json): Outcome<WireValue> {
  if (typeof value === "string") {
    const reference = parseReference(value);
    if (reference) {
      if (!reference.ok) return reference;
      if (reference.value.opacity !== undefined && kind !== "color") {
        return { ok: false, error: `只有颜色引用可带透明度：${value}` };
      }
      return { ok: true, value: { type: "ref", ref: reference.value } };
    }
  }
  switch (kind) {
    case "color": {
      const color = typeof value === "string" ? parseHexColor(value) : undefined;
      return color
        ? { ok: true, value: { type: "literal", value: { type: "color", color } } }
        : { ok: false, error: `颜色须为 #RRGGBB / #RRGGBBAA 或引用：${jsonDisplay(value)}` };
    }
    case "length":
    case "radius":
    case "fontWeight":
    case "number": {
      const number = f32Of(value);
      return number === undefined
        ? { ok: false, error: `须为数字或引用：${jsonDisplay(value)}` }
        : { ok: true, value: { type: "literal", value: { type: "number", value: number } } };
    }
    case "fontFamily":
      return typeof value === "string" && value.trim().length > 0
        ? { ok: true, value: { type: "literal", value: { type: "font", value } } }
        : { ok: false, error: `字体须为非空字符串：${jsonDisplay(value)}` };
    case "shadow": {
      const layers = parseShadow(value);
      return layers.ok ? { ok: true, value: { type: "shadow", layers: layers.value } } : layers;
    }
  }
}

function parseShadow(value: Json): Outcome<WireShadow[]> {
  if (!Array.isArray(value)) return { ok: false, error: `阴影须为数组或引用：${jsonDisplay(value)}` };
  const layers: WireShadow[] = [];
  for (const [index, layer] of value.entries()) {
    if (!isJsonObject(layer)) return { ok: false, error: `阴影第 ${index} 层须为对象` };
    const numbers: Record<"x" | "y" | "blur" | "spread", number> = { x: 0, y: 0, blur: 0, spread: 0 };
    for (const key of ["x", "y", "blur", "spread"] as const) {
      if (!(key in layer)) continue;
      const number = f32Of(layer[key]);
      if (number === undefined) return { ok: false, error: `阴影第 ${index} 层 ${key} 须为数字` };
      numbers[key] = number;
    }
    let color: ColorSource = { type: "literal", color: SHADOW_DEFAULT_COLOR };
    if ("color" in layer) {
      const raw = layer.color;
      if (typeof raw !== "string") {
        return { ok: false, error: `阴影第 ${index} 层 color 非法：${jsonDisplay(raw)}` };
      }
      const reference = parseReference(raw);
      if (reference) {
        if (!reference.ok) return reference;
        color = { type: "ref", ref: reference.value };
      } else {
        const literal = parseHexColor(raw);
        if (!literal) return { ok: false, error: `阴影第 ${index} 层 color 非法：${raw}` };
        color = { type: "literal", color: literal };
      }
    }
    let inset = false;
    if ("inset" in layer) {
      const raw = layer.inset;
      if (typeof raw !== "boolean") {
        return { ok: false, error: `阴影第 ${index} 层 inset 须为布尔：${jsonDisplay(raw)}` };
      }
      inset = raw;
    }
    layers.push({ ...numbers, color, inset });
  }
  return { ok: true, value: layers };
}

/** 合法字面量（含颜色全为字面量的阴影）的规范 JSON；引用、含引用的阴影与非法值返回 `undefined`。 */
export function normalizedLiteral(kind: TokenKind, value: Json): Json | undefined {
  const parsed = parseWire(kind, value);
  if (!parsed.ok) return undefined;
  const wire = parsed.value;
  switch (wire.type) {
    case "literal":
      return tokenValueJson(wire.value);
    case "ref":
      return undefined;
    case "shadow": {
      const layers: Json[] = [];
      for (const layer of wire.layers) {
        if (layer.color.type === "ref") return undefined;
        layers.push(shadowJson({ ...layer, color: layer.color.color }));
      }
      return layers;
    }
  }
}
