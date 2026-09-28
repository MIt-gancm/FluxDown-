/**
 * 首屏粒子流:「多源 → 一个文件」的数据汇聚。
 *
 * - 粒子沿 hero-geometry 的贝塞尔通道流向复刻窗口顶部焦点,越近越快、越亮;
 *   抵达即被吸收,为焦点光晕与地平线充能。
 * - 指针是一块透镜:附近粒子被推开并绕行(弹簧回位),同时与指针、彼此之间亮起
 *   神经连线;准星随指针速度张开。
 * - 在空白处按下:冲击波推散粒子,并迸出一簇火花,随后被焦点引力收回。
 * - 悬停下载按钮(`setSurge`):整条流加速增亮,焦点充能。
 */
import { CHANNELS, FOCUS, fitSlice, type Channel } from "./hero-geometry";
import {
  StrokeBatch,
  rgba,
  smoothstep,
  type Palette,
  type Pointer,
  type Scene,
} from "../../scripts/particle-stage";

interface Mote {
  channel: Channel;
  /** 通道参数 0 → 1 */
  t: number;
  speed: number;
  /** 相对通道的横向偏移(px),随 t 收束为 0 */
  lane: number;
  phase: number;
  thick: boolean;
  cyan: boolean;
  /** 指针扰动位移及其速度(弹簧阻尼) */
  dx: number;
  dy: number;
  vx: number;
  vy: number;
  x: number;
  y: number;
  px: number;
  py: number;
  alpha: number;
  fresh: boolean;
}

interface Spark {
  x: number;
  y: number;
  px: number;
  py: number;
  vx: number;
  vy: number;
  age: number;
  cyan: boolean;
}

interface Wave {
  x: number;
  y: number;
  age: number;
}

const TAU = Math.PI * 2;
const LENS_R = 150;
const LINK_R = 170;
const PAIR_R = 90;
const MESH_R = 38;
const WAVE_R = 240;
const WAVE_LIFE = 0.9;
const SPRING = 26;
const MAX_DISP = 110;
const MAX_SPARKS = 180;
const TRAIL = 22;

function reseed(m: Mote, t: number) {
  m.channel = CHANNELS[Math.floor(Math.random() * CHANNELS.length)] ?? m.channel;
  m.t = t;
  m.speed = 0.11 + Math.random() * 0.09;
  m.lane = (Math.random() * 2 - 1) * 7;
  m.phase = Math.random() * TAU;
  m.thick = Math.random() < 0.35;
  m.cyan = Math.random() < 0.3;
  m.dx = m.dy = m.vx = m.vy = 0;
  m.fresh = true;
}

export class FluxField implements Scene {
  private motes: Mote[] = [];
  private sparks: Spark[] = [];
  private waves: Wave[] = [];
  private near: { m: Mote; d: number }[] = [];
  private fit = fitSlice(1, 1);
  private fx = 0;
  private fy = 0;
  private time = 0;
  private energy = 0.25;
  private surge = 0;
  private surgeTarget = 0;

  private readonly thin = new StrokeBatch(1.1);
  private readonly thick = new StrokeBatch(1.9);
  private readonly hair = new StrokeBatch(0.75);
  private readonly spark = new StrokeBatch(1.6);

  setSurge(on: boolean) {
    this.surgeTarget = on ? 1 : 0;
  }

  resize(width: number, height: number) {
    this.fit = fitSlice(width, height);
    this.fx = this.fit.x + FOCUS.x * this.fit.scale;
    this.fy = this.fit.y + FOCUS.y * this.fit.scale;
    const target = Math.round(Math.min(240, Math.max(70, (width * height) / 4200)));
    while (this.motes.length < target) {
      const m = { channel: CHANNELS[0] } as Mote;
      reseed(m, Math.random());
      m.x = m.y = m.px = m.py = m.alpha = 0;
      this.motes.push(m);
    }
    this.motes.length = target;
    for (const m of this.motes) m.fresh = true;
  }

