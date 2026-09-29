import * as RadixSwitch from '@radix-ui/react-switch'
import { cn } from '../lib/cn'

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  id,
  'aria-label': ariaLabel,
  className,
}: {
  checked: boolean
  onCheckedChange: (checked: boolean) => void
  disabled?: boolean
  id?: string
  'aria-label'?: string
  className?: string
}) {
  return (
    <RadixSwitch.Root
      id={id}
      checked={checked}
      onCheckedChange={onCheckedChange}
      disabled={disabled}
      aria-label={ariaLabel}
      className={cn(
        'relative inline-flex h-5 w-9 shrink-0 items-center rounded-full transition-colors',
        'data-[state=checked]:bg-primary data-[state=unchecked]:bg-muted-foreground/35',
        'disabled:cursor-not-allowed disabled:opacity-50 coarse:h-6 coarse:w-11',
        className,
      )}
    >
      <RadixSwitch.Thumb className="pointer-events-none block size-4 translate-x-0.5 rounded-full bg-white shadow-sm transition-transform data-[state=checked]:translate-x-[18px] coarse:size-5 coarse:data-[state=checked]:translate-x-[22px]" />
    </RadixSwitch.Root>
  )
}
