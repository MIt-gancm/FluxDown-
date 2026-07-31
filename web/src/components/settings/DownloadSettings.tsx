// 下载：默认保存目录 / 下载·上传限速 / 全局 User-Agent / 多 CDN 并发（服务器 config 表）。
import { useState } from 'react'
import { confirmDialog } from '../../lib/confirm'
import { useI18n } from '../../lib/i18n'
import type { ConfigMap } from '../../lib/types'
import { FsPicker } from '../dialogs/fs-picker'
import { UA_PRESETS } from '../../lib/ua-presets'
import { NumberFieldRow, SetRow, SetSelect, SetSwitch, TextInput } from './controls'

const KB = 1024

const CUSTOM = '__custom__'

export function DownloadSettings({
  config,
  mutate,
}: {
  config: ConfigMap
  mutate: (entries: ConfigMap) => void
}) {
  const { t } = useI18n()
  const saveDir = config.default_save_dir ?? ''
  // 与桌面端一致：KB/s 整数展示，引擎按 B/s 存储。
  const speedKB = Math.floor(Number(config.speed_limit_bytes ?? '0') / KB)
  const uploadKB = Math.floor(Number(config.upload_limit_bytes ?? '0') / KB)
  const ua = config.global_user_agent ?? ''
  const useServerTime = (config.use_server_time ?? 'false') === 'true'
  const silentSkipSelection = (config.silent_skip_selection ?? 'false') === 'true'
  const cdnMultiEnabled = (config.cdn_multi_enabled ?? '0') === '1'
  const cdnMaxNodes = Number(config.cdn_max_nodes ?? '0')
  const proxyMode = config.proxy_mode ?? 'none'
  const fileExistsBehavior = config.file_exists_behavior ?? 'rename'

  /** 开启多 CDN 并发时与代理互斥（对齐桌面端 _onCdnMultiChanged）：代理已启用则
   *  弹确认框——确认「关闭代理并开启」一次写入两个键，取消则不改任何状态。
   *  Auto 模式视同可用：CDN 聚合对直连任务仍然生效，不触发互斥。 */
  async function onCdnMultiChange(v: boolean) {
    if (!v || proxyMode === 'none' || proxyMode === 'auto') {
      mutate({ cdn_multi_enabled: v ? '1' : '0' })
      return
    }
    const ok = await confirmDialog({
      title: t('set.download.cdnMultiProxyConfirmTitle'),
      message:
        proxyMode === 'system'
          ? t('set.download.cdnMultiProxyConfirmDescSystem')
          : t('set.download.cdnMultiProxyConfirmDescManual'),
      confirmLabel: t('set.download.cdnMultiProxyConfirmDisable'),
    })
    if (ok) mutate({ proxy_mode: 'none', cdn_multi_enabled: '1' })
  }

  // 自定义模式：用户在下拉里选了"自定义"，或当前值不匹配任何预设。
  const isPreset = ua === '' || UA_PRESETS.some((p) => p.value === ua)
  const [customMode, setCustomMode] = useState(!isPreset)
  const customActive = customMode || !isPreset

  // Radix Select 把 value="" 视为"未选择"，触发器会显示空白 —— 默认项用哨兵值。
  const DEFAULT = '__default__'
  const uaOptions = [
    { label: t('set.download.uaDefault'), value: DEFAULT },
    ...UA_PRESETS,
    { label: t('common.custom'), value: CUSTOM },
  ]
  const selectValue = customActive ? CUSTOM : ua === '' ? DEFAULT : ua

  return (
    <>
      <h2 className="set-title">{t('set.download')}</h2>
      <p className="set-desc">{t('set.download.desc')}</p>
      <div className="set-group">
        <SetRow title={t('set.download.saveDir')} desc={t('set.download.saveDirDesc')}>
          <div className="dir-row" style={{ width: 300, flexShrink: 0 }}>
            <TextInput value={saveDir} onCommit={(v) => mutate({ default_save_dir: v })} />
            <FsPicker value={saveDir} onChange={(p) => mutate({ default_save_dir: p })} />
          </div>
        </SetRow>
        <NumberFieldRow
          title={t('set.download.speedLimit')}
          desc={t('set.download.speedLimitDesc')}
          value={speedKB}
          min={0}
          onCommit={(n) => mutate({ speed_limit_bytes: String(Math.max(0, Math.round(n)) * KB) })}
        />
        <NumberFieldRow
          title={t('set.download.uploadLimit')}
          desc={t('set.download.uploadLimitDesc')}
          value={uploadKB}
          min={0}
          onCommit={(n) => mutate({ upload_limit_bytes: String(Math.max(0, Math.round(n)) * KB) })}
        />
        <SetRow title={t('set.download.ua')} desc={t('set.download.uaDesc')}>
          <div style={{ display: 'flex', gap: 8, alignItems: 'center', flexShrink: 0 }}>
            {customActive && (
              <div style={{ width: 220 }}>
                <TextInput
                  value={ua}
                  placeholder={t('set.download.uaCustomPlaceholder')}
                  onCommit={(v) => mutate({ global_user_agent: v.trim() })}
                />
              </div>
            )}
            <SetSelect
              width={customActive ? 130 : 220}
              value={selectValue}
              onValueChange={(v) => {
                if (v === CUSTOM) {
                  setCustomMode(true)
                } else {
                  setCustomMode(false)
                  mutate({ global_user_agent: v === DEFAULT ? '' : v })
                }
              }}
              options={uaOptions}
            />
          </div>
        </SetRow>
        <SetRow title={t('set.download.serverTime')} desc={t('set.download.serverTimeDesc')}>
          <SetSwitch
            checked={useServerTime}
            onCheckedChange={(v) => mutate({ use_server_time: String(v) })}
          />
        </SetRow>
        {/* headless 无确认弹框：接管入口（扩展远程投递/脚本）创建的任务开启后
            跳过 BT 文件/画质的 WS 选择往返，直接按默认开始下载 */}
        <SetRow
          title={t('set.download.silentSkipSelection')}
          desc={t('set.download.silentSkipSelectionDesc')}
        >
          <SetSwitch
            checked={silentSkipSelection}
            onCheckedChange={(v) => mutate({ silent_skip_selection: String(v) })}
          />
        </SetRow>
        <SetRow title={t('set.download.fileExists')} desc={t('set.download.fileExistsDesc')}>
          <SetSelect
            value={fileExistsBehavior === 'overwrite' ? 'overwrite' : 'rename'}
            onValueChange={(v) => mutate({ file_exists_behavior: v })}
            options={[
              { value: 'rename', label: t('set.download.fileExistsRename') },
              { value: 'overwrite', label: t('set.download.fileExistsOverwrite') },
            ]}
            width={160}
          />
        </SetRow>
        <SetRow title={t('set.download.cdnMulti')} desc={t('set.download.cdnMultiDesc')}>
          <SetSwitch checked={cdnMultiEnabled} onCheckedChange={(v) => void onCdnMultiChange(v)} />
        </SetRow>
        {cdnMultiEnabled && (
          <NumberFieldRow
            title={t('set.download.cdnMaxNodes')}
            desc={t('set.download.cdnMaxNodesDesc')}
            value={cdnMaxNodes}
            min={0}
            max={8}
            onCommit={(n) => mutate({ cdn_max_nodes: String(Math.min(8, Math.max(0, n))) })}
          />
        )}
      </div>
    </>
  )
}
