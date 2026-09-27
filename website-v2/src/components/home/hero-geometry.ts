/**
 * 首屏汇聚线的唯一几何源:SVG 线稿(Hero.astro)与粒子流(flux-field.ts)共用同一组
 * 三次贝塞尔,改这里两者同步,不会出现粒子偏离线稿。
 *
 * 坐标系是 viewBox(VIEW.w × VIEW.h),SVG 以 `xMidYMax slice` 铺满容器;
 * 粒子端用 `fitSlice` 复现同一映射。
 */
export interface Point {
  x: number;
  y: number;
}

export interface Channel {
  p0: Point;
  p1: Point;
  p2: Point;
  p3: Point;
}

export const VIEW = { w: 1280, h: 640 } as const;
export const FOCUS: Point = { x: VIEW.w / 2, y: VIEW.h };

const ROWS = 13;

/** 两侧各 13 条,从左右边缘水平出发,收束到复刻窗口顶部中点。 */
export const CHANNELS: readonly Channel[] = Array.from({ length: ROWS }, (_, i) => {
  const y = 36 + i * 44;
  const bend = 0.55 + (i % 4) * 0.06;
  return ([-1, 1] as const).map((side) => ({
    p0: { x: side < 0 ? 0 : VIEW.w, y },
    p1: { x: side < 0 ? VIEW.w * 0.3 : VIEW.w * 0.7, y },
    p2: { x: FOCUS.x + side * 180 * bend, y: FOCUS.y - 120 },
    p3: FOCUS,
  }));
}).flat();

export function channelPath({ p0, p1, p2, p3 }: Channel): string {
  return `M${p0.x} ${p0.y} C ${p1.x} ${p1.y}, ${p2.x} ${p2.y}, ${p3.x} ${p3.y}`;
}

/** `preserveAspectRatio="xMidYMax slice"` 的 viewBox → 容器像素映射。 */
export function fitSlice(width: number, height: number) {
  const scale = Math.max(width / VIEW.w, height / VIEW.h);
  return { scale, x: (width - VIEW.w * scale) / 2, y: height - VIEW.h * scale };
}
