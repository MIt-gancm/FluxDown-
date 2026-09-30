import { useEffect, useReducer, useRef } from 'react'
import type { DownloadTaskView } from '../model/task'

/** 1 Hz 采样 120 点 = 近 2 分钟（对齐 GPUI `SPEED_HISTORY_CAPACITY`）。 */
export const SPEED_HISTORY_CAPACITY = 120

/**
 * 详情面板级速度采样：随 `TaskDetail`（以 taskId 为 key）挂载，切换标签不丢历史；
 * 缓冲区放在 ref 里，只有「速度」标签可见时才触发重渲染，其余标签零开销。
 */
export function useSpeedHistory(view: DownloadTaskView, visible: boolean): readonly number[] {
  const speedRef = useRef(0)
  speedRef.current = view.state === 'downloading' ? (view.speed ?? 0) : 0
  const visibleRef = useRef(visible)
  visibleRef.current = visible
  const buffer = useRef<number[]>([])
  const [, rerender] = useReducer((n: number) => n + 1, 0)

  useEffect(() => {
    const id = setInterval(() => {
      const next = buffer.current
      if (next.length >= SPEED_HISTORY_CAPACITY) next.shift()
      next.push(speedRef.current)
      if (visibleRef.current) rerender()
    }, 1000)
    return () => clearInterval(id)
  }, [])

  return buffer.current
}