  press(x: number, y: number) {
    this.waves.push({ x, y, age: 0 });
    if (this.waves.length > 5) this.waves.shift();
    for (const m of this.motes) {
      const rx = m.x - x;
      const ry = m.y - y;
      const d = Math.hypot(rx, ry) || 1;
      if (d > WAVE_R) continue;
      const impulse = (1 - d / WAVE_R) * 1100;
      m.vx += (rx / d) * impulse;
      m.vy += (ry / d) * impulse;
    }
    const count = Math.min(26, MAX_SPARKS - this.sparks.length);
    for (let i = 0; i < count; i++) {
      const a = Math.random() * TAU;
      const v = 160 + Math.random() * 340;
      this.sparks.push({ x, y, px: x, py: y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, age: 0, cyan: Math.random() < 0.5 });
    }
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    this.time += dt;
    this.surge += (this.surgeTarget - this.surge) * Math.min(1, dt * 3.5);
    this.energy *= Math.exp(-1.6 * dt);

    const presence = p.presence;
    const stir = 1 + Math.min(2, Math.hypot(p.vx, p.vy) / 900);
    const flow = 1 + 1.7 * this.surge;
    const damp = Math.exp(-7 * dt);
    const { scale, x: ox, y: oy } = this.fit;
    const tone = pal.dark ? 1 : 0.8;

    for (const m of this.motes) {
      m.t += m.speed * (0.5 + 1.7 * m.t * m.t) * flow * dt;
      if (m.t >= 1) {
        this.energy = Math.min(1, this.energy + 0.018);
        reseed(m, Math.random() * 0.04);
      }
      // 三次贝塞尔求点与法线
      const c = m.channel;
      const t = m.t;
      const u = 1 - t;
      const b0 = u * u * u;
      const b1 = 3 * u * u * t;
      const b2 = 3 * u * t * t;
      const b3 = t * t * t;
      const tx = 3 * u * u * (c.p1.x - c.p0.x) + 6 * u * t * (c.p2.x - c.p1.x) + 3 * t * t * (c.p3.x - c.p2.x);
      const ty = 3 * u * u * (c.p1.y - c.p0.y) + 6 * u * t * (c.p2.y - c.p1.y) + 3 * t * t * (c.p3.y - c.p2.y);
      const tl = Math.hypot(tx, ty) || 1;
      const off = (m.lane + Math.sin(this.time * 1.7 + m.phase) * 1.6) * Math.pow(u, 1.3);
      const baseX = ox + (b0 * c.p0.x + b1 * c.p1.x + b2 * c.p2.x + b3 * c.p3.x) * scale - (ty / tl) * off;
      const baseY = oy + (b0 * c.p0.y + b1 * c.p1.y + b2 * c.p2.y + b3 * c.p3.y) * scale + (tx / tl) * off;

      if (dt > 0) {
        let ax = -SPRING * m.dx;
        let ay = -SPRING * m.dy;
        if (presence > 0.01) {
          const rx = baseX + m.dx - p.x;
          const ry = baseY + m.dy - p.y;
          const d2 = rx * rx + ry * ry;
          if (d2 < LENS_R * LENS_R) {
            const d = Math.sqrt(d2) || 1;
            const f = (1 - d / LENS_R) ** 2 * presence;
            const push = 2400 * f * stir;
            const swirl = 1300 * f;
            ax += (rx / d) * push - (ry / d) * swirl + p.vx * f * 3;
            ay += (ry / d) * push + (rx / d) * swirl + p.vy * f * 3;
          }
        }
        m.vx = (m.vx + ax * dt) * damp;
        m.vy = (m.vy + ay * dt) * damp;
        m.dx += m.vx * dt;
        m.dy += m.vy * dt;
        const disp = Math.hypot(m.dx, m.dy);
        if (disp > MAX_DISP) {
          m.dx *= MAX_DISP / disp;
          m.dy *= MAX_DISP / disp;
        }
      }

      const x = baseX + m.dx;
      const y = baseY + m.dy;
      m.px = m.fresh ? x : m.x;
      m.py = m.fresh ? y : m.y;
      m.x = x;
      m.y = y;
      m.fresh = false;
      m.alpha = Math.min(1, smoothstep(0, 0.06, t) * (1 - smoothstep(0.88, 1, t)) * (0.35 + 0.65 * t) * (1 + 0.35 * this.surge));
    }

    for (let i = this.sparks.length - 1; i >= 0; i--) {
      const s = this.sparks[i];
      if (!s) continue;
      s.age += dt;
      const gx = this.fx - s.x;
      const gy = this.fy - s.y;
      const gd = Math.hypot(gx, gy) || 1;
      const pull = 260 + s.age * s.age * 1400;
      const drag = Math.exp(-1.4 * dt);
      s.vx = (s.vx + (gx / gd) * pull * dt) * drag;
      s.vy = (s.vy + (gy / gd) * pull * dt) * drag;
      s.px = s.x;
      s.py = s.y;
      s.x += s.vx * dt;
      s.y += s.vy * dt;
      if (gd < 14 || s.age > 5) {
        this.energy = Math.min(1, this.energy + 0.05);
        this.sparks[i] = this.sparks[this.sparks.length - 1] as Spark;
        this.sparks.pop();
      }
    }
    for (const w of this.waves) w.age += dt;
    this.waves = this.waves.filter((w) => w.age < WAVE_LIFE);

    this.drawGlow(ctx, p, pal, tone);
    this.drawLinks(p, pal, tone);
    this.hair.flush(ctx);

    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    for (const m of this.motes) {
      if (m.alpha < 0.02) continue;
      const vx = m.x - m.px;
      const vy = m.y - m.py;
      const len = Math.hypot(vx, vy);
      const k = len > 0.01 ? Math.min(TRAIL, len * 3.2) / len : 0;
      const batch = m.thick ? this.thick : this.thin;
      batch.line(pal, m.cyan ? "cyan" : "accent", m.alpha * 0.9 * tone, m.x - vx * k, m.y - vy * k, m.x + 0.01, m.y);
    }
    for (const s of this.sparks) {
      const a = Math.min(1, s.age * 8) * (1 - smoothstep(3.5, 5, s.age));
      this.spark.line(pal, s.cyan ? "cyan" : "accent", a * tone, s.px - (s.x - s.px) * 2, s.py - (s.y - s.py) * 2, s.x, s.y);
    }
    this.thin.flush(ctx);
    this.thick.flush(ctx);
    this.spark.flush(ctx);
    ctx.globalCompositeOperation = "source-over";

    this.drawWaves(ctx, pal);
    this.drawReticle(ctx, p, pal);
  }

