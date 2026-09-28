/**
 * 收尾 CTA 的神经点云球:斐波那契球面上的节点以最近邻织成网,缓慢自转。
 *
 * - 突触脉冲沿边传导并逐跳接力,抵达节点时点亮它 —— 「思考中」的网络。
 * - 指针牵引球体朝向,并像注意力一样吸引、点亮附近节点;附近更容易激发脉冲。
 * - 在空白处按下:球体轻微膨胀回弹,并从最近节点放出一轮链式脉冲。
 * - 球心区域整体压暗,不与居中的标题和按钮抢对比度。
 */
import { StrokeBatch, clamp01, smoothstep, type Palette, type Pointer, type Scene } from "../../scripts/particle-stage";

interface Node {
  x: number;
  y: number;
  z: number;
  links: number[];
  /** 投影结果(每帧更新) */
  sx: number;
  sy: number;
  depth: number;
  fade: number;
  flash: number;
}

interface Pulse {
  from: number;
  to: number;
  t: number;
  hops: number;
}

const GOLDEN = Math.PI * (3 - Math.sqrt(5));
const NEIGHBORS = 3;
const ATTN_R = 170;
const PULSE_SPEED = 2.1;
const MAX_PULSES = 90;

export class NeuralSphere implements Scene {
  private nodes: Node[] = [];
  private edges: [number, number][] = [];
  private pulses: Pulse[] = [];
  private width = 0;
  private height = 0;
  private radius = 0;
  private yaw = 0;
  private tiltX = 0;
  private tiltY = 0;
  private time = 0;
  private kick = 0;
  private kickV = 0;
  private spawnClock = 0;

  private readonly edgeBatch = new StrokeBatch(0.7);
  private readonly small = new StrokeBatch(1.6);
  private readonly large = new StrokeBatch(2.8);
  private readonly pulseBatch = new StrokeBatch(2);

  resize(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.radius = Math.min(320, Math.min(width, height) * 0.4);
    const count = Math.min(width, height) < 520 ? 140 : 240;
    if (count !== this.nodes.length) this.build(count);
  }

  private build(count: number) {
    this.nodes = Array.from({ length: count }, (_, i) => {
      const y = 1 - (2 * (i + 0.5)) / count;
      const r = Math.sqrt(1 - y * y);
      const a = i * GOLDEN;
      return { x: Math.cos(a) * r, y, z: Math.sin(a) * r, links: [], sx: 0, sy: 0, depth: 0, fade: 0, flash: 0 };
    });
    const seen = new Set<number>();
    this.edges = [];
    this.pulses = [];
    this.nodes.forEach((n, i) => {
      const nearest = this.nodes
        .map((o, j) => ({ j, d: (o.x - n.x) ** 2 + (o.y - n.y) ** 2 + (o.z - n.z) ** 2 }))
        .filter((o) => o.j !== i)
        .sort((a, b) => a.d - b.d)
        .slice(0, NEIGHBORS);
      for (const { j } of nearest) {
        const key = Math.min(i, j) * count + Math.max(i, j);
        if (seen.has(key)) continue;
        seen.add(key);
        this.edges.push([i, j]);
        n.links.push(j);
        this.nodes[j]?.links.push(i);
      }
    });
  }

