import * as RadixTooltip from '@radix-ui/react-tooltip'
import type { ReactNode } from 'react'
import { useIsCoarsePointer } from './hooks'

/** 全局唯一 Provider（AppProviders 挂载）。skipDelay 让相邻气泡连续出现。 */
export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <RadixTooltip.Provider delayDuration={500} skipDelayDuration={300}>
      {children}
    </RadixTooltip.Provider>
  )
}

/** 提示气泡；触屏无 hover，直接不渲染。`components.tooltip.radius`。 */
export function Tooltip({ content, children, side = 'bottom' }: { content: ReactNode; children: ReactNode; side?: 'top' | 'right' | 'bottom' | 'left' }) {
  const coarse = useIsCoarsePointer()
  if (coarse || content === null || content === undefined || content === '') return <>{children}</>
  return (
    <RadixTooltip.Root>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          side={side}
          sideOffset={6}
          collisionPadding={8}
          className="animate-fx-pop z-[60] max-w-xs rounded-[var(--fx-components-tooltip-radius)] bg-foreground px-2 py-1 text-xs text-background shadow-md"
        >
          {content}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  )
}
