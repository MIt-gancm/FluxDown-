/**
 * 格栅电路:把 1px 缝隙网格(`.cells` / `gap-px` 网格)的分隔线当成电路走线,
 * 粒子沿走线奔跑并在交叉点随机转向,留下渐隐尾迹。
 *
 * - 指针在场:路口转向偏向指针方向,指针所在格子的周边走线优先,附近粒子提亮并与指针连出突触;
 *   格内一小群粒子绕指针环行(裁剪在格子内,不越界压到相邻内容)。
 * - 在空白处按下:从最近路口沿每条走线各放出一颗高速粒子,并迸出一圈火花。
 * - 布局来自宿主子元素的实际矩形(响应式列数变化时重建走线图)。
 *
 * 用法:给网格容器加 `data-circuit`,页面脚本调用 `mountCircuits()`。
 */
import { StrokeBatch, mountStage, smoothstep, type Palette, type Pointer, type Scene } from "../../scripts/particle-stage";

interface Node {
  x: number;
  y: number;
  edges: number[];
  flash: number;
}

interface Edge {
  a: number;
  b: number;
  len: number;
}

interface Runner {
  edge: number;
  /** 沿边从 a → b 的参数 0 → 1 */
  t: number;
  dir: 1 | -1;
  speed: number;
  cyan: boolean;
  /** 尾迹环形缓冲(x, y 交替) */
  trail: Float32Array;
  head: number;
  filled: number;
  /** 爆发粒子寿命(秒);常驻粒子为 Infinity */
  life: number;
  x: number;
  y: number;
}

interface Spark {
  x: number;
  y: number;
  vx: number;
  vy: number;
  age: number;
}

interface Orbiter {
  angle: number;
  radius: number;
  spin: number;
  x: number;
  y: number;
}

interface Cell {
  x: number;
  y: number;
  w: number;
  h: number;
}

const TAU = Math.PI * 2;
const TRAIL = 22;
const LINK_R = 120;
const MAX_RUNNERS = 64;
const MAX_SPARKS = 60;
const ORBITERS = 16;
const SNAP = 3;

/** 把近似相等的坐标聚成一组,返回升序的代表值。 */
function cluster(values: number[]): number[] {
  const sorted = [...values].sort((a, b) => a - b);
  const out: number[] = [];
  for (const v of sorted) {
    const last = out[out.length - 1];
    if (last !== undefined && v - last < SNAP) continue;
    out.push(v);
  }
  return out;
}

function snap(values: number[], v: number): number {
  let best = v;
  let bestD = Infinity;
  for (const c of values) {
    const d = Math.abs(c - v);
    if (d < bestD) {
      bestD = d;
      best = c;
    }
  }
  return best;
}

export class GridCircuit implements Scene {
  private nodes: Node[] = [];
  private edges: Edge[] = [];
  private cells: Cell[] = [];
  private runners: Runner[] = [];
  private sparks: Spark[] = [];
  private orbiters: Orbiter[] = [];
  private hot = -1;
  private hotFade = 0;
  private width = 0;
  private height = 0;

  private readonly trailBatch = new StrokeBatch(1.4);
  private readonly headBatch = new StrokeBatch(2.6);
  private readonly hair = new StrokeBatch(0.7);
  private readonly dotBatch = new StrokeBatch(2.2);

  constructor(
    private readonly host: HTMLElement,
    private readonly canvas: HTMLCanvasElement,
  ) {
    for (let i = 0; i < ORBITERS; i++) {
      this.orbiters.push({ angle: (i / ORBITERS) * TAU, radius: 16 + Math.random() * 46, spin: (0.6 + Math.random() * 1.2) * (Math.random() < 0.5 ? -1 : 1), x: 0, y: 0 });
    }
  }

  resize(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.layout();
  }

