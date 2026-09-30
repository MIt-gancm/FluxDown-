import { describe, expect, test } from "bun:test";
import { isSafeLocalPath } from "../src/lib/return-to";

describe("isSafeLocalPath", () => {
  test("接受站内绝对路径", () => {
    expect(isSafeLocalPath("/pricing")).toBe(true);
    expect(isSafeLocalPath("/docs/a?b=1#c")).toBe(true);
  });
  test("拒绝跨域形态", () => {
    for (const raw of ["//evil.com", "/\\evil.com", "/\t/evil.com", "/\n/evil.com", "https://evil.com", "evil.com", "", null]) {
      expect(isSafeLocalPath(raw)).toBe(false);
    }
  });
});
