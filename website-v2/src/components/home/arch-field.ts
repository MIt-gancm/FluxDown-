/**
 * 原生架构的粒子爆炸图:四层等距平面(界面 / 网关 / 守护进程 / 引擎),自下而上绘制。
 *
 * - 可读性:每层是半实心的平面(页面底色遮挡下层)+ 实线边框,层间以虚线角柱相连;
 *   层名是屏幕对齐的引出标注,不随平面倾斜。当前层描强调色外框与光晕,其余层压暗。
 * - 每层内部是粒子纹样(窗口线框 / 广播环 / 队列行 / 芯片网格),各有节律动画。
 * - 滚动进度(`setProgress`)控制层间距;当前层(`setActive`)与列表悬停(`setFocus`)点亮平面。
 * - 数据包粒子在相邻层之间穿行:上行为事件(cyan),下行为指令(accent);抵达时回调 `onPulse`。
 * - 指针:整组平面朝指针轻微偏转、附近粒子被推开;悬停某层即点亮并回调 `onHover`;
 *   在平面上按下:该层泛起涟漪并向上下层迸发数据包。
 * - 正交投影:平面到屏幕是仿射变换,拾取(pick)对每层解 2×2 线性方程即可。
 */
import { StrokeBatch, rgba, smoothstep, type Palette, type Pointer, type Scene } from "../../scripts/particle-stage";

type Kind = "grid" | "deco";

interface Dot {
  u: number;
  v: number;
  kind: Kind;
  /** 纹样动画参数:行号 / 半径 / 引脚距离;-1 表示静态 */
  k: number;
}

interface Packet {
  u: number;
  v: number;
  from: number;
  to: number;
  t: number;
  speed: number;
}

interface Ripple {
  depth: number;
  u: number;
  v: number;
  age: number;
  size: number;
}

interface Mote {
  x: number;
  y: number;
  vy: number;
  phase: number;
}

const LAYERS = 4;
const BASE_YAW = (-42 * Math.PI) / 180;
const BASE_PITCH = (58 * Math.PI) / 180;
const PUSH_R = 72;
const MAX_PACKETS = 90;
const MAX_RIPPLES = 10;
const RIPPLE_LIFE = 1.4;
const UNIT_CORNERS = [
  [0, 0],
  [1, 0],
  [1, 1],
  [0, 1],
] as const;

function line(out: Dot[], u0: number, v0: number, u1: number, v1: number, step: number, k = -1) {
  const n = Math.max(1, Math.round(Math.hypot(u1 - u0, v1 - v0) / step));
  for (let i = 0; i <= n; i++) out.push({ u: u0 + ((u1 - u0) * i) / n, v: v0 + ((v1 - v0) * i) / n, kind: "deco", k });
}

function rect(out: Dot[], u0: number, v0: number, u1: number, v1: number, step: number) {
  line(out, u0, v0, u1, v0, step);
  line(out, u1, v0, u1, v1, step);
  line(out, u1, v1, u0, v1, step);
  line(out, u0, v1, u0, v0, step);
}

/** 按深度(0 = 引擎 … 3 = 界面)生成一层的点阵。 */
function buildLayer(depth: number): Dot[] {
  const dots: Dot[] = [];
  for (let i = 1; i < 10; i++) for (let j = 1; j < 10; j++) dots.push({ u: i / 10, v: j / 10, kind: "grid", k: -1 });
  const step = 1 / 44;
  if (depth === 3) {
    // 界面:窗口线框 + 标题栏 + 侧栏 + 任务行(k = 行号)
    rect(dots, 0.1, 0.1, 0.9, 0.82, step);
    line(dots, 0.1, 0.19, 0.9, 0.19, step);
    line(dots, 0.3, 0.19, 0.3, 0.82, step);
    for (let r = 0; r < 5; r++) line(dots, 0.36, 0.3 + r * 0.11, 0.84, 0.3 + r * 0.11, step, r);
  } else if (depth === 2) {
    // 网关:同心广播环(k = 半径)
    for (let r = 0.08; r < 0.4; r += 0.07) {
      const n = Math.round((Math.PI * 2 * r) / 0.021);
      for (let i = 0; i < n; i++) {
        const a = (i / n) * Math.PI * 2;
        dots.push({ u: 0.5 + Math.cos(a) * r, v: 0.46 + Math.sin(a) * r, kind: "deco", k: r });
      }
    }
  } else if (depth === 1) {
    // 守护进程:队列行(k = 行号)
    for (let r = 0; r < 9; r++) line(dots, 0.1, 0.14 + r * 0.075, 0.9, 0.14 + r * 0.075, 1 / 40, r);
  } else {
    // 引擎:芯片网格 + 四边引脚(k = 引脚离芯片的距离)
    rect(dots, 0.28, 0.24, 0.72, 0.66, step);
    for (let i = 1; i < 8; i++) for (let j = 1; j < 7; j++) dots.push({ u: 0.28 + i * 0.055, v: 0.24 + j * 0.06, kind: "deco", k: -1 });
    for (let i = 1; i < 6; i++) {
      const u = 0.28 + (i * 0.44) / 6;
      const v = 0.24 + (i * 0.42) / 6;
      for (let s = 1; s <= 4; s++) {
        const d = s * 0.022;
        dots.push({ u, v: 0.24 - d, kind: "deco", k: d });
        dots.push({ u, v: 0.66 + d, kind: "deco", k: d });
        dots.push({ u: 0.28 - d, v, kind: "deco", k: d });
        dots.push({ u: 0.72 + d, v, kind: "deco", k: d });
      }
    }
  }
  return dots;
}

