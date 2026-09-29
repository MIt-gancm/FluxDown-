// 自定义请求头编辑：名称 | 值 | 删除，行尾「添加」。

import { Plus, X } from 'lucide-react'
import { useT } from '../../i18n'
import { Button, FormField, Icon, Input } from '../../ui'

export interface HeaderRow {
  id: number
  key: string
  value: string
}

export function HeadersEditor({
  rows,
  onChange,
  onAdd,
}: {
  rows: readonly HeaderRow[]
  onChange: (rows: HeaderRow[]) => void
  onAdd: () => void
}) {
  const t = useT()
  return (
    <FormField label={t('webhookFieldHeaders')}>
      <div className="flex w-full flex-col gap-2">
        {rows.map((row) => (
          <div key={row.id} className="flex w-full items-center gap-2">
            <Input
              className="w-[36%] shrink-0"
              value={row.key}
              placeholder={t('webhookHeaderName')}
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              aria-label={t('webhookHeaderName')}
              onChange={(event) => onChange(rows.map((entry) => (entry.id === row.id ? { ...entry, key: event.target.value } : entry)))}
            />
            <Input
              className="min-w-0 flex-1"
              value={row.value}
              placeholder={t('webhookHeaderValue')}
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
              aria-label={t('webhookHeaderValue')}
              onChange={(event) => onChange(rows.map((entry) => (entry.id === row.id ? { ...entry, value: event.target.value } : entry)))}
            />
            <Button
              variant="ghost"
              iconOnly
              aria-label={t('webhookRowDelete')}
              title={t('webhookRowDelete')}
              className="text-muted-foreground hover:text-destructive"
              onClick={() => onChange(rows.filter((entry) => entry.id !== row.id))}
            >
              <Icon icon={X} size="md" />
            </Button>
          </div>
        ))}
        <div>
          <Button icon={Plus} onClick={onAdd}>
            {t('webhookAddHeader')}
          </Button>
        </div>
      </div>
    </FormField>
  )
}
