/**
 * token 注册表：`registry.json` 由 `cargo run -p fluxdown_ui_theme --example gen_theme_registry`
 * 从 Rust `crates/theme/src/registry.rs` 生成（唯一事实源），这里只做类型化与索引。
 */
import raw from "./registry.json";
import type { Json } from "./types";

export type TokenKind = "color" | "length" | "radius" | "fontFamily" | "fontWeight" | "number" | "shadow";

export interface ByMode<T> {
  dark: T;
  light: T;
}

export type DefaultExpr =
  | { kind: "base" }
  | { kind: "ref"; path: string }
  | { kind: "refAlpha"; path: string; alpha: number }
  | { kind: "refOffset"; path: string; offset: number }
  | { kind: "mix"; from: string; to: string; amount: ByMode<number> }
  | { kind: "contrast"; color: string; against: string; min: number }
  | { kind: "literal"; value: ByMode<string | number> }
  | { kind: "byMode"; dark: DefaultExpr; light: DefaultExpr };

export interface TokenSpec {
  path: string;
  group: string;
  kind: TokenKind;
  default: DefaultExpr;
  range: { min: number; max: number } | null;
  since: number;
  modeDependent: boolean;
  kitBound: boolean;
  scales: boolean;
}

export interface Registry {
  format: string;
  schemaVersion: number;
  schemaUrl: string;
  groups: string[];
  extends: string[];
  defaultExtends: string;
  radiusFullThreshold: number;
  fontSentinels: Record<string, string>;
  /** 运行时用户强调色覆盖的 token 路径；仅差异导出永不删除这些路径上的字面量。 */
  accentTokenPaths: string[];
  tokens: TokenSpec[];
  builtins: Record<string, ByMode<Record<string, Json>>>;
  /** Flutter FluxThemeJson → v2 的适配表（镜像 Rust `flutter.rs`）。 */
  flutterAdapter: {
    appearances: string[];
    extends: string;
    metaKeys: string[];
    colorMap: { from: string; to: string[] }[];
  };
  migrations: {
    v1KeyRenames: { from: string; to: string }[];
    steps: { toVersion: number; renames: { from: string; to: string }[] }[];
  };
}

export const REGISTRY = raw as unknown as Registry;
export const TOKENS: readonly TokenSpec[] = REGISTRY.tokens;
export const GROUPS: readonly string[] = REGISTRY.groups;
export const EXTENDS_VALUES: readonly string[] = REGISTRY.extends;
export const ACCENT_TOKEN_PATHS: ReadonlySet<string> = new Set(REGISTRY.accentTokenPaths);

const BY_PATH = new Map(TOKENS.map((spec) => [spec.path, spec]));

export function tokenSpec(path: string): TokenSpec | undefined {
  return BY_PATH.get(path);
}
