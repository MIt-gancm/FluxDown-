import { useCallback, useEffect, useRef, useState } from 'react'
import { rpc, useAgent, useConnection, useServiceEvents } from '../../../lib/rpc'
import type { EventFrame, TaskActivityDto } from '../../../lib/rpc'
import { ActivityFeed } from './activityFeed'

function activityOf(frame: EventFrame): TaskActivityDto | null {
  const outer = frame.event as { service: string; event: { type: string; data?: unknown } }
  if (outer.service === 'agent') {
    if (outer.event.type !== 'daemon') return null
    const inner = outer.event.data as { type?: string; data?: unknown } | undefined
    return inner?.type === 'taskActivityAdded' ? (inner.data as TaskActivityDto) : null
  }
  if (outer.service === 'daemon') {
    return outer.event.type === 'taskActivityAdded' ? (outer.event.data as TaskActivityDto) : null
  }
  return null
}

/** 任务活动日志（`task_detail.rs` fetch_activity / apply_event 的等价物）。每个 taskId 单独挂载。 */
export function useActivity(taskId: string) {
  const [, setVersion] = useState(0)
  const feedRef = useRef<ActivityFeed | null>(null)
  if (feedRef.current === null) feedRef.current = new ActivityFeed(taskId)
  const feed = feedRef.current
  const phase = useConnection().phase
  const daemonConnected = useAgent((snapshot) => snapshot.daemonConnected, false)
  const online = phase === 'ready' && daemonConnected
  const onlineRef = useRef(online)
  onlineRef.current = online
  const aliveRef = useRef(true)

  const bump = useCallback(() => setVersion((v) => v + 1), [])

  const pump = useCallback(() => {
    if (!aliveRef.current || !onlineRef.current) return
    const ticket = feed.begin()
    if (!ticket) return
    bump()
    rpc.daemon.task
      .activity(ticket.query)
      .then((page) => page, () => null)
      .then((page) => {
        if (!aliveRef.current) return
        if (feed.finish(ticket, page)) {
          bump()
          pump()
        }
      })
  }, [feed, bump])

  useEffect(() => {
    aliveRef.current = true
    return () => {
      aliveRef.current = false
    }
  }, [])

  // 离线 → 挂起；重新在线 → 补齐（已加载走 afterId，否则重取最新页）。
  const wasOnline = useRef(online)
  useEffect(() => {
    if (!online) {
      feed.suspend()
      bump()
    } else if (!wasOnline.current) {
      feed.reconnect()
    }
    wasOnline.current = online
    pump()
  }, [online, feed, bump, pump])

  useServiceEvents(
    useCallback(
      (frame: EventFrame) => {
        const entry = activityOf(frame)
        if (entry && feed.add(entry)) bump()
      },
      [feed, bump],
    ),
  )

  return {
    feed,
    loadOlder: () => {
      feed.loadOlder()
      pump()
    },
    retry: () => {
      feed.retry()
      pump()
    },
  }
}
