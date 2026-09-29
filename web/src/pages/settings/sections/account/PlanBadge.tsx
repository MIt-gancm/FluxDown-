// 套餐徽标（GPUI profile.rs `plan_tag` / Flutter `_PlanTag`）：outline | solid | medal | ribbon | plain，纯色无渐变。

import { Crown } from 'lucide-react'
import type { CSSProperties } from 'react'
import type { CloudPlan } from '../../../../lib/rpc'

/** 接受可选 `#` 前缀的 6 位 `RRGGBB` 或 8 位 `AARRGGBB`，返回 CSS 颜色；非法返回 null。 */
function parseBadgeColor(value: string): string | null {
  const hex = value.trim().replace(/^#/, '')
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null
  if (hex.length === 6) return `#${hex}`
  if (hex.length === 8) {
    const alpha = Number.parseInt(hex.slice(0, 2), 16) / 255
    const r = Number.parseInt(hex.slice(2, 4), 16)
    const g = Number.parseInt(hex.slice(4, 6), 16)
    const b = Number.parseInt(hex.slice(6, 8), 16)
    return `rgb(${r} ${g} ${b} / ${alpha.toFixed(3)})`
  }
  return null
}

/** `{badge} No.{ordinal 补零到 digits(1..=6)}`。 */
function badgeText(plan: CloudPlan, ordinal: number | null): string | null {
  const base = plan.badge?.trim()
  if (!base) return null
  if (plan.badgeNumbered && ordinal !== null) {
    const width = Math.min(6, Math.max(1, plan.badgeNumberDigits))
    return `${base} No.${String(ordinal).padStart(width, '0')}`
  }
  return base
}

const TEXT = 'text-caption leading-[14px]'

export function PlanBadge({ plan, ordinal }: { plan: CloudPlan; ordinal: number | null }) {
  const text = badgeText(plan, ordinal)
  if (!text) return null
  const color = parseBadgeColor(plan.badgeColor) ?? 'var(--fx-colors-accent-foreground, currentColor)'
  const tint = (alpha: number): CSSProperties['background'] => `color-mix(in srgb, ${color} ${alpha * 100}%, transparent)`
  const pill = 'inline-flex shrink-0 items-center rounded-full'
  switch (plan.badgeStyle) {
    case 'outline':
      return (
        <span className={`${pill} gap-0.5 border px-2 ${TEXT} font-semibold`} style={{ color, borderColor: color, background: tint(0.08) }}>
          <Crown className="size-3" strokeWidth={1.75} />
          {text}
        </span>
      )
    case 'solid':
      return (
        <span className={`${pill} px-2 ${TEXT} font-semibold text-white`} style={{ background: color }}>
          {text}
        </span>
      )
    case 'medal':
      return (
        <span className={`${pill} overflow-hidden border ${TEXT} font-semibold`} style={{ color, borderColor: color }}>
          <span className="flex items-center self-stretch px-1 text-white" style={{ background: color }}>
            <Crown className="size-3" strokeWidth={1.75} />
          </span>
          <span className="px-1">{text}</span>
        </span>
      )
    case 'ribbon':
      return (
        <span className={`${pill} px-2 ${TEXT} font-semibold text-white`} style={{ background: color }}>
          {text}
        </span>
      )
    default:
      return (
        <span className={`${pill} px-1.5 ${TEXT} font-semibold`} style={{ color, background: tint(0.12) }}>
          {text}
        </span>
      )
  }
}