  /** 按子元素实际矩形重建走线图(canvas 左上角为原点)。 */
  layout() {
    const origin = this.canvas.getBoundingClientRect();
    const { width, height } = this;
    if (!width || !height) return;
    const clampX = (v: number) => Math.min(width - 0.5, Math.max(0.5, v));
    const clampY = (v: number) => Math.min(height - 0.5, Math.max(0.5, v));
    const rects: Cell[] = [];
    for (const child of this.host.children) {
      if (child === this.canvas) continue;
      const r = child.getBoundingClientRect();
      if (r.width < 1 || r.height < 1) continue;
      rects.push({ x: r.left - origin.left, y: r.top - origin.top, w: r.width, h: r.height });
    }
    this.cells = rects;
    // 分隔线位于格子外沿半像素处(1px 缝隙的中线)
    const xs = cluster(rects.flatMap((r) => [clampX(r.x - 0.5), clampX(r.x + r.w + 0.5)]));
    const ys = cluster(rects.flatMap((r) => [clampY(r.y - 0.5), clampY(r.y + r.h + 0.5)]));

    const nodeIndex = new Map<string, number>();
    const nodes: Node[] = [];
    const edges: Edge[] = [];
    const edgeKeys = new Set<string>();
    const node = (x: number, y: number) => {
      const key = `${x}|${y}`;
      let i = nodeIndex.get(key);
      if (i === undefined) {
        i = nodes.length;
        nodes.push({ x, y, edges: [], flash: 0 });
        nodeIndex.set(key, i);
      }
      return i;
    };
    const addRun = (fixed: number, from: number, to: number, horizontal: boolean) => {
      const stops = (horizontal ? xs : ys).filter((v) => v >= from - SNAP && v <= to + SNAP);
      for (let k = 0; k + 1 < stops.length; k++) {
        const s0 = stops[k] as number;
        const s1 = stops[k + 1] as number;
        const a = horizontal ? node(s0, fixed) : node(fixed, s0);
        const b = horizontal ? node(s1, fixed) : node(fixed, s1);
        const key = a < b ? `${a}-${b}` : `${b}-${a}`;
        if (edgeKeys.has(key)) continue;
        edgeKeys.add(key);
        const e = edges.length;
        edges.push({ a, b, len: s1 - s0 });
        nodes[a]?.edges.push(e);
        nodes[b]?.edges.push(e);
      }
    };
    for (const r of rects) {
      const left = snap(xs, clampX(r.x - 0.5));
      const right = snap(xs, clampX(r.x + r.w + 0.5));
      const top = snap(ys, clampY(r.y - 0.5));
      const bottom = snap(ys, clampY(r.y + r.h + 0.5));
      addRun(top, left, right, true);
      addRun(bottom, left, right, true);
      addRun(left, top, bottom, false);
      addRun(right, top, bottom, false);
    }
    this.nodes = nodes;
    this.edges = edges;

    // 走线图已变:丢弃旧粒子(尾迹坐标失效),按总走线长度重新投放常驻粒子
    const total = edges.reduce((sum, e) => sum + e.len, 0);
    const target = edges.length ? Math.round(Math.min(26, Math.max(4, total / 240))) : 0;
    this.runners = Array.from({ length: target }, () => this.spawn(Math.floor(Math.random() * edges.length), Math.random(), Infinity));
    this.sparks = [];
  }

  press(x: number, y: number) {
    const from = this.nearestNode(x, y);
    const n = this.nodes[from];
    if (!n) return;
    n.flash = 1;
    for (const e of n.edges) {
      if (this.runners.length >= MAX_RUNNERS) break;
      const edge = this.edges[e];
      if (!edge) continue;
      const r = this.spawn(e, edge.a === from ? 0 : 1, 1.6);
      r.dir = edge.a === from ? 1 : -1;
      r.speed = 320 + Math.random() * 120;
      r.cyan = true;
      this.runners.push(r);
    }
    const count = Math.min(18, MAX_SPARKS - this.sparks.length);
    for (let i = 0; i < count; i++) {
      const a = Math.random() * TAU;
      const v = 90 + Math.random() * 220;
      this.sparks.push({ x, y, vx: Math.cos(a) * v, vy: Math.sin(a) * v, age: 0 });
    }
  }