export class ArchField implements Scene {
  /** 指针悬停的层变化时回调(null = 离开) */
  onHover: ((depth: number | null) => void) | null = null;
  /** 数据包抵达某层时回调;`up` = 上行事件 */
  onPulse: ((depth: number, up: boolean) => void) | null = null;

  private readonly layers: Dot[][] = Array.from({ length: LAYERS }, (_, d) => buildLayer(d));
  private readonly lit = new Float32Array(LAYERS);
  private packets: Packet[] = [];
  private ripples: Ripple[] = [];
  private motes: Mote[] = [];
  private width = 0;
  private height = 0;
  private size = 0;
  private progress = 1;
  private shownProgress = 1;
  private active = LAYERS - 1;
  private focus: number | null = null;
  private hover: number | null = null;
  private tiltX = 0;
  private tiltY = 0;
  private time = 0;
  private spawnClock = 0;
  private mono = "ui-monospace, monospace";
  /** 层名字号:窄屏缩小以保证标注不出界 */
  private labelPx = 12;

  // 当前帧的投影参数
  private cx = 0;
  private cy = 0;
  private cosY = 1;
  private sinY = 0;
  private cosP = 1;
  private sinP = 0;
  private gap = 0;

  private readonly gridBatch = new StrokeBatch(1.2);
  private readonly dotBatch = new StrokeBatch(2);
  private readonly hotBatch = new StrokeBatch(2.8);
  private readonly packetBatch = new StrokeBatch(2);

  constructor(private readonly labels: readonly string[]) {
    const mono = getComputedStyle(document.documentElement).getPropertyValue("--font-mono").trim();
    if (mono) this.mono = mono;
  }

  setProgress(p: number) {
    this.progress = p;
  }

  setActive(depth: number) {
    this.active = depth;
  }

  setFocus(depth: number | null) {
    this.focus = depth;
  }

  resize(width: number, height: number) {
    this.width = width;
    this.height = height;
    // 左侧按最长层名给引出标注留位(等宽字宽 ≈ 0.61em,另加引线与边距),平面居中于剩余区域;
    // 高度按完全展开时的包围盒(≈1.72·S)适配
    this.labelPx = width < 480 ? 10 : 12;
    const room = Math.max(...this.labels.map((l) => l.length)) * this.labelPx * 0.61 + 44;
    this.cx = (room + width - 12) / 2;
    this.size = Math.max(100, Math.min(300, (width - 12 - room) / 1.42, (height - 48) / 1.72));
    const target = Math.round(Math.min(70, (width * height) / 9000));
    while (this.motes.length < target) {
      this.motes.push({ x: Math.random() * width, y: Math.random() * height, vy: 8 + Math.random() * 16, phase: Math.random() * Math.PI * 2 });
    }
    this.motes.length = target;
  }

