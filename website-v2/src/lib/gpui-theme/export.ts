/**
 * 主题导出，与 Rust `ThemeDocument::to_json_value` / `to_json_pretty` 同算法：
 *
 * - `diff`：反复删掉「删掉后两模式全部 token 的导出值都不变」的字面量覆盖直到不动点；
 *   引用、未知键与 `accentTokenPaths` 上的字面量保留（运行时强调色只作用于基底，
 *   删掉显式固定的强调色路径会让它被覆盖）。
 * - `full`：两模式全部 token 解析为字面量写出（两模式相同的写入 `tokens`）；kitBound 不写。
 * - 合法字面量改写为规范形式（颜色小写 `#rrggbbaa`、数字 f32 最短表示）。
 * - 键序：顶层固定；层内分组按注册表 `groups`，组内按注册表 token 顺序，未知键按原顺序排最后。
 */
import { ACCENT_TOKEN_PATHS, GROUPS, TOKENS, tokenSpec } from "./registry";
import { removeToken, setToken, tokenValue } from "./document";
import { flatTokens, resolveTheme } from "./resolve";
import {
  THEME_FORMAT,
  THEME_SCHEMA_URL,
  THEME_SCHEMA_VERSION,
  cloneDocument,
  isJsonObject,
  type ExportMode,
  type Json,
  type JsonObject,
  type ThemeDocument,
  type ThemeMeta,
  type TokenLayer,
} from "./types";
import { normalizedLiteral, tokenValueJson } from "./value";

const LAYERS: readonly TokenLayer[] = ["tokens", "dark", "light"];

function snapshot(document: ThemeDocument): string {
  return JSON.stringify([flatTokens(resolveTheme(document, "dark").tokens), flatTokens(resolveTheme(document, "light").tokens)]);
}

function minimized(source: ThemeDocument): ThemeDocument {
  let document = cloneDocument(source);
  let current = snapshot(document);
  for (;;) {
    let changed = false;
    for (const layer of ["dark", "light", "tokens"] as const) {
      for (const spec of TOKENS) {
        const value = tokenValue(document, layer, spec.path);
        if (value === undefined) continue;
        if ((typeof value === "string" && value.startsWith("{")) || ACCENT_TOKEN_PATHS.has(spec.path)) continue;
        const without = cloneDocument(document);
        removeToken(without, layer, spec.path);
        const next = snapshot(without);
        if (next === current) {
          document = without;
          current = next;
          changed = true;
        }
      }
    }
    if (!changed) return document;
  }
}

function expanded(source: ThemeDocument): ThemeDocument {
  const document = cloneDocument(source);
  const dark = resolveTheme(source, "dark").tokens;
  const light = resolveTheme(source, "light").tokens;
  for (const layer of LAYERS) {
    for (const spec of TOKENS) removeToken(document, layer, spec.path);
  }
  for (const spec of TOKENS) {
    if (spec.kitBound) continue;
    const darkValue = dark.get(spec.path);
    const lightValue = light.get(spec.path);
    if (!darkValue || !lightValue) continue;
    const darkJson = tokenValueJson(darkValue);
    const lightJson = tokenValueJson(lightValue);
    if (JSON.stringify(darkJson) === JSON.stringify(lightJson)) {
      setToken(document, "tokens", spec.path, darkJson);
    } else {
      setToken(document, "dark", spec.path, darkJson);
      setToken(document, "light", spec.path, lightJson);
    }
  }
  return document;
}

function normalizeLiterals(document: ThemeDocument): void {
  for (const layer of LAYERS) {
    for (const spec of TOKENS) {
      const value = tokenValue(document, layer, spec.path);
      if (value === undefined) continue;
      const normalized = normalizedLiteral(spec.kind, value);
      if (normalized !== undefined) setToken(document, layer, spec.path, normalized);
    }
  }
}

/** 子键按注册表顺序排列，未知键按原顺序排在最后。 */
function canonicalObject(map: JsonObject, prefix: string): JsonObject {
  const order = (key: string): number => {
    if (!prefix) return GROUPS.indexOf(key);
    const path = `${prefix}.${key}`;
    return TOKENS.findIndex((spec) => spec.path === path || spec.path.startsWith(`${path}.`));
  };
  const known: [number, string, Json][] = [];
  const unknown: [string, Json][] = [];
  for (const [key, value] of Object.entries(map)) {
    const index = order(key);
    if (index >= 0) known.push([index, key, value]);
    else unknown.push([key, value]);
  }
  known.sort((a, b) => a[0] - b[0]);
  const object: JsonObject = {};
  for (const [, key, value] of known) {
    const path = prefix ? `${prefix}.${key}` : key;
    object[key] = isJsonObject(value) && !tokenSpec(path) ? canonicalObject(value, path) : value;
  }
  for (const [key, value] of unknown) object[key] = value;
  return object;
}

function metaJson(meta: ThemeMeta): JsonObject {
  const object: JsonObject = {};
  for (const key of ["id", "name", "author", "version", "description"] as const) {
    const value = meta[key];
    if (value !== undefined) object[key] = value;
  }
  return Object.assign(object, meta.extra);
}

/** 规范键序的 JSON 对象。 */
export function exportThemeValue(source: ThemeDocument, mode: ExportMode): JsonObject {
  const document = mode === "diff" ? minimized(source) : expanded(source);
  normalizeLiterals(document);
  const object: JsonObject = {
    $schema: THEME_SCHEMA_URL,
    format: THEME_FORMAT,
    schemaVersion: Math.max(document.schemaVersion ?? THEME_SCHEMA_VERSION, THEME_SCHEMA_VERSION),
  };
  if (document.meta) object.meta = metaJson(document.meta);
  if (document.extends !== undefined) object.extends = document.extends;
  for (const layer of LAYERS) {
    if (Object.keys(document[layer]).length > 0) object[layer] = canonicalObject(document[layer], "");
  }
  return Object.assign(object, document.extra);
}

/** 规范键序、两空格缩进、末尾换行（与 serde_json `to_string_pretty` 字节一致）。 */
export function exportTheme(document: ThemeDocument, mode: ExportMode): string {
  return `${JSON.stringify(exportThemeValue(document, mode), null, 2)}\n`;
}
