// 事件序号连续性（对应 agent `daemon_client::forward_event`）：
//   - epoch 变化 → 必须重新 snapshot；
//   - sequence <= 游标 → 快照之前的旧帧，丢弃；
//   - sequence == 游标+1 → 接受并前移；
//   - 其他（断档）→ 必须重新 snapshot。

export interface EventCursor {
  epoch: string
  sequence: number
}

export type CursorVerdict = 'accept' | 'skip' | 'resync'

export function judgeFrame(cursor: EventCursor, frame: EventCursor): CursorVerdict {
  if (frame.epoch !== cursor.epoch) return 'resync'
  if (frame.sequence <= cursor.sequence) return 'skip'
  if (frame.sequence !== cursor.sequence + 1) return 'resync'
  return 'accept'
}
