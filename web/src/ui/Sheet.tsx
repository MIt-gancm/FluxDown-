import * as RadixDialog from '@radix-ui/react-dialog'
import { X } from 'lucide-react'
import type { ReactNode } from 'react'
import { useT } from '../i18n'
import { cn } from '../lib/cn'
import { Button } from './Button'
import { Icon } from './Icon'

export type SheetSide = 'right' | 'left' | 'bottom'

const PANEL: Record<SheetSide, string> = {
  right: 'animate-fx-slide-right data-[state=closed]:animate-fx-slide-right-out inset-y-0 right-0 w-[min(420px,100vw)] border-l pt-safe pr-safe',
  left: 'animate-fx-slide-left data-[state=closed]:animate-fx-slide-left-out inset-y-0 left-0 w-[min(320px,86vw)] border-r pt-safe pl-safe',
  bottom:
    'animate-fx-slide-up data-[state=closed]:animate-fx-slide-up-out inset-x-0 bottom-0 max-h-[85dvh] rounded-t-[var(--fx-components-dialog-radius)] border-t pl-safe pr-safe',
}

/**
 * 侧边面板 / 底部抽屉：右/左侧滑入（详情面板、侧栏抽屉），或底部弹出（菜单、选择器）。
 * 移动端页面用它承载 GPUI 里「停靠面板」的内容。`title` 必填（无障碍），`hideHeader` 时视觉隐藏。
 */
export function Sheet({
  open,
  onOpenChange,
  side = 'right',
  title,
  hideHeader = false,
  footer,
  children,
  className,
  bodyClassName,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  side?: SheetSide
  title: ReactNode
  hideHeader?: boolean
  footer?: ReactNode
  children?: ReactNode
  className?: string
  bodyClassName?: string
}) {
  const t = useT()
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="animate-fx-fade data-[state=closed]:animate-fx-fade-out fixed inset-0 z-40 bg-black/40" />
        <RadixDialog.Content
          aria-describedby={undefined}
          className={cn(
            'fixed z-50 flex flex-col border-hairline bg-surface text-surface-foreground shadow-lg outline-none',
            PANEL[side],
            className,
          )}
        >
          {side === 'bottom' ? <div className="mx-auto mt-2 h-1 w-9 shrink-0 rounded-full bg-muted-foreground/30" aria-hidden /> : null}
          <div className={cn('flex shrink-0 items-center gap-2 px-4 py-2', hideHeader && 'sr-only')}>
            <RadixDialog.Title className="min-w-0 flex-1 truncate text-title font-semibold text-foreground">{title}</RadixDialog.Title>
            <RadixDialog.Close asChild>
              <Button variant="ghost" iconOnly aria-label={t('close')} title={t('close')} className="text-muted-foreground hover:text-foreground">
                <Icon icon={X} size="md" />
              </Button>
            </RadixDialog.Close>
          </div>
          <div className={cn('min-h-0 flex-1 overflow-y-auto', bodyClassName)}>{children}</div>
          {footer ? (
            <div className="shrink-0 border-t border-hairline bg-chrome px-4 py-3 pb-[max(0.75rem,env(safe-area-inset-bottom))]">{footer}</div>
          ) : (
            <div className="shrink-0 pb-safe" />
          )}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  )
}