  press(x: number, y: number) {
    const hit = this.pick(x, y);
    if (!hit) return;
    this.ripple(hit.depth, hit.u, hit.v, 0.55);
    for (let i = 0; i < 16 && this.packets.length < MAX_PACKETS; i++) {
      const up = i % 2 === 0;
      const to = up ? hit.depth + 1 : hit.depth - 1;
      if (to < 0 || to >= LAYERS) continue;
      const u = Math.min(0.95, Math.max(0.05, hit.u + (Math.random() - 0.5) * 0.3));
      const v = Math.min(0.95, Math.max(0.05, hit.v + (Math.random() - 0.5) * 0.3));
      this.packets.push({ u, v, from: hit.depth, to, t: -Math.random() * 0.3, speed: 1.4 + Math.random() * 0.8 });
    }
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    const { width, height } = this;
    if (!width || !height) return;
    this.time += dt;
    this.shownProgress = dt > 0 ? this.shownProgress + (this.progress - this.shownProgress) * Math.min(1, dt * 6) : this.progress;

    // 朝指针偏转(平滑)
    this.tiltY += (p.presence * ((p.x - width / 2) / width) * 0.36 - this.tiltY) * Math.min(1, dt * 3);
    this.tiltX += (p.presence * ((p.y - height / 2) / height) * -0.2 - this.tiltX) * Math.min(1, dt * 3);
    const yaw = BASE_YAW + this.tiltY;
    const pitch = BASE_PITCH + this.tiltX;
    this.cosY = Math.cos(yaw);
    this.sinY = Math.sin(yaw);
    this.cosP = Math.cos(pitch);
    this.sinP = Math.sin(pitch);
    this.gap = this.size * (0.13 + 0.25 * this.shownProgress);
    this.cy = height / 2;

    const hovered = p.presence > 0.3 ? (this.pick(p.x, p.y)?.depth ?? null) : null;
    if (hovered !== this.hover) {
      this.hover = hovered;
      this.onHover?.(hovered);
    }
    const target = this.focus ?? this.hover ?? this.active;
    for (let d = 0; d < LAYERS; d++) {
      const want = d === target ? 1 : 0;
      const now = this.lit[d] ?? 0;
      this.lit[d] = dt > 0 ? now + (want - now) * Math.min(1, dt * 5) : want;
    }

    this.stepPackets(dt, p);
    for (const r of this.ripples) r.age += dt;
    this.ripples = this.ripples.filter((r) => r.age < RIPPLE_LIFE);

    const tone = pal.dark ? 1 : 0.9;
    this.drawMotes(dt, p, pal, tone);
    this.gridBatch.flush(ctx);

    for (let d = 0; d < LAYERS; d++) {
      if (d > 0) this.drawPillars(ctx, d - 1, pal, tone);
      this.drawLayer(ctx, d, p, pal, tone);
    }

    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    for (const k of this.packets) {
      if (k.t <= 0) continue;
      const z = k.from + (k.to - k.from) * smoothstep(0, 1, k.t);
      const tail = k.from + (k.to - k.from) * smoothstep(0, 1, Math.max(0, k.t - 0.16));
      const [x0, y0] = this.project(k.u, k.v, tail);
      const [x1, y1] = this.project(k.u, k.v, z);
      const a = Math.sin(Math.PI * Math.min(1, k.t)) * tone;
      this.packetBatch.line(pal, k.to > k.from ? "cyan" : "accent", a, x0, y0, x1, y1);
    }
    this.packetBatch.flush(ctx);
    ctx.globalCompositeOperation = "source-over";

    for (let d = 0; d < LAYERS; d++) this.drawLabel(ctx, d, pal, tone);
  }

  /** 平面坐标 (u, v) + 层高(以层为单位,可为小数)→ 屏幕坐标。 */
  private project(u: number, v: number, layer: number): [number, number] {
    const s = this.size;
    const X = (u - 0.5) * s;
    const Y = (v - 0.5) * s;
    const x1 = X * this.cosY - Y * this.sinY;
    const y1 = X * this.sinY + Y * this.cosY;
    const z = (layer - (LAYERS - 1) / 2) * this.gap;
    return [this.cx + x1, this.cy + y1 * this.cosP - z * this.sinP];
  }

