/**
 * 页脚「FluxDown」巨型字标的粒子版:把字标按页面实际排版栅格化后采样成粒子。
 *
 * - 首次进入视口:粒子从下方散落处汇流成字。
 * - 轮廓粒子较亮(延续原描边字标的观感),字腔内部是淡淡的强调色填充;自下而上渐隐,同原遮罩。
 * - 指针:附近粒子被推开并绕指针旋转(弹簧回位),划过时被「点燃」成强调色 / cyan 后慢慢冷却;
 *   快速划过会带起一阵风。
 * - 在空白处按下:冲击波炸散附近粒子,随后重新聚合。
 * - 空闲时随机零星闪烁。
 */
import { StrokeBatch, smoothstep, type Palette, type Pointer, type Scene } from "../../scripts/particle-stage";

interface Grain {
  hx: number;
  hy: number;
  x: number;
  y: number;
  vx: number;
  vy: number;
  heat: number;
  edge: boolean;
  cyan: boolean;
  /** 自下而上渐隐系数(按归位坐标,0..1) */
  fade: number;
}

const REPEL_R = 110;
const BLAST_R = 300;
const SPRING = 60;

export class WordmarkField implements Scene {
  private grains: Grain[] = [];
  private width = 0;
  private height = 0;
  private introPending = true;

  private readonly edgeBatch = new StrokeBatch(1.8);
  private readonly fillBatch = new StrokeBatch(1.5);
  private readonly hotBatch = new StrokeBatch(2.6);

  constructor(
    private readonly text: HTMLElement,
    private readonly canvas: HTMLCanvasElement,
  ) {}

  resize(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.sample();
  }

  /** 按字标 span 的实际字体与位置栅格化并采样(字体加载完成后需重采)。 */
  sample() {
    const { width, height } = this;
    const node = this.text.firstChild;
    if (!width || !height || !node) return;
    const style = getComputedStyle(this.text);
    const size = Number.parseFloat(style.fontSize) || 200;
    const range = document.createRange();
    range.selectNodeContents(node);
    const box = range.getBoundingClientRect();
    const origin = this.canvas.getBoundingClientRect();

    const off = document.createElement("canvas");
    off.width = Math.ceil(width);
    off.height = Math.ceil(height);
    const ctx = off.getContext("2d", { willReadFrequently: true });
    if (!ctx) return;
    ctx.font = `${style.fontWeight} ${size}px ${style.fontFamily}`;
    if ("letterSpacing" in ctx) ctx.letterSpacing = style.letterSpacing === "normal" ? "0px" : style.letterSpacing;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillStyle = "#000";
    ctx.fillText(this.text.textContent ?? "", box.left - origin.left + box.width / 2, box.top - origin.top + box.height / 2);
    const data = ctx.getImageData(0, 0, off.width, off.height).data;
    const inside = (x: number, y: number) => {
      const ix = Math.round(x);
      const iy = Math.round(y);
      if (ix < 0 || iy < 0 || ix >= off.width || iy >= off.height) return false;
      return (data[(iy * off.width + ix) * 4 + 3] ?? 0) > 110;
    };

    const step = Math.max(4, Math.round(size / 42));
    const probe = step * 0.75;
    const grains: Grain[] = [];
    for (let y = step / 2; y < height; y += step) {
      for (let x = step / 2; x < width; x += step) {
        if (!inside(x, y)) continue;
        const edge = !inside(x - probe, y) || !inside(x + probe, y) || !inside(x, y - probe) || !inside(x, y + probe);
        grains.push({
          hx: x,
          hy: y,
          x,
          y,
          vx: 0,
          vy: 0,
          heat: 0,
          edge,
          cyan: Math.random() < 0.15 + 0.6 * (x / width),
          fade: 1 - smoothstep(0.3 * height, height, y),
        });
      }
    }
    this.grains = grains;
  }

  press(x: number, y: number) {
    for (const g of this.grains) {
      const dx = g.x - x;
      const dy = g.y - y;
      const d = Math.hypot(dx, dy) || 1;
      if (d > BLAST_R) continue;
      const k = 1 - d / BLAST_R;
      g.vx += (dx / d) * 1500 * k;
      g.vy += (dy / d) * 1500 * k;
      g.heat = Math.max(g.heat, k);
    }
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    if (!this.grains.length) return;
    if (dt > 0 && this.introPending) {
      // 汇流入场:从下方散落处飞回字形
      this.introPending = false;
      for (const g of this.grains) {
        g.x = g.hx + (Math.random() - 0.5) * this.width * 0.5;
        g.y = this.height + 20 + Math.random() * 160;
        g.vx = g.vy = 0;
      }
    }

    const damp = Math.exp(-7 * dt);
    const cool = Math.exp(-1.4 * dt);
    if (dt > 0) {
      // 空闲闪烁
      for (let i = 0; i < 3; i++) {
        const g = this.grains[Math.floor(Math.random() * this.grains.length)];
        if (g) g.heat = Math.max(g.heat, 0.55);
      }
      for (const g of this.grains) {
        let ax = (g.hx - g.x) * SPRING;
        let ay = (g.hy - g.y) * SPRING;
        if (p.presence > 0.01) {
          const dx = g.x - p.x;
          const dy = g.y - p.y;
          const d2 = dx * dx + dy * dy;
          if (d2 < REPEL_R * REPEL_R) {
            const d = Math.sqrt(d2) || 1;
            const f = (1 - d / REPEL_R) ** 2 * p.presence;
            ax += (dx / d) * 5200 * f - (dy / d) * 1900 * f + p.vx * f * 4;
            ay += (dy / d) * 5200 * f + (dx / d) * 1900 * f + p.vy * f * 4;
            g.heat = Math.max(g.heat, Math.min(1, f * 2.2));
          }
        }
        g.vx = (g.vx + ax * dt) * damp;
        g.vy = (g.vy + ay * dt) * damp;
        g.x += g.vx * dt;
        g.y += g.vy * dt;
        g.heat *= cool;
      }
    }

    const tone = pal.dark ? 1 : 0.9;
    for (const g of this.grains) {
      const moving = Math.abs(g.vx) + Math.abs(g.vy) > 30;
      const x0 = moving ? g.x - g.vx * 0.016 : g.x;
      const y0 = moving ? g.y - g.vy * 0.016 : g.y;
      const x1 = moving ? g.x : g.x + 0.01;
      // 飞散中的粒子不受渐隐影响太多,保证入场与炸散可见
      const fade = Math.max(g.fade, moving ? 0.5 : 0);
      if (g.heat > 0.35) {
        this.hotBatch.line(pal, g.cyan ? "cyan" : "accent", (0.45 + 0.55 * g.heat) * fade * tone, x0, y0, x1, g.y);
      } else if (g.edge) {
        this.edgeBatch.line(pal, g.heat > 0.12 ? "accent" : "subtle", (0.6 + 0.4 * g.heat) * fade * tone, x0, y0, x1, g.y);
      } else {
        this.fillBatch.line(pal, g.cyan ? "cyan" : "accent", (0.14 + 0.6 * g.heat) * fade * tone, x0, y0, x1, g.y);
      }
    }
    this.fillBatch.flush(ctx);
    this.edgeBatch.flush(ctx);
    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    this.hotBatch.flush(ctx);
    ctx.globalCompositeOperation = "source-over";
  }
}
