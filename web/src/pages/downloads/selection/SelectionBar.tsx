import { Download, Pause, Play, Trash2, X } from 'lucide-react'
import type { ReactNode } from 'react'
import { useT } from '../../../i18n'
import { cn } from '../../../lib/cn'
import { ActionMenu, Icon, Tooltip } from '../../../ui'
import type { MenuEntry } from '../../../ui'
import { confirmDeleteWithFiles, deleteViews, downloadViewsFiles, pauseViews, resumeViews } from '../model/actions'
import { useDownloads } from '../state'

const BTN =
  'inline-flex h-control min-w-control shrink-0 items-center justify-center gap-1 rounded-md px-1.5 text-sm text-foreground hover:bg-nav-hover active:bg-nav-selected disabled:pointer-events-none disabled:opacity-50 mobile:min-h-touch mobile:min-w-touch'

function Action({ label, onClick, disabled, children }: { label: string; onClick?: () => void; disabled?: boolean; children: ReactNode }) {
  return (
    <Tooltip content={label} side="top">
      <button type="button" aria-label={label} disabled={disabled} onClick={onClick} className={BTN}>
        {children}
      </button>
    </Tooltip>
  )
}

const Sep = () => <span className="mx-1 h-4 w-px shrink-0 bg-hairline" />

/** 表格区域底部浮动选择条：选中 ≥1 项时出现，承载批量操作。 */
export function SelectionBar() {
  const t = useT()
  const { summary, selectedViews, clearSelection } = useDownloads()
  if (!summary.any) return null

  const deleteEntries: MenuEntry[] = [
    { type: 'item', key: 'delete-task', label: t('deleteTask'), icon: Trash2, onSelect: () => void deleteViews(selectedViews, false) },
    { type: 'item', key: 'delete-files', label: t('deleteTaskAndFile'), icon: Trash2, onSelect: () => void confirmDeleteWithFiles(selectedViews) },
  ]

  return (
    <div
      className={cn(
        'absolute bottom-4 left-1/2 z-20 flex h-9 -translate-x-1/2 items-center gap-0.5 overflow-x-auto rounded-lg border border-hairline bg-surface px-2 text-xs shadow-md',
        'max-w-[calc(100vw-16px)] [scrollbar-width:none]',
        'mobile:fixed mobile:bottom-[calc(var(--fx-bottom-bar,0px)+0.5rem)] mobile:h-12 mobile:px-1.5',
      )}
    >
      <span className="shrink-0 whitespace-nowrap px-1 tabular-nums text-foreground">{t('selectedCount', { n: summary.count })}</span>
      <Sep />
      {summary.anyResumable && (
        <Action label={t('resume')} onClick={() => void resumeViews(selectedViews)}>
          <Icon icon={Play} size="md" />
        </Action>
      )}
      {summary.anyActive && (
        <Action label={t('pause')} onClick={() => void pauseViews(selectedViews)}>
          <Icon icon={Pause} size="md" />
        </Action>
      )}
      {summary.anyLocal && (
        <Action label={t('webDownloadFile')} disabled={!summary.allDownloadable} onClick={() => downloadViewsFiles(selectedViews)}>
          <Icon icon={Download} size="md" />
        </Action>
      )}
      <ActionMenu
        title={t('delete')}
        align="start"
        entries={deleteEntries}
        trigger={
          <button type="button" aria-label={t('delete')} className={cn(BTN, 'text-destructive')}>
            <Icon icon={Trash2} size="md" />
          </button>
        }
      />
      <Sep />
      <Action label={t('deselectAll')} onClick={clearSelection}>
        <Icon icon={X} size="md" />
      </Action>
    </div>
  )
}
