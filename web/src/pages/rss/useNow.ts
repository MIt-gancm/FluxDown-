import { useEffect, useState } from 'react'

/** 当前 Unix 秒，按 `intervalMs` 刷新（「刚刚 / N 分钟前」这类相对时间随之更新）。 */
export function useNowSeconds(intervalMs = 30_000): number {
  const [now, setNow] = useState(() => Math.floor(Date.now() / 1000))
  useEffect(() => {
    const timer = setInterval(() => setNow(Math.floor(Date.now() / 1000)), intervalMs)
    return () => clearInterval(timer)
  }, [intervalMs])
  return now
}
