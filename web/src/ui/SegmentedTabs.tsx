import type { ReactNode } from 'react'
import { cn } from '../lib/cn'

export interface SegmentedItem<V extends string> {
  value: V
  label: ReactNode
  disabled?: boolean
}

/**
 * 分段标签（GPUI `segmented_tabs`）：nav-hover 轨道内，选中项 surface 底 + 细阴影 + 中等字重。
 * 轨道高 `density.control`，项高再减 4；窄屏可横向滚动。
 */
export function SegmentedTabs<V extends string>({
  items,
  value,
  onValueChange,
  className,
  'aria-label': ariaLabel,
}: {
  items: readonly SegmentedItem<V>[]
  value: V
  onValueChange: (value: V) => void
  className?: string
  'aria-label'?: string
}) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      className={cn(
        'inline-flex h-control max-w-full shrink-0 items-center gap-0.5 overflow-x-auto rounded-[calc(var(--fx-components-tab-radius)+1px)] bg-nav-hover p-0.5 coarse:h-11',
        className,
      )}
    >
      {items.map((item) => {
        const active = item.value === value
        return (
          <button
            key={item.value}
            type="button"
            role="tab"
            aria-selected={active}
            disabled={item.disabled}
            onClick={() => onValueChange(item.value)}
            className={cn(
              'inline-flex h-[calc(var(--fx-density-control)-4px)] shrink-0 items-center whitespace-nowrap rounded-[var(--fx-components-tab-radius)] px-3 text-sm transition-colors coarse:h-full',
              active
                ? 'bg-surface font-medium text-foreground shadow-sm'
                : 'text-muted-foreground hover:text-foreground',
              'disabled:pointer-events-none disabled:opacity-50',
            )}
          >
            {item.label}
          </button>
        )
      })}
    </div>
  )
}
