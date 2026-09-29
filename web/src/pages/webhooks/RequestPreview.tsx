// 实时请求预览：卡片内等宽纯文本（可选中复制）+ 投递语义说明。

import { useT } from '../../i18n'
import { cn } from '../../lib/cn'
import { Card, FieldHint, FieldLabel } from '../../ui'

export function RequestPreview({ text, className }: { text: string; className?: string }) {
  const t = useT()
  return (
    <div className={cn('flex min-h-0 min-w-0 flex-col gap-1.5', className)}>
      <FieldLabel>{t('webhookPreviewTitle')}</FieldLabel>
      <Card className="min-h-0 max-h-[40dvh] overflow-auto px-3 py-2 desktop:max-h-none desktop:flex-1">
        <pre className="font-mono text-xs leading-4 break-all whitespace-pre-wrap text-foreground select-text">{text}</pre>
      </Card>
      <FieldHint className="whitespace-pre-line">{t('webhookPreviewMeta')}</FieldHint>
    </div>
  )
}