  frame(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette) {
    if (!this.edges.length) return;
    const tone = pal.dark ? 1 : 0.85;
    const hot = p.presence > 0.05 ? this.cells.findIndex((c) => p.x >= c.x && p.x <= c.x + c.w && p.y >= c.y && p.y <= c.y + c.h) : -1;
    if (hot !== -1) this.hot = hot;
    this.hotFade += ((hot !== -1 ? 1 : 0) - this.hotFade) * Math.min(1, dt * 5);
    const decay = Math.exp(-3 * dt);
    for (const n of this.nodes) n.flash *= decay;

    for (let i = this.runners.length - 1; i >= 0; i--) {
      const r = this.runners[i];
      if (!r) continue;
      if (dt > 0) {
        r.life -= dt;
        if (r.life <= 0) {
          this.runners[i] = this.runners[this.runners.length - 1] as Runner;
          this.runners.pop();
          continue;
        }
        this.advance(r, dt, p);
      }
      this.place(r);
    }

    for (let i = this.sparks.length - 1; i >= 0; i--) {
      const s = this.sparks[i];
      if (!s) continue;
      s.age += dt;
      const drag = Math.exp(-3.2 * dt);
      s.vx *= drag;
      s.vy *= drag;
      s.x += s.vx * dt;
      s.y += s.vy * dt;
      if (s.age > 0.9) {
        this.sparks[i] = this.sparks[this.sparks.length - 1] as Spark;
        this.sparks.pop();
      }
    }

    this.drawOrbiters(ctx, dt, p, pal, tone);

    // 路口:常驻暗点 + 经过时闪亮
    for (const n of this.nodes) {
      if (n.edges.length < 3 && n.flash < 0.05) continue;
      this.dotBatch.dot(pal, n.flash > 0.3 ? "cyan" : "subtle", (0.22 + 0.78 * n.flash) * tone, n.x, n.y);
    }

    if (pal.dark) ctx.globalCompositeOperation = "lighter";
    for (const r of this.runners) {
      const near = p.presence > 0.01 ? Math.max(0, 1 - Math.hypot(r.x - p.x, r.y - p.y) / LINK_R) * p.presence : 0;
      // 爆发粒子在寿命末段淡出
      const fade = Number.isFinite(r.life) ? smoothstep(0, 0.5, r.life) : 1;
      const base = (0.5 + 0.5 * near) * fade * tone;
      const color = r.cyan || near > 0.4 ? "cyan" : "accent";
      for (let k = 0; k + 1 < r.filled; k++) {
        const i0 = (r.head - k + TRAIL) % TRAIL;
        const i1 = (r.head - k - 1 + TRAIL) % TRAIL;
        const a = base * (1 - (k + 1) / r.filled) ** 1.4;
        this.trailBatch.line(pal, color, a, r.trail[i0 * 2] ?? 0, r.trail[i0 * 2 + 1] ?? 0, r.trail[i1 * 2] ?? 0, r.trail[i1 * 2 + 1] ?? 0);
      }
      this.headBatch.dot(pal, color, Math.min(1, base * 1.4), r.x, r.y);
      if (near > 0.05) this.hair.line(pal, "cyan", near * 0.3 * tone, p.x, p.y, r.x, r.y);
    }
    for (const s of this.sparks) {
      const a = (1 - s.age / 0.9) ** 2 * tone;
      this.hair.line(pal, "accent", a, s.x - s.vx * 0.03, s.y - s.vy * 0.03, s.x, s.y);
    }
    this.hair.flush(ctx);
    this.trailBatch.flush(ctx);
    this.headBatch.flush(ctx);
    this.dotBatch.flush(ctx);
    ctx.globalCompositeOperation = "source-over";
  }

  private spawn(edge: number, t: number, life: number): Runner {
    return {
      edge,
      t,
      dir: Math.random() < 0.5 ? 1 : -1,
      speed: 60 + Math.random() * 90,
      cyan: Math.random() < 0.3,
      trail: new Float32Array(TRAIL * 2),
      head: 0,
      filled: 0,
      life,
      x: 0,
      y: 0,
    };
  }

  private advance(r: Runner, dt: number, p: Pointer) {
    const hotCell = this.hotFade > 0.05 ? this.cells[this.hot] : undefined;
    const onHot = hotCell ? this.onPerimeter(r.edge, hotCell) : false;
    let travel = r.speed * dt * (onHot ? 1 + this.hotFade : 1);
    // 一帧内可能跨过多个路口
    for (let guard = 0; guard < 6 && travel > 0; guard++) {
      const edge = this.edges[r.edge];
      if (!edge || edge.len <= 0) return;
      const remain = (r.dir === 1 ? 1 - r.t : r.t) * edge.len;
      if (travel < remain) {
        r.t += (r.dir * travel) / edge.len;
        return;
      }
      travel -= remain;
      const at = r.dir === 1 ? edge.b : edge.a;
      const node = this.nodes[at];
      if (!node) return;
      node.flash = Math.max(node.flash, 0.7);
      const next = this.choose(at, r.edge, p, hotCell);
      const nextEdge = this.edges[next];
      if (!nextEdge) return;
      r.edge = next;
      r.dir = nextEdge.a === at ? 1 : -1;
      r.t = r.dir === 1 ? 0 : 1;
    }
  }

