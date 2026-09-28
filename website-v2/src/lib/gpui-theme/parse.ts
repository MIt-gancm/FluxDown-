/**
 * 主题 JSON → `ThemeDocument`，与 Rust `ThemeDocument::from_value` 同规则：
 *
 * - `format` 为 `fluxdown.gpui-theme` → v2；否则若形如 Flutter FluxThemeJson → 适配转换；
 *   否则 `format` 存在即整体拒绝；无 `format` 且形如 v1 内部格式 → v1 迁移；其余按 v2。
 * - 未知键保留并记 `unknownKey`；结构错误记 `invalidValue`；值本身在 resolve 时校验。
 * - `schemaVersion` 高于支持版本 → `newerVersion`；低于则按 `registry.migrations.steps` 升级。
 */
import { REGISTRY, TOKENS, tokenSpec } from "./registry";
import { getPath } from "./document";
import {
  THEME_FORMAT,
  THEME_SCHEMA_VERSION,
  emptyDocument,
  isJsonObject,
  type Diagnostic,
  type Json,
  type JsonObject,
  type ParseResult,
  type ThemeMeta,
} from "./types";
import { jsonDisplay } from "./value";

const META_KEYS = ["id", "name", "author", "version", "description"] as const;
type MetaKey = (typeof META_KEYS)[number];

const V1_GROUPS = ["colors", "radius", "spacing", "typography", "shadow"];

/** 顶层 `appearance` 为 `dark` / `light` 且 `colors` 为对象。 */
export function isFlutterTheme(object: JsonObject): boolean {
  return (
    typeof object.appearance === "string" &&
    REGISTRY.flutterAdapter.appearances.includes(object.appearance) &&
    isJsonObject(object.colors)
  );
}

/** `AARRGGBB` / `RRGGBB`（可带 `#`）→ `#rrggbbaa`。 */
function flutterHex(text: string): string | undefined {
  const trimmed = text.trim();
  const hex = trimmed.startsWith("#") ? trimmed.slice(1) : trimmed;
  if (!/^[0-9a-fA-F]*$/.test(hex)) return undefined;
  let argb: string;
  if (hex.length === 6) argb = `ff${hex}`;
  else if (hex.length === 8) argb = hex;
  else return undefined;
  argb = argb.toLowerCase();
  return `#${argb.slice(2)}${argb.slice(0, 2)}`;
}

function convertFlutter(object: JsonObject, diagnostics: Diagnostic[]): JsonObject {
  const mode = object.appearance === "dark" ? "dark" : "light";
  const colors = isJsonObject(object.colors) ? object.colors : {};
  const group: JsonObject = {};
  for (const { from: source, to: targets } of REGISTRY.flutterAdapter.colorMap) {
    const raw = getPath(colors, source);
    if (raw === undefined) continue;
    const hex = typeof raw === "string" ? flutterHex(raw) : undefined;
    if (hex === undefined) {
      diagnostics.push({
        path: `colors.${source}`,
        kind: "invalidValue",
        message: `Flutter 颜色非法：${jsonDisplay(raw)}，已忽略`,
      });
      continue;
    }
    for (const target of targets) group[target.slice("colors.".length)] = hex;
  }
  const meta: JsonObject = {};
  for (const key of REGISTRY.flutterAdapter.metaKeys) {
    const value = object[key];
    if (typeof value === "string") meta[key] = value;
  }
  const output: JsonObject = { format: THEME_FORMAT, schemaVersion: THEME_SCHEMA_VERSION };
  if (Object.keys(meta).length > 0) output.meta = meta;
  output.extends = REGISTRY.flutterAdapter.extends;
  if (Object.keys(group).length > 0) output[mode] = { colors: group };
  diagnostics.push({
    path: "format",
    kind: "migrated",
    message: `已从 Flutter FluxThemeJson（${mode}）转换；Flutter 专有颜色与 metrics 未转换`,
  });
  return output;
}

function hasSnakeCaseKey(map: JsonObject): boolean {
  return Object.entries(map).some(([key, value]) => key.includes("_") || (isJsonObject(value) && hasSnakeCaseKey(value)));
}

/** 是否形如 v1 内部格式：无 `format`，`schemaVersion` 为 1，或缺失但 `light` / `dark` 含 snake_case 键。 */
export function isV1(object: JsonObject): boolean {
  if (Object.hasOwn(object, "format")) return false;
  const layers = ["light", "dark"].map((key) => object[key]).filter(isJsonObject);
  if (!layers.some((layer) => V1_GROUPS.some((group) => Object.hasOwn(layer, group)))) return false;
  if (Object.hasOwn(object, "schemaVersion")) return object.schemaVersion === 1;
  return layers.some(hasSnakeCaseKey);
}

