/**
 * 动态分段的可视化模拟(确定性,固定步长),复刻 `segment_coordinator` 的核心策略:
 * - 连接数逐步爬升(每 `rampEvery` 秒翻倍,直到 `maxConn`);
 * - 空闲连接把「剩余最多」的在传分段对半拆开并接管后半截(work stealing);
 * - 剩余量低于 `minSplit` 的分段不再拆分。
 * 仅用于官网演示,速度与抖动是示意值。
 */

export interface Segment {
  id: number;
  start: number;
  end: number;
  pos: number;
  /** 当前字节/秒 */
  speed: number;
  /** 该连接的基准速度(模拟不同镜像/链路质量) */
  base: number;
  worker: number;
}

export interface SplitEvent {
  at: number;
  offset: number;
  time: number;
}

export interface SimOptions {
  size: number;
  maxConn: number;
  /** 每条连接的平均速度(字节/秒) */
  perConn: number;
  rampEvery?: number;
  minSplit?: number;
  seed?: number;
  /** 起始进度(0..1),用于列表里「已经在下」的任务 */
  startAt?: number;
}

function mulberry32(seed: number) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

export class SegmentSim {
  readonly size: number;
  readonly maxConn: number;
  segments: Segment[] = [];
  splits: SplitEvent[] = [];
  time = 0;
  private readonly perConn: number;
  private readonly rampEvery: number;
  private readonly minSplit: number;
  private readonly rand: () => number;
  private nextId = 0;
  private nextWorker = 0;
  private freeWorkers: number[] = [];

  constructor(options: SimOptions) {
    this.size = options.size;
    this.maxConn = options.maxConn;
    this.perConn = options.perConn;
    this.rampEvery = options.rampEvery ?? 0.9;
    this.minSplit = options.minSplit ?? options.size / 400;
    this.rand = mulberry32(options.seed ?? 7);
    const startAt = Math.min(1, Math.max(0, options.startAt ?? 0));
    if (startAt > 0) {
      // 预热:直接生成一个已分段、部分完成的状态
      const n = options.maxConn;
      const chunk = this.size / n;
      for (let i = 0; i < n; i++) {
        const start = i * chunk;
        const end = i === n - 1 ? this.size : (i + 1) * chunk;
        const jitter = 0.75 + this.rand() * 0.5;
        const seg = this.spawn(start, end);
        seg.pos = Math.min(end, start + (end - start) * Math.min(1, startAt * jitter));
      }
      this.time = this.rampEvery * Math.log2(n) + 1;
    } else {
      this.spawn(0, this.size);
    }
  }

  private spawn(start: number, end: number): Segment {
    const worker = this.freeWorkers.shift() ?? this.nextWorker++;
    // 约 1/6 的连接是「慢镜像」,展示被拆分救援的效果
    const slow = this.rand() < 0.17;
    const base = this.perConn * (slow ? 0.18 + this.rand() * 0.12 : 0.75 + this.rand() * 0.5);
    const seg: Segment = { id: this.nextId++, start, end, pos: start, speed: 0, base, worker };
    this.segments.push(seg);
    return seg;
  }

  get allowed(): number {
    return Math.min(this.maxConn, 2 ** Math.floor(this.time / this.rampEvery));
  }

  get active(): Segment[] {
    return this.segments.filter((s) => s.pos < s.end);
  }

  get downloaded(): number {
    let sum = 0;
    for (const s of this.segments) sum += s.pos - s.start;
    return sum;
  }

  get progress(): number {
    return this.downloaded / this.size;
  }

  get speed(): number {
    let sum = 0;
    for (const s of this.segments) if (s.pos < s.end) sum += s.speed;
    return sum;
  }

  get done(): boolean {
    return this.downloaded >= this.size - 1;
  }

  step(dt: number) {
    this.time += dt;
    for (const s of this.segments) {
      if (s.pos >= s.end) continue;
      // 速度平滑爬升 + 轻微抖动(TCP 慢启动的示意)
      const target = s.base * (0.85 + this.rand() * 0.3);
      s.speed += (target - s.speed) * Math.min(1, dt * 3);
      s.pos = Math.min(s.end, s.pos + s.speed * dt);
      if (s.pos >= s.end && !this.freeWorkers.includes(s.worker)) this.freeWorkers.push(s.worker);
    }
    // 空闲连接接管:拆最大剩余分段
    while (this.active.length < this.allowed) {
      let victim: Segment | undefined;
      for (const s of this.active) {
        if (!victim || s.end - s.pos > victim.end - victim.pos) victim = s;
      }
      if (!victim) break;
      const remaining = victim.end - victim.pos;
      if (remaining < this.minSplit * 2) break;
      const mid = victim.pos + remaining / 2;
      const end = victim.end;
      victim.end = mid;
      this.spawn(mid, end);
      this.splits.push({ at: this.time, offset: mid, time: this.time });
    }
    if (this.splits.length > 64) this.splits.splice(0, this.splits.length - 64);
  }
}

const UNITS = ["B", "KB", "MB", "GB", "TB"];

export function formatBytes(bytes: number, digits = 1): string {
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  return `${value.toFixed(unit === 0 ? 0 : digits)} ${UNITS[unit]}`;
}

export function formatEta(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds <= 0) return "—";
  const s = Math.ceil(seconds);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  if (h > 0) return `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`;
  return `${m}:${String(sec).padStart(2, "0")}`;
}
