import * as RadixCheckbox from '@radix-ui/react-checkbox'
import { Check, Minus } from 'lucide-react'
import type { ReactNode } from 'react'
import { cn } from '../lib/cn'
import { Icon } from './Icon'

export type CheckState = boolean | 'indeterminate'

/** `density.checkMark` 见方、`components.checkbox.radius`。 */
export function Checkbox({
  checked,
  onCheckedChange,
  disabled,
  id,
  'aria-label': ariaLabel,
  className,
}: {
  checked: CheckState
  onCheckedChange?: (checked: boolean) => void
  disabled?: boolean
  id?: string
  'aria-label'?: string
  className?: string
}) {
  return (
    <RadixCheckbox.Root
      id={id}
      checked={checked}
      disabled={disabled}
      aria-label={ariaLabel}
      onCheckedChange={(next) => onCheckedChange?.(next === true)}
      style={{
        width: 'var(--fx-density-check-mark)',
        height: 'var(--fx-density-check-mark)',
        borderRadius: 'var(--fx-components-checkbox-radius)',
      }}
      className={cn(
        'inline-flex shrink-0 items-center justify-center border border-input bg-transparent transition-colors',
        'data-[state=checked]:border-primary data-[state=checked]:bg-primary data-[state=indeterminate]:border-primary data-[state=indeterminate]:bg-primary',
        'text-primary-foreground disabled:cursor-not-allowed disabled:opacity-50',
        className,
      )}
    >
      <RadixCheckbox.Indicator>
        <Icon icon={checked === 'indeterminate' ? Minus : Check} size="sm" />
      </RadixCheckbox.Indicator>
    </RadixCheckbox.Root>
  )
}

/** 整行可点的复选行（GPUI `check_row`）。 */
export function CheckRow({
  checked,
  onCheckedChange,
  children,
  disabled,
  className,
}: {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  children: ReactNode
  disabled?: boolean
  className?: string
}) {
  return (
    <label
      className={cn(
        'flex min-h-control cursor-pointer items-center gap-2 rounded-md px-1 text-sm hover:bg-row-hover coarse:min-h-touch',
        disabled && 'cursor-not-allowed opacity-50',
        className,
      )}
    >
      <Checkbox checked={checked} onCheckedChange={onCheckedChange} disabled={disabled} />
      <span className="min-w-0 flex-1">{children}</span>
    </label>
  )
}
