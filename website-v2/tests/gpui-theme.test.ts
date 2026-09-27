/**
 * GPUI 主题两端一致性：解析 + resolve `crates/theme/tests/fixtures/*.json`，与 Rust 生成的
 * `*.resolved.json` 逐项比对（含诊断 path / kind / message 及顺序）；导出往返。
 */
import { describe, expect, test } from "bun:test";
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { exportTheme, exportThemeValue } from "../src/lib/gpui-theme/export";
import { parseTheme } from "../src/lib/gpui-theme/parse";
import { flatTokens, resolveTheme, resolvedSnapshot } from "../src/lib/gpui-theme/resolve";
import type { ThemeDocument } from "../src/lib/gpui-theme/types";

const FIXTURES = join(import.meta.dir, "../../crates/theme/tests/fixtures");
const sources = readdirSync(FIXTURES)
  .filter((name) => name.endsWith(".json") && !name.endsWith(".resolved.json"))
  .sort();
const fixture = (name: string) => readFileSync(join(FIXTURES, name), "utf8");

function parsed(name: string): ThemeDocument {
  const result = parseTheme(fixture(name));
  if (!result.ok) throw new Error(result.error.message);
  return result.document;
}

const resolvedBoth = (document: ThemeDocument) => ({
  dark: flatTokens(resolveTheme(document, "dark").tokens),
  light: flatTokens(resolveTheme(document, "light").tokens),
});

test("fixtures present", () => {
  expect(sources.length).toBeGreaterThanOrEqual(11);
});

describe("resolved snapshot matches Rust", () => {
  for (const name of sources) {
    test(name, () => {
      const expected = JSON.parse(fixture(name.replace(/\.json$/, ".resolved.json")));
      const actual = resolvedSnapshot(fixture(name));
      expect(actual.diagnostics).toEqual(expected.diagnostics);
      expect(actual.dark).toEqual(expected.dark);
      expect(actual.light).toEqual(expected.light);
      // 键序同样一致（注册表顺序）。
      expect(Object.keys(actual.dark ?? {})).toEqual(Object.keys(expected.dark));
    });
  }
});

describe("export round trip", () => {
  for (const name of sources) {
    test(`${name}: diff / full exports re-resolve identically and are idempotent`, () => {
      const document = parsed(name);
      const original = resolvedBoth(document);
      for (const mode of ["diff", "full"] as const) {
        const text = exportTheme(document, mode);
        const reparsed = parseTheme(text);
        if (!reparsed.ok) throw new Error(reparsed.error.message);
        expect(resolvedBoth(reparsed.document)).toEqual(original);
        expect(exportTheme(reparsed.document, mode)).toBe(text);
      }
    });
  }

  test("full export of v2-full.json is byte-identical to the file", () => {
    expect(exportTheme(parsed("v2-full.json"), "full")).toBe(fixture("v2-full.json"));
  });

  test("diff of v2-full reduces to v2-minimal-diff plus pinned accent paths", () => {
    const full = exportThemeValue(parsed("v2-full.json"), "diff");
    const minimal = exportThemeValue(parsed("v2-minimal-diff.json"), "diff");
    // 完整导出把强调色路径写成字面量（固定颜色），diff 精简保留它们，其余与最小文件一致。
    expect(full.dark).toEqual({
      colors: {
        primary: "#5e81acff",
        primaryForeground: "#2e3440ff",
        accent: "#88c0d026",
        accentForeground: "#88c0d0ff",
        ring: "#88c0d0ff",
      },
    });
    expect(full.light).toEqual({
      colors: {
        primary: "#3b82f6ff",
        primaryForeground: "#ffffffff",
        accent: "#3b82f61a",
        accentForeground: "#3b82f6ff",
        ring: "#3b82f6ff",
      },
    });
    expect(minimal.dark).toEqual({ colors: { primary: "#5e81acff" } });
    for (const diff of [full, minimal]) {
      delete diff.meta;
      delete diff.dark;
      delete diff.light;
    }
    expect(full).toEqual(minimal);
  });

  test("diff export keeps accent literals equal to the base", () => {
    const result = parseTheme(`{
  "format": "fluxdown.gpui-theme",
  "schemaVersion": 2,
  "extends": "builtin:default",
  "tokens": { "colors": { "primary": "#3B82F6", "ring": "#3b82f6ff" } },
  "dark": { "colors": { "background": "#1c1c1e", "primaryForeground": "#ffffff", "accentForeground": "#3b82f6" } }
}`);
    if (!result.ok) throw new Error(result.error.message);
    expect(result.diagnostics).toEqual([]);
    const exported = exportThemeValue(result.document, "diff");
    expect(exported.tokens).toEqual({ colors: { primary: "#3b82f6ff", ring: "#3b82f6ff" } });
    expect(exported.dark).toEqual({ colors: { primaryForeground: "#ffffffff", accentForeground: "#3b82f6ff" } });
  });

  test("missing fields export only the override, normalized", () => {
    const exported = exportThemeValue(parsed("missing-fields.json"), "diff");
    expect(exported.tokens).toEqual({ colors: { primary: "#10b981ff" } });
    expect(exported.light).toBeUndefined();
    expect(exported.schemaVersion).toBe(2);
  });

  test("unknown keys survive and sort after known keys", () => {
    const document = parsed("unknown-keys.json");
    const text = exportTheme(document, "diff");
    const reparsed = parseTheme(text);
    if (!reparsed.ok) throw new Error(reparsed.error.message);
    expect(Object.keys(reparsed.document.tokens)).toEqual(["colors", "typography", "motion"]);
    const exported = exportThemeValue(reparsed.document, "diff");
    expect(exported["x-editor"]).toEqual({ lastOpened: "colors", zoom: 2 });
    expect(Object.keys(exported.tokens as object)).toEqual(["colors", "typography", "motion"]);
    expect(Object.keys((exported.tokens as { colors: object }).colors)).toEqual(["primary", "sparkle"]);
  });

  test("v1 export keeps only values differing from builtin:default", () => {
    const exported = exportThemeValue(parsed("v1-internal.json"), "diff");
    expect((exported.dark as { radius: unknown }).radius).toEqual({ md: 8 });
  });
});

test("rejects non-objects and foreign formats", () => {
  expect(parseTheme("[]")).toMatchObject({ ok: false, error: { kind: "notAnObject" } });
  expect(parseTheme("{")).toMatchObject({ ok: false, error: { kind: "invalidJson" } });
  expect(parseTheme('{"format":"vscode-theme"}')).toMatchObject({ ok: false, error: { kind: "unsupportedFormat" } });
});