/** 把 `from` 处的值移到 `to`（目标已存在时不覆盖）；移走后清理空父对象。 */
function renamePath(layer: JsonObject, from: string, to: string): void {
  if (getPath(layer, to) !== undefined) return;
  const value = takePath(layer, from);
  if (value !== undefined) insertPath(layer, to, value);
}

function takePath(map: JsonObject, path: string): Json | undefined {
  const dot = path.indexOf(".");
  if (dot < 0) {
    if (!Object.hasOwn(map, path)) return undefined;
    const value = map[path];
    delete map[path];
    return value;
  }
  const head = path.slice(0, dot);
  const child = map[head];
  if (!isJsonObject(child)) return undefined;
  const value = takePath(child, path.slice(dot + 1));
  if (value !== undefined && Object.keys(child).length === 0) delete map[head];
  return value;
}

/** 与 `setPath` 不同：中间节点已存在但不是对象时放弃写入（同 Rust `insert_path`）。 */
function insertPath(map: JsonObject, path: string, value: Json): void {
  const dot = path.indexOf(".");
  if (dot < 0) {
    map[path] = value;
    return;
  }
  const head = path.slice(0, dot);
  if (!Object.hasOwn(map, head)) map[head] = {};
  const child = map[head];
  if (isJsonObject(child)) insertPath(child, path.slice(dot + 1), value);
}

/** `{ color, offset: {x, y}, blur_radius, spread_radius, inset }` → `{ x, y, blur, spread, color, inset }`。 */
function migrateV1Shadow(level: JsonObject): JsonObject {
  const output: JsonObject = {};
  const x = getPath(level, "offset.x");
  if (x !== undefined) output.x = x;
  const y = getPath(level, "offset.y");
  if (y !== undefined) output.y = y;
  for (const [from, to] of [
    ["blur_radius", "blur"],
    ["spread_radius", "spread"],
    ["color", "color"],
    ["inset", "inset"],
  ]) {
    if (Object.hasOwn(level, from)) output[to] = level[from];
  }
  return output;
}

function migrateV1Layer(layer: JsonObject): JsonObject {
  for (const { from, to } of REGISTRY.migrations.v1KeyRenames) renamePath(layer, from, to);
  const shadow = layer.shadow;
  if (isJsonObject(shadow)) {
    for (const levels of Object.values(shadow)) {
      if (!Array.isArray(levels)) continue;
      levels.forEach((level, index) => {
        if (isJsonObject(level)) levels[index] = migrateV1Shadow(level);
      });
    }
  }
  return layer;
}

/** v1 → v2：`name` / `author` 移入 `meta`，两份快照键改名、阴影改为扁平字段，基底取 `builtin:default`。 */
function migrateV1(object: JsonObject, diagnostics: Diagnostic[]): JsonObject {
  const output: JsonObject = { format: THEME_FORMAT, schemaVersion: THEME_SCHEMA_VERSION };
  const meta: JsonObject = {};
  const rest: JsonObject = {};
  for (const [key, value] of Object.entries(object)) {
    if (key === "schemaVersion") continue;
    if (key === "name" || key === "author") {
      if (value !== null) meta[key] = value;
    } else if (key === "light" || key === "dark") {
      rest[key] = isJsonObject(value) ? migrateV1Layer(value) : value;
    } else {
      rest[key] = value;
    }
  }
  if (Object.keys(meta).length > 0) output.meta = meta;
  output.extends = "builtin:default";
  Object.assign(output, rest);
  diagnostics.push({ path: "schemaVersion", kind: "migrated", message: "已从 v1 内部格式迁移到 v2" });
  return output;
}

/** 按 `registry.migrations.steps` 把 `fromVersion` 之后的步骤依次作用到一个 token 层。 */
function applySteps(layer: JsonObject, fromVersion: number): void {
  for (const step of REGISTRY.migrations.steps) {
    if (step.toVersion <= fromVersion) continue;
    for (const { from, to } of step.renames) renamePath(layer, from, to);
  }
}

function stringField(path: string, value: Json, diagnostics: Diagnostic[]): string | undefined {
  if (typeof value === "string") return value;
  diagnostics.push({ path, kind: "invalidValue", message: `须为字符串：${jsonDisplay(value)}，已忽略` });
  return undefined;
}