  /** 指针柔光 + 焦点光晕 + 窗口顶沿「地平线」,强度随吸收能量起伏。 */
  private drawGlow(ctx: CanvasRenderingContext2D, p: Pointer, pal: Palette, tone: number) {
    if (p.presence > 0.01) {
      const r = 240;
      const g = ctx.createRadialGradient(p.x, p.y, 0, p.x, p.y, r);
      g.addColorStop(0, rgba(pal.accent, 0.1 * p.presence * tone));
      g.addColorStop(1, rgba(pal.accent, 0));
      ctx.fillStyle = g;
      ctx.fillRect(p.x - r, p.y - r, r * 2, r * 2);
    }
    const e = Math.min(1, this.energy + this.surge * 0.35);
    const r = 44 + 80 * e;
    const halo = ctx.createRadialGradient(this.fx, this.fy, 0, this.fx, this.fy, r);
    halo.addColorStop(0, rgba(pal.accent, (0.3 + 0.5 * e) * tone));
    halo.addColorStop(0.35, rgba(pal.accent, (0.1 + 0.18 * e) * tone));
    halo.addColorStop(1, rgba(pal.accent, 0));
    ctx.fillStyle = halo;
    ctx.fillRect(this.fx - r, this.fy - r, r * 2, r * 2);
    const hw = 140 + 300 * e;
    const line = ctx.createLinearGradient(this.fx - hw, 0, this.fx + hw, 0);
    line.addColorStop(0, rgba(pal.cyan, 0));
    line.addColorStop(0.5, rgba(pal.cyan, (0.35 + 0.5 * e) * tone));
    line.addColorStop(1, rgba(pal.cyan, 0));
    ctx.fillStyle = line;
    ctx.fillRect(this.fx - hw, this.fy - 1.5, hw * 2, 1.5);
  }

