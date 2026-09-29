import type { ReactNode } from 'react'

/** 登录 / 首次运行向导的共用外框：居中卡片，移动端占满宽并避开安全区。 */
export function AuthCard({ title, subtitle, children }: { title: string; subtitle: string; children: ReactNode }) {
  return (
    <div className="flex min-h-dvh w-full items-center justify-center bg-background px-4 pt-[max(1rem,env(safe-area-inset-top))] pb-[max(1rem,env(safe-area-inset-bottom))]">
      <div className="w-full max-w-[400px] rounded-[var(--fx-components-card-radius)] border border-hairline bg-surface p-6 narrow:p-4">
        <div className="mb-4 flex items-center gap-3">
          <img src="/favicon.svg" alt="" className="size-9 shrink-0" draggable={false} />
          <div className="min-w-0">
            <h1 className="truncate text-title font-semibold text-foreground">{title}</h1>
            <p className="text-xs text-muted-foreground">{subtitle}</p>
          </div>
        </div>
        {children}
      </div>
    </div>
  )
}