  /** 自上而下找出指针所在的平面,返回平面坐标。 */
  private pick(x: number, y: number) {
    const s = this.size;
    // 屏幕 = 原点 + u·U + v·V(仿射),对每层解 2×2 线性方程
    const ux = s * this.cosY;
    const uy = s * this.sinY * this.cosP;
    const vx = -s * this.sinY;
    const vy = s * this.cosY * this.cosP;
    const det = ux * vy - vx * uy;
    if (Math.abs(det) < 1e-6) return null;
    for (let d = LAYERS - 1; d >= 0; d--) {
      const [ox, oy] = this.project(0, 0, d);
      const rx = x - ox;
      const ry = y - oy;
      const u = (rx * vy - vx * ry) / det;
      const v = (ux * ry - rx * uy) / det;
      if (u >= 0 && u <= 1 && v >= 0 && v <= 1) return { depth: d, u, v };
    }
    return null;
  }

  private ripple(depth: number, u: number, v: number, size: number) {
    if (this.ripples.length >= MAX_RIPPLES) this.ripples.shift();
    this.ripples.push({ depth, u, v, age: 0, size });
  }

  private stepPackets(dt: number, p: Pointer) {
    if (dt <= 0) return;
    this.spawnClock += dt * (5 + 9 * p.presence);
    while (this.spawnClock >= 1 && this.packets.length < MAX_PACKETS) {
      this.spawnClock -= 1;
      const from = Math.floor(Math.random() * LAYERS);
      const to = from === 0 ? 1 : from === LAYERS - 1 ? from - 1 : Math.random() < 0.5 ? from + 1 : from - 1;
      this.packets.push({ u: 0.15 + Math.random() * 0.7, v: 0.15 + Math.random() * 0.7, from, to, t: 0, speed: 0.7 + Math.random() * 0.6 });
    }
    this.spawnClock = Math.min(this.spawnClock, 1);
    for (let i = this.packets.length - 1; i >= 0; i--) {
      const k = this.packets[i];
      if (!k) continue;
      k.t += dt * k.speed;
      if (k.t < 1.15) continue;
      this.ripple(k.to, k.u, k.v, 0.12);
      this.onPulse?.(k.to, k.to > k.from);
      this.packets[i] = this.packets[this.packets.length - 1] as Packet;
      this.packets.pop();
    }
  }

  /** 背景浮尘:缓慢上升,被指针推开。 */
  private drawMotes(dt: number, p: Pointer, pal: Palette, tone: number) {
    for (const m of this.motes) {
      m.y -= m.vy * dt;
      if (m.y < -4) {
        m.y = this.height + 4;
        m.x = Math.random() * this.width;
      }
      const [x, y] = this.push(m.x + Math.sin(this.time * 0.6 + m.phase) * 6, m.y, p, 1.6);
      this.gridBatch.dot(pal, "subtle", 0.35 * tone, x, y);
    }
  }

  /** 指针附近的点被径向推开(无状态,随指针包络平滑)。 */
  private push(x: number, y: number, p: Pointer, gain: number): [number, number] {
    if (p.presence < 0.01) return [x, y];
    const dx = x - p.x;
    const dy = y - p.y;
    const d = Math.hypot(dx, dy);
    if (d >= PUSH_R || d < 0.01) return [x, y];
    const f = (1 - d / PUSH_R) ** 2 * 16 * gain * p.presence;
    return [x + (dx / d) * f, y + (dy / d) * f];
  }

  /** 相邻两层四角之间的虚线角柱(爆炸图导引线)。 */
  private drawPillars(ctx: CanvasRenderingContext2D, lower: number, pal: Palette, tone: number) {
    ctx.save();
    ctx.setLineDash([2, 4]);
    ctx.lineWidth = 1;
    ctx.strokeStyle = rgba(pal.subtle, 0.55 * tone);
    ctx.beginPath();
    for (const [u, v] of UNIT_CORNERS) {
      const [x0, y0] = this.project(u, v, lower);
      const [x1, y1] = this.project(u, v, lower + 1);
      ctx.moveTo(x0, y0);
      ctx.lineTo(x1, y1);
    }
    ctx.stroke();
    ctx.restore();
  }

