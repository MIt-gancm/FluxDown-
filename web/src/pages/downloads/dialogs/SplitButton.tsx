// 分体按钮（GPUI `DropdownButton`）：主操作 + 右侧箭头打开队列菜单。移动端菜单为底部面板。

import { ChevronDown } from 'lucide-react'
import { Button, ActionMenu, Icon, Tooltip } from '../../../ui'
import type { ButtonVariant, MenuEntry } from '../../../ui'
import { cn } from '../../../lib/cn'

export function SplitButton({
  variant = 'outline',
  label,
  tooltip,
  disabled,
  loading,
  menu,
  menuTitle,
  menuLabel,
  onClick,
  className,
}: {
  variant?: ButtonVariant
  label: string
  tooltip?: string
  disabled?: boolean
  loading?: boolean
  menu: readonly MenuEntry[]
  menuTitle: string
  menuLabel: string
  onClick: () => void
  className?: string
}) {
  const divider = variant === 'primary' ? 'border-l border-primary-foreground/25' : ''
  return (
    <div className={cn('inline-flex min-w-0', className)}>
      <Tooltip content={tooltip}>
        <Button variant={variant} disabled={disabled} loading={loading} onClick={onClick} className="min-w-0 flex-1 rounded-r-none">
          <span className="truncate">{label}</span>
        </Button>
      </Tooltip>
      <ActionMenu
        title={menuTitle}
        entries={menu}
        trigger={
          <Button
            variant={variant}
            iconOnly
            disabled={disabled || loading || menu.length === 0}
            title={menuLabel}
            aria-label={menuLabel}
            className={cn('rounded-l-none', variant === 'outline' && 'border-l-0', divider)}
          >
            <Icon icon={ChevronDown} />
          </Button>
        }
      />
    </div>
  )
}