  press(x: number, y: number) {
    this.kickV += 1.6;
    const origin = this.nearest(x, y, this.nodes.length);
    if (origin < 0) return;
    const node = this.nodes[origin];
    if (!node) return;
    node.flash = 1;
    for (const to of node.links) this.fire(origin, to, 6);
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    const { width, height } = this;
    const cx = width / 2;
    const cy = height / 2;
    this.time += dt;

    // 朝向:匀速自转 + 指针牵引(平滑跟随)
    const targetX = p.presence * ((p.y - cy) / height) * 0.7;
    const targetY = p.presence * ((p.x - cx) / width) * 1.1;
    const follow = Math.min(1, dt * 2.5);
    this.tiltX += (targetX - this.tiltX) * follow;
    this.tiltY += (targetY - this.tiltY) * follow;
    this.yaw += dt * 0.14;
    // 按下后的膨胀:欠阻尼弹簧
    this.kickV += (-30 * this.kick - 5 * this.kickV) * dt;
    this.kick += this.kickV * dt;

    const yaw = this.yaw + this.tiltY;
    const pitch = 0.38 + this.tiltX;
    const cosY = Math.cos(yaw);
    const sinY = Math.sin(yaw);
    const cosP = Math.cos(pitch);
    const sinP = Math.sin(pitch);
    const R = this.radius * (1 + 0.012 * Math.sin(this.time * 1.1) + 0.14 * this.kick);
    const decay = Math.exp(-2.6 * dt);

    for (const n of this.nodes) {
      const x1 = n.x * cosY + n.z * sinY;
      const z1 = -n.x * sinY + n.z * cosY;
      const y2 = n.y * cosP - z1 * sinP;
      const z2 = n.y * sinP + z1 * cosP;
      const persp = 2.6 / (2.6 - z2);
      let sx = cx + x1 * R * persp;
      let sy = cy + y2 * R * persp;
      const attn = p.presence * Math.max(0, 1 - Math.hypot(sx - p.x, sy - p.y) / ATTN_R) ** 2;
      sx += (p.x - sx) * attn * 0.08;
      sy += (p.y - sy) * attn * 0.08;
      n.sx = sx;
      n.sy = sy;
      n.depth = (z2 + 1) / 2;
      // 球心压暗:标题与按钮位于正中
      n.fade = 0.22 + 0.78 * smoothstep(0.3, 0.95, Math.hypot(sx - cx, sy - cy) / R);
      n.flash = Math.max(n.flash * decay, attn);
    }

    this.stepPulses(dt, p);

    const tone = pal.dark ? 1 : 0.85;
    for (const [a, b] of this.edges) {
      const na = this.nodes[a];
      const nb = this.nodes[b];
      if (!na || !nb) continue;
      const depth = (na.depth + nb.depth) / 2;
      const lit = Math.max(na.flash, nb.flash);
      const alpha = (0.05 + 0.26 * depth ** 1.5) * Math.min(na.fade, nb.fade) + lit * 0.35;
      this.edgeBatch.line(pal, lit > 0.25 ? "accent" : "subtle", alpha * tone, na.sx, na.sy, nb.sx, nb.sy);
    }
    this.edgeBatch.flush(ctx);

    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    for (const n of this.nodes) {
      const base = (0.12 + 0.7 * n.depth ** 1.4) * n.fade;
      const batch = n.depth > 0.62 || n.flash > 0.4 ? this.large : this.small;
      if (n.flash > 0.08) batch.dot(pal, n.flash > 0.6 ? "cyan" : "accent", clamp01(base + n.flash) * tone, n.sx, n.sy);
      else batch.dot(pal, "fg", base * 0.8 * tone, n.sx, n.sy);
    }
    for (const pulse of this.pulses) {
      const a = this.nodes[pulse.from];
      const b = this.nodes[pulse.to];
      if (!a || !b) continue;
      const head = pulse.t;
      const tail = Math.max(0, pulse.t - 0.35);
      const alpha = (0.35 + 0.65 * (a.depth + b.depth) / 2) * Math.min(a.fade, b.fade) * 1.4;
      this.pulseBatch.line(
        pal,
        "cyan",
        alpha * tone,
        a.sx + (b.sx - a.sx) * tail,
        a.sy + (b.sy - a.sy) * tail,
        a.sx + (b.sx - a.sx) * head,
        a.sy + (b.sy - a.sy) * head,
      );
    }
    this.small.flush(ctx);
    this.large.flush(ctx);
    this.pulseBatch.flush(ctx);
    ctx.globalCompositeOperation = "source-over";
  }

  private stepPulses(dt: number, p: Pointer) {
    if (dt <= 0) return;
    // 自发放电:常驻节律 + 指针在场时更活跃
    this.spawnClock += dt * (5 + 10 * p.presence);
    while (this.spawnClock >= 1) {
      this.spawnClock -= 1;
      const from = p.presence > 0.3 && Math.random() < 0.6 ? this.nearest(p.x, p.y, 14) : Math.floor(Math.random() * this.nodes.length);
      const node = this.nodes[from];
      const to = node?.links[Math.floor(Math.random() * node.links.length)];
      if (to !== undefined) this.fire(from, to, 1 + Math.floor(Math.random() * 3));
    }
    for (let i = this.pulses.length - 1; i >= 0; i--) {
      const pulse = this.pulses[i];
      if (!pulse) continue;
      pulse.t += dt * PULSE_SPEED;
      if (pulse.t < 1) continue;
      const arrived = this.nodes[pulse.to];
      if (arrived) arrived.flash = Math.max(arrived.flash, 0.9);
      this.pulses[i] = this.pulses[this.pulses.length - 1] as Pulse;
      this.pulses.pop();
      if (arrived && pulse.hops > 0) {
        const options = arrived.links.filter((j) => j !== pulse.from);
        const next = options[Math.floor(Math.random() * options.length)];
        if (next !== undefined) this.fire(pulse.to, next, pulse.hops - 1);
      }
    }
  }

  private fire(from: number, to: number, hops: number) {
    if (this.pulses.length < MAX_PULSES) this.pulses.push({ from, to, t: 0, hops });
  }

  /** 从 `samples` 个随机节点(全量时为全部)中取离 (x, y) 最近且朝向观者的一个。 */
  private nearest(x: number, y: number, samples: number) {
    let best = -1;
    let bestD = Infinity;
    const all = samples >= this.nodes.length;
    for (let k = 0; k < samples; k++) {
      const i = all ? k : Math.floor(Math.random() * this.nodes.length);
      const n = this.nodes[i];
      if (!n || n.depth < 0.45) continue;
      const d = (n.sx - x) ** 2 + (n.sy - y) ** 2;
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    }
    return best;
  }
}
