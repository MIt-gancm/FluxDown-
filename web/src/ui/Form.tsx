// 表单原语（GPUI kit：form / form_field / form_row / field_* / option_row / option_group / card）。
// 所有对话框与表单页统一用这些块拼装，保证间距、标签、说明在各处一致。

import type { ReactNode } from 'react'
import { cn } from '../lib/cn'

/** 卡片：surface 底 + hairline 边 + `components.card.radius`，无阴影。 */
export function Card({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div
      className={cn('rounded-[var(--fx-components-card-radius)] border border-hairline bg-surface text-surface-foreground', className)}
    >
      {children}
    </div>
  )
}

/** 表单整体：纵向排列，字段组间距 `spacing.lg`。 */
export function Form({ children, className, onSubmit }: { children: ReactNode; className?: string; onSubmit?: (event: React.FormEvent<HTMLFormElement>) => void }) {
  return (
    <form
      className={cn('flex w-full flex-col gap-4', className)}
      onSubmit={(event) => {
        event.preventDefault()
        onSubmit?.(event)
      }}
    >
      {children}
    </form>
  )
}

/** 字段标签：12px 中等字重、正文色。 */
export function FieldLabel({ children, htmlFor, className }: { children: ReactNode; htmlFor?: string; className?: string }) {
  return (
    <label htmlFor={htmlFor} className={cn('text-xs font-medium text-foreground', className)}>
      {children}
    </label>
  )
}

/** 字段说明：12px 三级文字色。 */
export function FieldHint({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('text-xs text-text-tertiary', className)}>{children}</div>
}

/** 字段错误：12px 危险色。 */
export function FieldError({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <div role="alert" className={cn('text-xs text-destructive', className)}>
      {children}
    </div>
  )
}

/** 标准字段：标签 → 控件 → 可选说明/错误（间距 6px）。 */
export function FormField({
  label,
  htmlFor,
  hint,
  error,
  children,
  className,
}: {
  label: ReactNode
  htmlFor?: string
  hint?: ReactNode
  error?: ReactNode
  children: ReactNode
  className?: string
}) {
  return (
    <div className={cn('flex w-full min-w-0 flex-col gap-1.5', className)}>
      <FieldLabel htmlFor={htmlFor}>{label}</FieldLabel>
      {children}
      {error ? <FieldError>{error}</FieldError> : hint ? <FieldHint>{hint}</FieldHint> : null}
    </div>
  )
}

/** 同行并排字段（等分宽度）；窄屏自动纵向堆叠。 */
export function FormRow({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn('grid w-full grid-cols-1 gap-3 min-[560px]:grid-flow-col min-[560px]:auto-cols-fr', className)}>{children}</div>
}

/** 输入框 + 同行操作按钮（浏览 / 验证）：输入框吃满剩余宽度。 */
export function InputWithAction({ input, action, className }: { input: ReactNode; action: ReactNode; className?: string }) {
  return (
    <div className={cn('flex w-full items-center gap-2', className)}>
      <div className="min-w-0 flex-1">{input}</div>
      <div className="shrink-0">{action}</div>
    </div>
  )
}

/** 开关行：左侧标题 + 说明，右侧控件。放进 [`OptionGroup`] 使用。 */
export function OptionRow({
  title,
  description,
  control,
  className,
}: {
  title: ReactNode
  description?: ReactNode
  control: ReactNode
  className?: string
}) {
  return (
    <div className={cn('flex w-full items-center gap-4 px-3 py-2.5 coarse:min-h-touch', className)}>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="text-sm text-foreground">{title}</div>
        {description ? <div className="text-xs text-muted-foreground">{description}</div> : null}
      </div>
      <div className="shrink-0">{control}</div>
    </div>
  )
}

/** 选项分组：一张 Card，行间 hairline 分隔（分组表单风格）。 */
export function OptionGroup({ children, className }: { children: ReactNode; className?: string }) {
  return (
    <Card className={cn('flex w-full flex-col overflow-hidden [&>*+*]:border-t [&>*+*]:border-hairline', className)}>
      {children}
    </Card>
  )
}

/** 分组标题（页面内 section 头，`density.sectionHeader` 高）。 */
export function SectionHeader({ children, className, trailing }: { children: ReactNode; className?: string; trailing?: ReactNode }) {
  return (
    <div className={cn('flex min-h-section-header items-center justify-between gap-2 text-caption font-medium text-text-tertiary', className)}>
      <span className="min-w-0 truncate">{children}</span>
      {trailing}
    </div>
  )
}
