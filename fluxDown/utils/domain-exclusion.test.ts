import { describe, expect, test } from "bun:test";
import {
  isDomainExcluded,
  normalizeDomain,
  normalizeDomainList,
} from "./domain-exclusion";

describe("normalizeDomain", () => {
  test("strips scheme, path, wildcard and case", () => {
    expect(normalizeDomain("https://Example.COM/a/b")).toBe("example.com");
    expect(normalizeDomain("*.example.com")).toBe("example.com");
    expect(normalizeDomain("  example.com  ")).toBe("example.com");
  });

  test("rejects empty input", () => {
    expect(normalizeDomain("   ")).toBeNull();
  });

  test("dedupes list after normalization", () => {
    expect(
      normalizeDomainList(["Example.com", "https://example.com/", "a.org"]),
    ).toEqual(["example.com", "a.org"]);
  });
});

describe("isDomainExcluded", () => {
  test("matches on DNS label boundary only", () => {
    expect(isDomainExcluded("microsoft.com", ["t.co"])).toBe(false);
    expect(isDomainExcluded("t.co", ["t.co"])).toBe(true);
    expect(isDomainExcluded("www.example.com", ["example.com"])).toBe(true);
    expect(isDomainExcluded("badexample.com", ["example.com"])).toBe(false);
  });
});
