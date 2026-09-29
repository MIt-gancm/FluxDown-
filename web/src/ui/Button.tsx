import { forwardRef } from 'react'
import type { ButtonHTMLAttributes, ReactNode } from 'react'
import { Loader2 } from 'lucide-react'
import type { LucideIcon } from 'lucide-react'
import { cn } from '../lib/cn'
import { Icon } from './Icon'

/** 对应 gpui-component 的 primary / outline / ghost / danger。 */
export type ButtonVariant = 'primary' | 'outline' | 'ghost' | 'danger'

const VARIANT: Record<ButtonVariant, string> = {
  primary: 'bg-primary text-primary-foreground hover:bg-primary/90 active:bg-primary/80',
  outline: 'border border-border bg-transparent text-foreground hover:bg-row-hover active:bg-nav-hover',
  ghost: 'bg-transparent text-foreground hover:bg-nav-hover active:bg-nav-selected',
  danger: 'bg-destructive text-destructive-foreground hover:bg-destructive/90 active:bg-destructive/80',
}

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  /** 前置图标。 */
  icon?: LucideIcon
  /** 仅图标：`density.control` 见方（触屏放大到 44px）。 */
  iconOnly?: boolean
  loading?: boolean
  children?: ReactNode
}

/**
 * 统一控件：`density.control` 高、13px 字、`radius.md`。触屏下最小 44px 命中区。
 * 不要在页面里再自定高度。
 */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = 'outline', icon, iconOnly = false, loading = false, className, disabled, children, type = 'button', ...rest },
  ref,
) {
  return (
    <button
      ref={ref}
      type={type}
      disabled={disabled || loading}
      className={cn(
        'inline-flex h-control shrink-0 select-none items-center justify-center gap-1.5 whitespace-nowrap rounded-md text-sm font-normal transition-colors',
        'disabled:pointer-events-none disabled:opacity-50 coarse:min-h-touch',
        iconOnly ? 'w-control coarse:min-w-touch p-0' : 'px-2.5',
        VARIANT[variant],
        className,
      )}
      {...rest}
    >
      {loading ? <Icon icon={Loader2} className="animate-spin" /> : icon ? <Icon icon={icon} /> : null}
      {children}
    </button>
  )
})