  private drawLayer(ctx: CanvasRenderingContext2D, depth: number, p: Pointer, pal: Palette, tone: number) {
    const lit = this.lit[depth] ?? 0;
    const dots = this.layers[depth] ?? [];
    const t = this.time;
    const ripples = this.ripples.filter((r) => r.depth === depth);

    // 平面:页面底色半实心遮挡下层 + 点亮时的强调色光晕与外框
    const quad = UNIT_CORNERS.map(([u, v]) => this.project(u, v, depth));
    ctx.beginPath();
    quad.forEach(([x, y], i) => (i ? ctx.lineTo(x, y) : ctx.moveTo(x, y)));
    ctx.closePath();
    ctx.fillStyle = rgba(pal.bg, pal.dark ? 0.8 : 0.84);
    ctx.fill();
    ctx.fillStyle = rgba(pal.accent, (0.035 + 0.09 * lit) * tone);
    ctx.fill();
    if (lit > 0.02) {
      ctx.save();
      ctx.shadowColor = rgba(pal.accent, 0.55 * lit);
      ctx.shadowBlur = 18 * lit;
      ctx.strokeStyle = rgba(pal.accent, 0.95 * lit * tone);
      ctx.lineWidth = 1.6;
      ctx.stroke();
      ctx.restore();
    }
    ctx.strokeStyle = rgba(pal.subtle, (0.75 - 0.5 * lit) * tone);
    ctx.lineWidth = 1;
    ctx.stroke();

    const dim = 0.55 + 0.45 * lit;
    for (const dot of dots) {
      let [x, y] = this.project(dot.u, dot.v, depth);
      [x, y] = this.push(x, y, p, 1);
      let wave = 0;
      if (dot.kind === "deco" && dot.k >= 0) {
        if (depth === 3) wave = smoothstep(0.1, 0, Math.abs(((t * 0.22 + dot.k * 0.21) % 1.3) - 0.1 - (dot.u - 0.36) / 0.48));
        else if (depth === 2) wave = smoothstep(0.035, 0, Math.abs(((t * 0.16) % 0.44) + 0.04 - dot.k));
        else if (depth === 1) wave = smoothstep(0.14, 0, Math.abs(((t * 0.12 + dot.k * 0.37) % 1.3) - 0.1 - dot.u));
        else wave = 0.5 + 0.5 * Math.sin(t * 5 - dot.k * 60);
      }
      for (const r of ripples) {
        const radius = r.size * (1 - (1 - r.age / RIPPLE_LIFE) ** 3);
        const ring = smoothstep(0.05, 0, Math.abs(Math.hypot(dot.u - r.u, dot.v - r.v) - radius));
        wave = Math.max(wave, ring * (1 - r.age / RIPPLE_LIFE));
      }
      if (dot.kind === "grid") {
        this.gridBatch.dot(pal, lit > 0.5 ? "accent" : "subtle", (0.22 + 0.2 * lit + 0.6 * wave) * tone, x, y);
      } else if (wave > 0.35) {
        this.hotBatch.dot(pal, lit > 0.5 ? "cyan" : "accent", (0.4 + 0.3 * lit + 0.3 * wave) * tone, x, y);
      } else {
        this.dotBatch.dot(pal, lit > 0.5 ? "accent" : "fg", (0.3 + 0.35 * wave) * dim * tone, x, y);
      }
    }
    this.gridBatch.flush(ctx);
    this.dotBatch.flush(ctx);
    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    this.hotBatch.flush(ctx);
    ctx.globalCompositeOperation = "source-over";
  }

  /** 屏幕对齐的引出标注:从平面最左角水平引出,右对齐写层名。 */
  private drawLabel(ctx: CanvasRenderingContext2D, depth: number, pal: Palette, tone: number) {
    const label = this.labels[depth];
    if (!label) return;
    const lit = this.lit[depth] ?? 0;
    let [ax, ay] = this.project(0, 0, depth);
    for (const [u, v] of UNIT_CORNERS) {
      const [x, y] = this.project(u, v, depth);
      if (x < ax) [ax, ay] = [x, y];
    }
    const lead = 16 + 10 * lit;
    const color = lit > 0.5 ? pal.accent : pal.subtle;
    ctx.strokeStyle = rgba(color, (0.6 + 0.4 * lit) * tone);
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(ax - 3, ay);
    ctx.lineTo(ax - lead, ay);
    ctx.stroke();
    ctx.fillStyle = rgba(color, tone);
    ctx.beginPath();
    ctx.arc(ax, ay, 2 + lit, 0, Math.PI * 2);
    ctx.fill();
    ctx.font = `${lit > 0.5 ? 600 : 500} ${this.labelPx}px ${this.mono}`;
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    ctx.fillStyle = rgba(lit > 0.5 ? pal.accent : pal.fg, (0.62 + 0.38 * lit) * tone);
    ctx.fillText(label, ax - lead - 6, ay);
    ctx.textAlign = "start";
  }
}
