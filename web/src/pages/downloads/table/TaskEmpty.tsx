import { Download } from 'lucide-react'
import { useT } from '../../../i18n'

/** 空态：32px 图标 + 标题 + 引导（与 GPUI 同文案）。 */
export function TaskEmpty() {
  const t = useT()
  return (
    <div className="flex h-full flex-col items-center justify-center gap-2 bg-surface p-6 text-center">
      <Download aria-hidden className="text-text-tertiary" strokeWidth={1.75} style={{ width: 32, height: 32 }} />
      <div className="text-sm font-medium text-foreground">{t('emptyTitle')}</div>
      <div className="text-xs text-muted-foreground">{t('emptySubtitle')}</div>
    </div>
  )
}