function parseMeta(value: Json, diagnostics: Diagnostic[]): ThemeMeta | undefined {
  if (!isJsonObject(value)) {
    diagnostics.push({ path: "meta", kind: "invalidValue", message: "meta 须为对象，已忽略" });
    return undefined;
  }
  const meta: ThemeMeta = { extra: {} };
  for (const [key, field] of Object.entries(value)) {
    const path = `meta.${key}`;
    if ((META_KEYS as readonly string[]).includes(key)) {
      meta[key as MetaKey] = stringField(path, field, diagnostics);
    } else {
      diagnostics.push({ path, kind: "unknownKey", message: "未知 meta 键，已原样保留" });
      meta.extra[key] = field;
    }
  }
  return meta;
}

/** 路径 `prefix` 是否是某个 token 的祖先（中间对象）。 */
export function isTokenPrefix(prefix: string): boolean {
  return TOKENS.some((spec) => spec.path.startsWith(`${prefix}.`));
}

/** 未知键 / 结构错误诊断（值本身在 resolve 时校验）。 */
function checkLayer(layer: string, map: JsonObject, prefix: string, diagnostics: Diagnostic[]): void {
  for (const [key, value] of Object.entries(map)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (tokenSpec(path)) continue;
    if (!isTokenPrefix(path)) {
      diagnostics.push({ path: `${layer}.${path}`, kind: "unknownKey", message: "未知 token，已原样保留" });
    } else if (isJsonObject(value)) {
      checkLayer(layer, value, path, diagnostics);
    } else {
      diagnostics.push({
        path: `${layer}.${path}`,
        kind: "invalidValue",
        message: `须为对象：${jsonDisplay(value)}，已忽略`,
      });
    }
  }
}

/** 解析已 `JSON.parse` 的值。 */
export function themeFromValue(value: unknown): ParseResult {
  if (!isJsonObject(value as Json)) {
    return { ok: false, error: { kind: "notAnObject", message: "主题文件顶层必须是 JSON 对象" } };
  }
  let object = structuredClone(value as JsonObject);
  const diagnostics: Diagnostic[] = [];
  if (object.format !== THEME_FORMAT) {
    if (isFlutterTheme(object)) {
      object = convertFlutter(object, diagnostics);
    } else if (Object.hasOwn(object, "format")) {
      return {
        ok: false,
        error: { kind: "unsupportedFormat", message: `不支持的主题格式：${jsonDisplay(object.format)}` },
      };
    } else if (isV1(object)) {
      object = migrateV1(object, diagnostics);
    }
  }

  const document = emptyDocument();
  let version = THEME_SCHEMA_VERSION;
  for (const [key, field] of Object.entries(object)) {
    switch (key) {
      case "$schema":
        document.schema = stringField(key, field, diagnostics);
        break;
      case "format":
        document.format = stringField(key, field, diagnostics);
        break;
      case "schemaVersion":
        if (typeof field === "number" && Number.isInteger(field) && field >= 0 && field <= 0xffff_ffff) {
          version = field;
        } else {
          diagnostics.push({
            path: key,
            kind: "invalidValue",
            message: `schemaVersion 须为正整数：${jsonDisplay(field)}，按 ${THEME_SCHEMA_VERSION} 处理`,
          });
        }
        break;
      case "meta":
        document.meta = parseMeta(field, diagnostics);
        break;
      case "extends":
        document.extends = stringField(key, field, diagnostics);
        break;
      case "tokens":
      case "dark":
      case "light":
        if (!isJsonObject(field)) {
          diagnostics.push({ path: key, kind: "invalidValue", message: "token 层须为对象，已忽略" });
          break;
        }
        checkLayer(key, field, "", diagnostics);
        document[key] = field;
        break;
      default:
        diagnostics.push({ path: key, kind: "unknownKey", message: "未知顶层键，已原样保留" });
        document.extra[key] = field;
    }
  }

  if (version > THEME_SCHEMA_VERSION) {
    diagnostics.push({
      path: "schemaVersion",
      kind: "newerVersion",
      message: `文件 schemaVersion ${version} 高于支持的 ${THEME_SCHEMA_VERSION}，已尽力加载`,
    });
  } else if (version < THEME_SCHEMA_VERSION) {
    for (const layer of [document.tokens, document.dark, document.light]) applySteps(layer, version);
    version = THEME_SCHEMA_VERSION;
  }
  document.schemaVersion = version;
  document.format = THEME_FORMAT;
  return { ok: true, document, diagnostics };
}

/**
 * 解析主题 JSON 文本（含 v1 内部格式迁移与 Flutter FluxThemeJson 适配）。
 * 非 JSON、顶层非对象，或 `format` 既不是本格式也不是可识别的 Flutter 主题时整体拒绝。
 */
export function parseTheme(text: string): ParseResult {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch (error) {
    return {
      ok: false,
      error: { kind: "invalidJson", message: `主题文件不是合法 JSON：${error instanceof Error ? error.message : String(error)}` },
    };
  }
  return themeFromValue(value);
}