  /** 路口转向:不走回头路;指针方向与悬停格子周边加权。 */
  private choose(at: number, from: number, p: Pointer, hotCell: Cell | undefined) {
    const node = this.nodes[at];
    if (!node) return from;
    const options = node.edges.length > 1 ? node.edges.filter((e) => e !== from) : node.edges;
    let total = 0;
    const weights = options.map((e) => {
      const edge = this.edges[e];
      const other = edge ? this.nodes[edge.a === at ? edge.b : edge.a] : undefined;
      let w = 1;
      if (other && p.presence > 0.05) {
        const dx = other.x - node.x;
        const dy = other.y - node.y;
        const px = p.x - node.x;
        const py = p.y - node.y;
        const len = Math.hypot(dx, dy) * Math.hypot(px, py) || 1;
        w += 3 * p.presence * Math.max(0, (dx * px + dy * py) / len);
      }
      if (hotCell && this.onPerimeter(e, hotCell)) w += 4 * this.hotFade;
      total += w;
      return w;
    });
    let pick = Math.random() * total;
    for (let i = 0; i < options.length; i++) {
      pick -= weights[i] ?? 0;
      if (pick <= 0) return options[i] ?? from;
    }
    return options[options.length - 1] ?? from;
  }

  private place(r: Runner) {
    const edge = this.edges[r.edge];
    const a = edge ? this.nodes[edge.a] : undefined;
    const b = edge ? this.nodes[edge.b] : undefined;
    if (!a || !b) return;
    r.x = a.x + (b.x - a.x) * r.t;
    r.y = a.y + (b.y - a.y) * r.t;
    r.head = (r.head + 1) % TRAIL;
    r.trail[r.head * 2] = r.x;
    r.trail[r.head * 2 + 1] = r.y;
    r.filled = Math.min(TRAIL, r.filled + 1);
  }

  private onPerimeter(e: number, c: Cell) {
    const edge = this.edges[e];
    const a = edge ? this.nodes[edge.a] : undefined;
    const b = edge ? this.nodes[edge.b] : undefined;
    if (!a || !b) return false;
    const mx = (a.x + b.x) / 2;
    const my = (a.y + b.y) / 2;
    const tol = SNAP + 1;
    const inX = mx >= c.x - tol && mx <= c.x + c.w + tol;
    const inY = my >= c.y - tol && my <= c.y + c.h + tol;
    const onV = Math.abs(mx - c.x) <= tol || Math.abs(mx - c.x - c.w) <= tol;
    const onH = Math.abs(my - c.y) <= tol || Math.abs(my - c.y - c.h) <= tol;
    return (onV && inY) || (onH && inX);
  }

  private nearestNode(x: number, y: number) {
    let best = -1;
    let bestD = Infinity;
    this.nodes.forEach((n, i) => {
      const d = (n.x - x) ** 2 + (n.y - y) ** 2;
      if (d < bestD) {
        bestD = d;
        best = i;
      }
    });
    return best;
  }

  /** 悬停格子内绕指针环行的一小群粒子,裁剪在格子内。 */
  private drawOrbiters(ctx: CanvasRenderingContext2D, dt: number, p: Pointer, pal: Palette, tone: number) {
    const cell = this.cells[this.hot];
    const k = this.hotFade * p.presence;
    const follow = Math.min(1, dt * 7);
    for (const o of this.orbiters) {
      o.angle += o.spin * dt;
      const tx = p.x + Math.cos(o.angle) * o.radius;
      const ty = p.y + Math.sin(o.angle) * o.radius * 0.8;
      if (k < 0.02 || dt === 0) {
        o.x = tx;
        o.y = ty;
      } else {
        o.x += (tx - o.x) * follow;
        o.y += (ty - o.y) * follow;
      }
    }
    if (!cell || k < 0.02) return;
    ctx.save();
    ctx.beginPath();
    ctx.rect(cell.x, cell.y, cell.w, cell.h);
    ctx.clip();
    for (const o of this.orbiters) {
      const a = k * (0.25 + 0.35 * (1 - (o.radius - 16) / 46)) * tone;
      this.dotBatch.dot(pal, o.spin > 0 ? "accent" : "cyan", a, o.x, o.y);
    }
    this.dotBatch.flush(ctx);
    ctx.restore();
  }
}

/** 为页面上所有 `[data-circuit]` 网格挂载电路粒子层。 */
export function mountCircuits(root: ParentNode = document) {
  root.querySelectorAll<HTMLElement>("[data-circuit]").forEach((host) => {
    if (host.querySelector(":scope > .circuit-layer")) return;
    const canvas = document.createElement("canvas");
    canvas.className = "circuit-layer";
    canvas.setAttribute("aria-hidden", "true");
    host.append(canvas);
    const scene = new GridCircuit(host, canvas);
    mountStage(canvas, host, scene);
    // 子元素尺寸变化(换行、字体加载)但宿主尺寸不变时,也要重建走线
    let queued = 0;
    const relayout = () => {
      if (queued) return;
      queued = requestAnimationFrame(() => {
        queued = 0;
        scene.layout();
      });
    };
    const observer = new ResizeObserver(relayout);
    for (const child of host.children) if (child !== canvas) observer.observe(child);
  });
}
