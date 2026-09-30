// `resolveRowOrder` 的 React 绑定：判定结构性变化、保存已应用顺序、顺序过期时在保持期结束后强制重排。

import { useEffect, useMemo, useRef, useState } from 'react'
import { holdExpiresAt, isMenuOpen, pointerActivity, resolveRowOrder } from './rowOrder'
import type { OrderResult, OrderState } from './rowOrder'

/** 菜单打开时的复查间隔。 */
const MENU_RECHECK_MS = 300

interface Structure {
  deps: readonly unknown[]
  keys: ReadonlySet<string>
}

const sameDeps = (left: readonly unknown[], right: readonly unknown[]): boolean =>
  left.length === right.length && left.every((value, index) => Object.is(value, right[index]))

function sameKeySet(previous: ReadonlySet<string>, next: ReadonlySet<string>): boolean {
  if (previous.size !== next.size) return false
  for (const key of next) if (!previous.has(key)) return false
  return true
}

/**
 * @param sorted 按当前排序键排好、已应用筛选与搜索的行。
 * @param sources 筛选前的来源行（key 集合变化 = 任务增删 = 结构性变化）。
 * @param structuralDeps 变化即结构性变化的输入（侧栏选择、搜索词、排序偏好…），定长。
 * @param dynamicKey 当前排序键是动态键（速度 / 进度）。
 */
export function useStableRowOrder<T extends { key: string }>(
  sorted: readonly T[],
  sources: readonly { key: string }[],
  structuralDeps: readonly unknown[],
  dynamicKey: boolean,
): readonly T[] {
  const [tick, setTick] = useState(0)
  const structure = useRef<Structure | null>(null)
  const applied = useRef<OrderState | null>(null)
  const forced = useRef(0)
  const latest = useRef({ sources, structuralDeps, dynamicKey })
  latest.current = { sources, structuralDeps, dynamicKey }

  const result = useMemo<OrderResult<T>>(() => {
    const input = latest.current
    const keys = new Set(input.sources.map((source) => source.key))
    const previous = structure.current
    const structural =
      previous === null || !sameDeps(previous.deps, input.structuralDeps) || !sameKeySet(previous.keys, keys)
    const force = tick !== forced.current
    forced.current = tick
    const out = resolveRowOrder({
      sorted,
      previous: applied.current,
      structural,
      force,
      now: Date.now(),
      lastPointerAt: pointerActivity.lastAt,
      menuOpen: isMenuOpen(),
      dynamicKey: input.dynamicKey,
    })
    structure.current = { deps: input.structuralDeps, keys }
    applied.current = out.state
    return out
  }, [sorted, tick])

  useEffect(() => {
    if (!result.stale) return
    let timer: ReturnType<typeof setTimeout> | undefined
    const arm = () => {
      if (isMenuOpen()) {
        timer = setTimeout(arm, MENU_RECHECK_MS)
        return
      }
      const wait =
        holdExpiresAt({
          lastPointerAt: pointerActivity.lastAt,
          dynamicKey: latest.current.dynamicKey,
          lastReorderAt: result.state.lastReorderAt,
        }) - Date.now()
      if (wait <= 0) setTick((value) => value + 1)
      else timer = setTimeout(arm, wait)
    }
    arm()
    return () => clearTimeout(timer)
  }, [result])

  return result.rows
}
