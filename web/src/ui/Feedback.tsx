import { Loader2 } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '../lib/cn'
import { Icon } from './Icon'
import type { IconSize } from './Icon'

export function Spinner({ size = 'lg', className }: { size?: IconSize; className?: string }) {
  return <Icon icon={Loader2} size={size} className={cn('animate-spin text-muted-foreground', className)} />
}

export type BadgeTone = 'neutral' | 'accent' | 'success' | 'warning' | 'destructive'

const TONE: Record<BadgeTone, string> = {
  neutral: 'bg-muted text-muted-foreground',
  accent: 'bg-accent text-accent-text',
  success: 'bg-success/15 text-success',
  warning: 'bg-warning/15 text-warning',
  destructive: 'bg-destructive/15 text-destructive',
}

/** 徽标：`components.badge.radius`（全圆角），caption 字号。 */
export function Badge({ tone = 'neutral', children, className }: { tone?: BadgeTone; children: ReactNode; className?: string }) {
  return (
    <span className={cn('inline-flex items-center rounded-full px-2 py-0.5 text-caption font-medium tabular', TONE[tone], className)}>
      {children}
    </span>
  )
}

/** 进度条：`components.progress` 高 4、全圆角；`value` 0..1，`null` 为不定进度。 */
export function ProgressBar({ value, className }: { value: number | null; className?: string }) {
  return (
    <div
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={value === null ? undefined : Math.round(value * 100)}
      className={cn('w-full overflow-hidden rounded-[var(--fx-components-progress-radius)] bg-progress-track', className)}
      style={{ height: 'var(--fx-components-progress-height)' }}
    >
      <div
        className={cn('h-full rounded-[inherit] bg-progress-fill transition-[width]', value === null && 'w-1/3 animate-pulse')}
        style={value === null ? undefined : { width: `${Math.min(1, Math.max(0, value)) * 100}%` }}
      />
    </div>
  )
}

/** 空状态：32px 图标 + 标题 + 说明（GPUI 空状态约定）。 */
export function EmptyState({ icon, title, description, action, className }: { icon: React.ComponentType<{ className?: string; strokeWidth?: number; style?: React.CSSProperties }>; title: ReactNode; description?: ReactNode; action?: ReactNode; className?: string }) {
  const Glyph = icon
  return (
    <div className={cn('flex flex-col items-center justify-center gap-2 px-6 py-10 text-center', className)}>
      <Glyph strokeWidth={1.5} className="text-text-tertiary" style={{ width: 32, height: 32 }} />
      <div className="text-sm font-medium text-foreground">{title}</div>
      {description ? <div className="max-w-sm text-xs text-muted-foreground">{description}</div> : null}
      {action}
    </div>
  )
}
