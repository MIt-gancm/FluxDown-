// 列表编辑行：条目堆叠（每条一行 + 删除图标钮），底部输入 + 添加。

import { Trash2 } from 'lucide-react'
import { useState } from 'react'
import { useT } from '../../../../i18n'
import { Button, Icon, Input } from '../../../../ui'
import { SettingsRow, optionalText, setDaemon, useDaemonValue } from '../../kit'
import { listEntries, listToStored } from './listFormat'
import type { ListFormat } from './listFormat'

export function ListEditorRow({
  configKey,
  titleKey,
  descKey,
  placeholderKey,
  format,
  disabled,
}: {
  configKey: string
  titleKey: string
  descKey: string
  placeholderKey: string
  format: ListFormat
  disabled?: boolean
}) {
  const t = useT()
  const stored = useDaemonValue(configKey)
  const entries = listEntries(format, stored)
  const [draft, setDraft] = useState('')
  const title = t(titleKey)
  // placeholder 多行示例仅取首行提示
  const placeholder = t(placeholderKey).split('\n')[0]

  const commit = (next: string[]) => setDaemon(configKey, listToStored(format, next))
  const add = () => {
    const values = listEntries(format, draft)
    if (values.length === 0) return
    commit([...entries, ...values])
    setDraft('')
  }

  return (
    <SettingsRow title={title} description={optionalText(t, descKey)} vertical disabled={disabled}>
      <div className="flex w-full flex-col gap-2">
        {entries.length > 0 ? (
          <ul className="flex flex-col divide-y divide-hairline rounded-md border border-hairline">
            {entries.map((entry, index) => (
              <li key={`${index}:${entry}`} className="flex items-center gap-2 py-1 pr-1 pl-3">
                <span className="min-w-0 flex-1 break-all py-1 font-mono text-xs text-foreground">{entry}</span>
                <Button
                  variant="ghost"
                  iconOnly
                  className="text-muted-foreground hover:text-destructive"
                  aria-label={`${t('delete')} ${entry}`}
                  title={t('delete')}
                  onClick={() => commit(entries.filter((_, i) => i !== index))}
                >
                  <Icon icon={Trash2} size="md" />
                </Button>
              </li>
            ))}
          </ul>
        ) : null}
        <div className="flex items-center gap-2">
          <Input
            aria-label={title}
            value={draft}
            placeholder={placeholder}
            autoComplete="off"
            spellCheck={false}
            className="min-w-0 flex-1"
            onChange={(event) => setDraft(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault()
                add()
              }
            }}
          />
          <Button variant="outline" disabled={draft.trim() === ''} onClick={add}>
            {t('webListAdd')}
          </Button>
        </div>
      </div>
    </SettingsRow>
  )
}