  /** 神经连线:相邻粒子间的常驻细网 + 指针周围被「点亮」的突触。 */
  private drawLinks(p: Pointer, pal: Palette, tone: number) {
    const motes = this.motes;
    const meshA = (pal.dark ? 0.2 : 0.16) * tone;
    for (let i = 0; i < motes.length; i++) {
      const a = motes[i];
      if (!a || a.alpha < 0.15 || a.t > 0.82) continue;
      for (let j = i + 1; j < motes.length; j++) {
        const b = motes[j];
        if (!b || b.alpha < 0.15 || b.t > 0.82) continue;
        const dx = a.x - b.x;
        const dy = a.y - b.y;
        if (Math.abs(dx) > MESH_R || Math.abs(dy) > MESH_R) continue;
        const d = Math.hypot(dx, dy);
        if (d < MESH_R) this.hair.line(pal, "accent", (1 - d / MESH_R) * Math.min(a.alpha, b.alpha) * meshA, a.x, a.y, b.x, b.y);
      }
    }

    if (p.presence < 0.01) return;
    const near = this.near;
    near.length = 0;
    for (const m of motes) {
      if (m.alpha < 0.15) continue;
      const d = Math.hypot(m.x - p.x, m.y - p.y);
      if (d < LINK_R) near.push({ m, d });
    }
    near.sort((a, b) => a.d - b.d);
    near.length = Math.min(near.length, 24);
    for (let i = 0; i < near.length; i++) {
      const a = near[i];
      if (!a) continue;
      const w = (1 - a.d / LINK_R) ** 1.5 * p.presence;
      this.hair.line(pal, "cyan", w * 0.55 * a.m.alpha * tone, p.x, p.y, a.m.x, a.m.y);
      this.hair.dot(pal, "fg", w * 0.9 * tone, a.m.x, a.m.y);
      for (let j = i + 1; j < near.length; j++) {
        const b = near[j];
        if (!b) continue;
        const d = Math.hypot(a.m.x - b.m.x, a.m.y - b.m.y);
        if (d < PAIR_R) this.hair.line(pal, "accent", (1 - d / PAIR_R) * 0.4 * p.presence * tone, a.m.x, a.m.y, b.m.x, b.m.y);
      }
    }
  }

  private drawWaves(ctx: CanvasRenderingContext2D, pal: Palette) {
    for (const w of this.waves) {
      const k = w.age / WAVE_LIFE;
      const r = WAVE_R * (1 - (1 - k) ** 3);
      ctx.strokeStyle = rgba(pal.accent, (1 - k) ** 2 * 0.6);
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.arc(w.x, w.y, r, 0, TAU);
      ctx.stroke();
      ctx.strokeStyle = rgba(pal.cyan, (1 - k) ** 3 * 0.5);
      ctx.beginPath();
      ctx.arc(w.x, w.y, r * 0.62, 0, TAU);
      ctx.stroke();
    }
  }

  /** 蓝图准星:四向刻度缓慢旋转,指针越快张得越开。 */
  private drawReticle(ctx: CanvasRenderingContext2D, p: Pointer, pal: Palette) {
    if (p.presence < 0.02) return;
    const a = p.presence * (pal.dark ? 0.55 : 0.45);
    const r = 12 + Math.min(10, Math.hypot(p.vx, p.vy) / 180);
    ctx.lineWidth = 1;
    ctx.strokeStyle = rgba(pal.fg, a * 0.45);
    ctx.setLineDash([2, 3]);
    ctx.beginPath();
    ctx.arc(p.x, p.y, r, 0, TAU);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.strokeStyle = rgba(pal.fg, a);
    ctx.beginPath();
    for (let k = 0; k < 4; k++) {
      const ang = this.time * 0.8 + (k * Math.PI) / 2;
      const cos = Math.cos(ang);
      const sin = Math.sin(ang);
      ctx.moveTo(p.x + cos * (r + 4), p.y + sin * (r + 4));
      ctx.lineTo(p.x + cos * (r + 9), p.y + sin * (r + 9));
    }
    ctx.stroke();
    ctx.fillStyle = rgba(pal.accent, a * 1.4);
    ctx.beginPath();
    ctx.arc(p.x, p.y, 1.6, 0, TAU);
    ctx.fill();
  }
}
