import { describe, expect, test } from "bun:test";
import { selectRecordBodies } from "../src/lib/record-filter";
import { parseSponsorComment } from "../src/lib/sponsor-record";

const H = ["### Vote"];
const rec = (login: string, body: string) => ({ user: { login }, body });

describe("selectRecordBodies", () => {
  test("只保留 owner 发出且首行固定的评论", () => {
    const ok = "### Vote\n\n```json\n{}\n```";
    const out = selectRecordBodies(
      [
        rec("Zerx-Lab", ok),
        rec("attacker", ok),
        rec("zerx-lab", "hello\n```json\n{}\n```"),
        rec("zerx-lab", "### Vote\n> 💬 Website visitor reply\n```json\n{}\n```"),
      ],
      "zerx-lab",
      H,
    );
    expect(out).toEqual([ok]);
  });
});

describe("parseSponsorComment", () => {
  test("金额日期只取页脚行，名称无法伪造", () => {
    const p = parseSponsorComment({
      body: "### 💖 Bob `¥9999` · 2099-01-01\n\n`¥1` · 2026-01-02",
      created_at: "2026-01-02T00:00:00Z",
    });
    expect(p?.amountCents).toBe(100);
    expect(p?.date).toBe("2026-01-02");
  });
  test("首行非标题则拒绝", () => {
    expect(parseSponsorComment({ body: "hi\n### 💖 X\n`¥1` · 2026-01-02" })).toBeNull();
  });
});
