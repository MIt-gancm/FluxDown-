import type { ReactNode } from 'react'
import { cn } from '../../../lib/cn'

/**
 * 详情键值表的一行（GPUI `detail_row`）：标签 xs 三级文字、固定 112px；值 sm 正文色、等宽数字。
 * 窄屏（<=820）标签在上、值在下。
 */
export function DetailRow({ label, children, className }: { label: ReactNode; children: ReactNode; className?: string }) {
  return (
    <div className={cn('flex min-h-control items-center gap-3 py-0.5 mobile:flex-col mobile:items-start mobile:gap-0.5 mobile:py-1.5', className)}>
      <div className="w-28 shrink-0 text-xs text-text-tertiary">{label}</div>
      <div className="tabular min-w-0 flex-1 break-words text-sm text-foreground mobile:w-full">{children}</div>
    </div>
  )
}

/** 面板内小号说明文字。 */
export function Note({ children, tone = 'muted', className }: { children: ReactNode; tone?: 'muted' | 'warning' | 'destructive'; className?: string }) {
  return (
    <div
      className={cn(
        'text-xs',
        tone === 'muted' && 'text-muted-foreground',
        tone === 'warning' && 'text-warning',
        tone === 'destructive' && 'text-destructive',
        className,
      )}
    >
      {children}
    </div>
  )
}
