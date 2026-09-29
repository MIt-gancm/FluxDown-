// 由真实文件字节区间投影的进度条（移植 crates/downloads/src/components/segment_progress.rs）。
// 窄列中把多个区间聚合到像素，绝不把总下载字节均摊到分段或把分段数当作活跃连接数。

import { memo, useLayoutEffect, useRef, useState } from 'react'
import { cn } from '../../../lib/cn'
import type { TaskRuntimeDto } from '../../../lib/rpc'
import type { TaskState } from '../model/task'

/** 相邻分段只有在投影宽度至少这么多像素时才留 1px 细缝。 */
const MIN_SEGMENT_WIDTH_FOR_GAP = 5
/** 像素覆盖率达到 `1 - FULL_COVERAGE_EPSILON` 即视为已完成。 */
const FULL_COVERAGE_EPSILON = 1e-5
/** 进度条最大像素宽度：防御异常列宽导致逐像素节点数失控。 */
const MAX_BAR_WIDTH = 4096

interface Pixel {
  completed: number
  active: boolean
  gap: boolean
}

/** 所有坐标限制在 [0, width)；把不可见的小分段按像素合并。 */
function projectPixels(runtime: TaskRuntimeDto, width: number): Pixel[] {
  const result: Pixel[] = Array.from({ length: width }, () => ({ completed: 0, active: false, gap: false }))
  const total = Math.max(0, runtime.totalBytes)
  if (total === 0 || width === 0) return result
  const scale = width / total
  for (const segment of runtime.segments) {
    // Wire 区间为 [startByte, endByte]（含末字节），下载量属于本段。
    const start = Math.max(0, segment.startByte)
    const end = Math.min(Math.max(0, segment.endByte) + 1, total)
    if (end <= start || start >= total) continue
    const done = Math.min(start + Math.max(0, segment.downloadedBytes), end)
    const left = start * scale
    const right = end * scale
    const filled = done * scale
    const first = Math.min(Math.floor(left), width)
    const last = Math.min(Math.ceil(right), width)
    for (let index = first; index < last; index += 1) {
      const pixel = result[index] as Pixel
      pixel.completed += Math.min(1, Math.max(0, Math.min(filled, index + 1) - Math.max(left, index)))
      if (segment.active === true && index < right && index + 1 > filled) pixel.active = true
    }
    if (right - left >= MIN_SEGMENT_WIDTH_FOR_GAP && end < total) {
      const boundary = Math.floor(right)
      if (boundary < width) (result[boundary] as Pixel).gap = true
    }
  }
  for (const pixel of result) pixel.completed = Math.min(pixel.completed, 1)
  return result
}

interface Run {
  left: number
  width: number
  kind: 'done' | 'active'
  /** 部分覆盖像素的透明度无需额外处理：用宽度表达覆盖率。 */
}

function buildRuns(columns: readonly Pixel[]): Run[] {
  const runs: Run[] = []
  let x = 0
  while (x < columns.length) {
    const pixel = columns[x] as Pixel
    if (pixel.gap) {
      x += 1
      continue
    }
    if (pixel.completed >= 1 - FULL_COVERAGE_EPSILON) {
      const left = x
      while (x < columns.length && !(columns[x] as Pixel).gap && (columns[x] as Pixel).completed >= 1 - FULL_COVERAGE_EPSILON) x += 1
      runs.push({ left, width: x - left, kind: 'done' })
      continue
    }
    if (pixel.active && pixel.completed <= 0) {
      const left = x
      while (
        x < columns.length &&
        !(columns[x] as Pixel).gap &&
        (columns[x] as Pixel).active &&
        (columns[x] as Pixel).completed <= 0
      )
        x += 1
      runs.push({ left, width: x - left, kind: 'active' })
      continue
    }
    if (pixel.active) runs.push({ left: x + pixel.completed, width: 1 - pixel.completed, kind: 'active' })
    if (pixel.completed > 0) runs.push({ left: x, width: pixel.completed, kind: 'done' })
    x += 1
  }
  return runs
}

const FILL: Record<TaskState, string> = {
  downloading: 'bg-progress-fill',
  paused: 'bg-status-paused/40',
  failed: 'bg-status-failed',
  pending: 'bg-status-queued',
  completed: 'bg-status-completed',
}

const ACTIVE_FILL: Record<TaskState, string> = {
  downloading: 'bg-progress-fill/40',
  paused: 'bg-status-paused/[0.16]',
  failed: 'bg-status-failed/40',
  pending: 'bg-status-queued/40',
  completed: 'bg-status-completed/40',
}

interface Props {
  runtime?: TaskRuntimeDto
  /** 0..1；无分段事实时退回总进度。 */
  progress: number
  state: TaskState
  /** 已知像素宽度（表格列宽推算）；缺省时按容器测量。 */
  width?: number
  className?: string
}

/** 主列表与详情共用的真实分段进度条。 */
export const SegmentProgress = memo(function SegmentProgress({ runtime, progress, state, width, className }: Props) {
  const ref = useRef<HTMLDivElement>(null)
  const [measured, setMeasured] = useState(0)
  useLayoutEffect(() => {
    if (width !== undefined) return
    const element = ref.current
    if (!element) return
    setMeasured(Math.floor(element.clientWidth))
    const observer = new ResizeObserver(() => setMeasured(Math.floor(element.clientWidth)))
    observer.observe(element)
    return () => observer.disconnect()
  }, [width])

  const pixels = Math.min(MAX_BAR_WIDTH, Math.max(0, Math.floor(width ?? measured)))
  const hasSegments = runtime !== undefined && runtime.totalBytes > 0 && runtime.segments.length > 0
  const runs = hasSegments && pixels > 0 ? buildRuns(projectPixels(runtime, pixels)) : []

  return (
    <div
      ref={ref}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(progress * 100)}
      className={cn('relative overflow-hidden rounded-[var(--fx-components-progress-radius)] bg-progress-track', className)}
      style={{ height: 'var(--fx-components-progress-height)', width: width === undefined ? undefined : pixels }}
    >
      {hasSegments ? (
        runs.map((run, index) => (
          <div
            key={index}
            className={cn('absolute top-0 h-full', run.kind === 'done' ? FILL[state] : ACTIVE_FILL[state])}
            style={{ left: run.left, width: run.width }}
          />
        ))
      ) : (
        <div
          className={cn('absolute left-0 top-0 h-full', FILL[state])}
          style={{ width: `${Math.min(1, Math.max(0, progress)) * 100}%` }}
        />
      )}
    </div>
  )
})
