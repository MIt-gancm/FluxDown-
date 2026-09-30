import { File, PanelBottom, PanelRight, X } from 'lucide-react'
import { useState } from 'react'
import { useT } from '../../../i18n'
import { Button, Icon, SegmentedTabs, Tooltip } from '../../../ui'
import { useDownloads } from '../state'
import { AdvancedTab } from './AdvancedTab'
import { GeneralTab } from './GeneralTab'
import { LogTab } from './LogTab'
import { SeedingTab } from './SeedingTab'
import { SpeedTab } from './SpeedTab'
import { useSpeedHistory } from './useSpeedHistory'
import type { DownloadTaskView } from '../model/task'

type DetailTab = 'general' | 'speed' | 'seeding' | 'log' | 'advanced'

function TaskDetail({ view }: { view: DownloadTaskView }) {
  const t = useT()
  const [tab, setTab] = useState<DetailTab>('general')
  const isBt = view.protocol === 'bt'
  const active: DetailTab = tab === 'seeding' && !isBt ? 'general' : tab
  const speedHistory = useSpeedHistory(view, active === 'speed')
  const items = [
    { value: 'general' as const, label: t('detailTabGeneral') },
    { value: 'speed' as const, label: t('detailTabSpeed') },
    ...(isBt ? [{ value: 'seeding' as const, label: t('tabSeeding') }] : []),
    { value: 'log' as const, label: t('detailTabLog') },
    { value: 'advanced' as const, label: t('detailTabAdvanced') },
  ]
  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 px-3 pt-3">
        <SegmentedTabs items={items} value={active} onValueChange={setTab} />
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-3">
        {active === 'general' ? <GeneralTab view={view} /> : null}
        {active === 'speed' ? <SpeedTab view={view} history={speedHistory} /> : null}
        {active === 'seeding' ? <SeedingTab view={view} /> : null}
        {active === 'log' ? <LogTab taskId={view.taskId} /> : null}
        {active === 'advanced' ? <AdvancedTab view={view} /> : null}
      </div>
    </div>
  )
}

/** 任务详情面板：docked = 带标题栏（切换位置 / 关闭）；sheet = 仅内容（Sheet 自带标题与关闭）。 */
export function DetailPanel({ mode }: { mode: 'docked' | 'sheet' }) {
  const t = useT()
  const { detailTaskId, byKey, closeDetail, prefs, updatePrefs } = useDownloads()
  const view = detailTaskId ? byKey.get(`l:${detailTaskId}`) : undefined
  const right = prefs.detail_placement === 'right'
  const nextPlacement = right ? 'bottom' : 'right'
  const placementLabel = t(right ? 'viewDetailBottom' : 'viewDetailRight')

  return (
    <div className="flex h-full min-h-0 flex-col bg-surface">
      {mode === 'docked' ? (
        <div className="flex min-h-control shrink-0 items-center gap-1 border-b border-hairline bg-chrome py-0.5 pl-3 pr-1">
          <div className="min-w-0 flex-1 truncate text-sm font-medium text-foreground">{t('detail')}</div>
          <Tooltip content={placementLabel}>
            <Button
              variant="ghost"
              iconOnly
              aria-label={placementLabel}
              onClick={() => updatePrefs((p) => ({ ...p, detail_placement: nextPlacement }))}
            >
              <Icon icon={right ? PanelBottom : PanelRight} />
            </Button>
          </Tooltip>
          <Tooltip content={t('close')}>
            <Button variant="ghost" iconOnly aria-label={t('close')} onClick={closeDetail}>
              <Icon icon={X} />
            </Button>
          </Tooltip>
        </div>
      ) : null}
      {view ? (
        <TaskDetail key={view.taskId} view={view} />
      ) : (
        <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6">
          <File strokeWidth={1.5} className="text-text-tertiary" style={{ width: 32, height: 32 }} />
          <div className="text-sm font-medium text-muted-foreground">{t('selectTaskHint')}</div>
        </div>
      )}
    </div>
  )
}
