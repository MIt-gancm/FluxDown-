import { useCallback, useEffect, useState } from 'react'

/** 秒级倒计时：`start(seconds)` 重置，归零后停止；返回剩余秒数（未启动 = 0）。 */
export function useCountdown(): { remaining: number; start: (seconds: number) => void } {
  const [deadline, setDeadline] = useState(0)
  const [remaining, setRemaining] = useState(0)

  useEffect(() => {
    if (deadline === 0) return
    const tick = () => {
      const left = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
      setRemaining(left)
      if (left === 0) window.clearInterval(timer)
    }
    const timer = window.setInterval(tick, 500)
    tick()
    return () => window.clearInterval(timer)
  }, [deadline])

  const start = useCallback((seconds: number) => {
    setDeadline(Date.now() + Math.max(0, seconds) * 1000)
  }, [])

  return { remaining, start }
}
