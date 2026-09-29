// 全局 User-Agent：预设下拉 + 自定义输入（GPUI user_agent.rs）。

import { useState } from 'react'
import { useT } from '../../../../i18n'
import { UA_PRESETS as UA_PRESET_LIST } from '../../../../lib/ua-presets'
import { DropdownField, SettingsRow, TextField, WIDTH_INPUT_WIDE, optionalText, setDaemon, useDaemonValue } from '../../kit'

const UA_KEY = 'global_user_agent'

/** 与 GPUI `UA_PRESETS` 同序同值（chrome/firefox/edge/safari）。 */
const PRESET_KEYS = ['chrome', 'firefox', 'edge', 'safari'] as const
const PRESET_VALUES: Record<(typeof PRESET_KEYS)[number], string> = Object.fromEntries(
  PRESET_KEYS.map((key, index) => [key, UA_PRESET_LIST[index]!.value]),
) as Record<(typeof PRESET_KEYS)[number], string>

type UaChoice = 'default' | (typeof PRESET_KEYS)[number] | 'custom'

/** UA 字符串 → 预设键；空 = `default`，未命中 = `custom`。 */
function detectPreset(ua: string): UaChoice {
  if (ua === '') return 'default'
  return PRESET_KEYS.find((key) => PRESET_VALUES[key] === ua) ?? 'custom'
}

const cap = (key: string) => key[0]!.toUpperCase() + key.slice(1)

export function UserAgentRow() {
  const t = useT()
  const current = useDaemonValue(UA_KEY)
  const [customMode, setCustomMode] = useState(false)
  const preset = detectPreset(current)
  const customActive = customMode || preset === 'custom'
  const selected: UaChoice = customActive ? 'custom' : preset
  const title = t('userAgent')
  const options = (['default', ...PRESET_KEYS, 'custom'] as const).map((key) => ({ value: key, label: t(`userAgentPreset${cap(key)}`) }))
  return (
    <SettingsRow title={title} description={optionalText(t, 'userAgentDesc')}>
      <div className="flex w-full flex-col gap-2 desktop:items-end">
        <DropdownField
          value={selected}
          options={options}
          aria-label={title}
          onValueChange={(next) => {
            if (next === 'custom') {
              setCustomMode(true)
              return
            }
            setCustomMode(false)
            setDaemon(UA_KEY, next === 'default' ? '' : PRESET_VALUES[next])
          }}
        />
        {customActive ? (
          <TextField value={current} placeholder={t('userAgentPlaceholder')} aria-label={title} className={WIDTH_INPUT_WIDE} onCommit={(next) => setDaemon(UA_KEY, next)} />
        ) : null}
      </div>
    </SettingsRow>
  )
}
