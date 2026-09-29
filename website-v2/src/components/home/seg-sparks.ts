/**
 * 分段条的粒子层:叠在 Engine 可视化的 `.seg-bar` 上(画布四周略微出血)。
 *
 * - 每个在传分段的写入头持续向已下载区喷出火花,喷射率随该连接速度;慢连接的火花偏暗。
 * - 新的拆分事件:在拆分点纵向迸出一簇火花。
 * - 指针:附近火花被卷向指针;在条上悬停时,竖线探针标出指针所指字节所属分段,
 *   描出该分段的范围并在条上方标注连接号与速度;指针靠近写入头时喷射加倍。
 * - 在空白处按下:原地迸出一圈火花。
 */
import { StrokeBatch, rgba, type Palette, type Pointer, type Scene } from "../../scripts/particle-stage";
import { formatBytes, type SegmentSim } from "./segment-sim";

interface Spark {
  x: number;
  y: number;
  vx: number;
  vy: number;
  age: number;
  life: number;
  color: "accent" | "cyan" | "fg" | "subtle";
}

const MB = 1024 * 1024;
const MAX_SPARKS = 320;
const PULL_R = 90;
const TAU = Math.PI * 2;

export class SegSparks implements Scene {
  private sparks: Spark[] = [];
  private seenSim: SegmentSim | null = null;
  private seenSplits = 0;
  private barX = 0;
  private barY = 0;
  private barW = 0;
  private barH = 0;
  private font = "10.5px ui-monospace, monospace";

  private readonly streak = new StrokeBatch(1.3);
  private readonly hair = new StrokeBatch(0.8);

  constructor(
    private readonly getSim: () => SegmentSim,
    private readonly bar: HTMLElement,
    private readonly canvas: HTMLCanvasElement,
  ) {
    const mono = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
    if (mono) this.font = `10.5px ${mono}`;
  }

  resize() {
    const origin = this.canvas.getBoundingClientRect();
    const r = this.bar.getBoundingClientRect();
    this.barX = r.left - origin.left;
    this.barY = r.top - origin.top;
    this.barW = r.width;
    this.barH = r.height;
  }

  press(x: number, y: number) {
    this.burst(x, y, 22, 60, 220, "accent");
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    const sim = this.getSim();
    if (sim !== this.seenSim) {
      this.seenSim = sim;
      this.seenSplits = 0;
      this.sparks = [];
    }
    const { barX, barY, barW, barH } = this;
    if (barW <= 0) return;
    const tone = pal.dark ? 1 : 0.9;

    if (dt > 0) {
      for (const s of sim.segments) {
        if (s.pos >= s.end) continue;
        const hx = barX + (s.pos / sim.size) * barW;
        const near = p.presence * Math.max(0, 1 - Math.hypot(hx - p.x, barY + barH / 2 - p.y) / 60);
        const rate = 22 * Math.min(1.6, s.speed / (5.2 * MB)) * (1 + 2 * near);
        const slow = s.base < 2 * MB;
        let n = rate * dt;
        while (n > 0 && this.sparks.length < MAX_SPARKS) {
          if (n < 1 && Math.random() > n) break;
          n -= 1;
          const a = Math.PI + (Math.random() - 0.5) * 1.3;
          const v = 40 + Math.random() * 110;
          this.sparks.push({
            x: hx,
            y: barY + 4 + Math.random() * (barH - 8),
            vx: Math.cos(a) * v,
            vy: Math.sin(a) * v,
            age: 0,
            life: 0.35 + Math.random() * 0.45,
            color: slow ? "subtle" : Math.random() < 0.55 ? "cyan" : "accent",
          });
        }
      }
      for (; this.seenSplits < sim.splits.length; this.seenSplits++) {
        const split = sim.splits[this.seenSplits];
        if (!split) continue;
        const x = barX + (split.offset / sim.size) * barW;
        for (const dir of [-1, 1]) this.burst(x, barY + (dir < 0 ? 0 : barH), 7, 50, 160, "fg", dir);
      }
    }

    const drag = Math.exp(-2.4 * dt);
    for (let i = this.sparks.length - 1; i >= 0; i--) {
      const s = this.sparks[i];
      if (!s) continue;
      s.age += dt;
      if (s.age >= s.life) {
        this.sparks[i] = this.sparks[this.sparks.length - 1] as Spark;
        this.sparks.pop();
        continue;
      }
      if (p.presence > 0.01) {
        const dx = p.x - s.x;
        const dy = p.y - s.y;
        const d = Math.hypot(dx, dy) || 1;
        if (d < PULL_R) {
          // 卷向指针:径向吸引 + 切向旋转
          const f = (1 - d / PULL_R) * p.presence;
          s.vx += ((dx / d) * 700 - (dy / d) * 500) * f * dt;
          s.vy += ((dy / d) * 700 + (dx / d) * 500) * f * dt;
        }
      }
      s.vx *= drag;
      s.vy *= drag;
      s.x += s.vx * dt;
      s.y += s.vy * dt;
    }

    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    for (const s of this.sparks) {
      const a = (1 - s.age / s.life) * tone;
      this.streak.line(pal, s.color, a, s.x - s.vx * 0.035, s.y - s.vy * 0.035, s.x, s.y);
    }
    this.streak.flush(ctx);
    ctx.globalCompositeOperation = "source-over";

    this.drawProbe(ctx, sim, p, pal, tone);
  }

  private burst(x: number, y: number, count: number, vMin: number, vMax: number, color: Spark["color"], dirY = 0) {
    for (let i = 0; i < count && this.sparks.length < MAX_SPARKS; i++) {
      const a = dirY === 0 ? Math.random() * TAU : (dirY < 0 ? -Math.PI / 2 : Math.PI / 2) + (Math.random() - 0.5) * 1.2;
      const v = vMin + Math.random() * (vMax - vMin);
      this.sparks.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, age: 0, life: 0.5 + Math.random() * 0.4, color });
    }
  }

  /** 悬停探针:指针所指字节属于哪个分段、哪条连接、多快。 */
  private drawProbe(ctx: CanvasRenderingContext2D, sim: SegmentSim, p: Pointer, pal: Palette, tone: number) {
    const { barX, barY, barW, barH } = this;
    if (p.presence < 0.05 || p.x < barX || p.x > barX + barW || p.y < barY - 24 || p.y > barY + barH + 24) return;
    const offset = ((p.x - barX) / barW) * sim.size;
    const seg = sim.segments.find((s) => offset >= s.start && offset < s.end);
    const a = p.presence * tone;

    this.hair.line(pal, "fg", 0.55 * a, p.x, barY - 4, p.x, barY + barH + 4);
    if (seg) {
      const x0 = barX + (seg.start / sim.size) * barW;
      const x1 = barX + (seg.end / sim.size) * barW;
      const color = seg.pos >= seg.end ? "subtle" : "cyan";
      this.hair.line(pal, color, 0.9 * a, x0, barY - 3, x1, barY - 3);
      this.hair.line(pal, color, 0.9 * a, x0, barY + barH + 3, x1, barY + barH + 3);
      this.hair.line(pal, color, 0.9 * a, x0, barY - 3, x0, barY + barH + 3);
      this.hair.line(pal, color, 0.9 * a, x1, barY - 3, x1, barY + barH + 3);
    }
    this.hair.flush(ctx);
    if (!seg) return;

    const id = `C${String(seg.worker + 1).padStart(2, "0")}`;
    const text = seg.pos >= seg.end ? `${id} · ✓ ${formatBytes(seg.end - seg.start)}` : `${id} · ${formatBytes(seg.speed)}/s`;
    ctx.font = this.font;
    ctx.textBaseline = "bottom";
    const w = ctx.measureText(text).width;
    const tx = Math.min(barX + barW - w, Math.max(barX, p.x - w / 2));
    ctx.fillStyle = rgba(seg.pos >= seg.end ? pal.subtle : pal.accent, a);
    ctx.fillText(text, tx, barY - 7);
  }
}
