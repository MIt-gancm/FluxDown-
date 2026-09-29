// 顶栏搜索框（GPUI title_bar.rs `render_search`）：chrome 上的浅色胶囊；聚焦变 surface 底 + 强调描边；
// 未输入时右侧显示快捷键徽标。输入 150ms 防抖后写入查询；Esc 清空并失焦。
// 快捷键：Ctrl/Cmd+F、无输入焦点时的 `/` 聚焦搜索框（仅这两处 preventDefault）。

import { useCallback, useEffect, useRef, useState } from 'react'
import { Search, X } from 'lucide-react'
import { useT } from '../../../i18n'
import { cn } from '../../../lib/cn'
import { Icon } from '../../../ui'
import { useDownloads } from '../state'

const DEBOUNCE_MS = 150

const IS_MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent)
/** GPUI `SEARCH_SHORTCUT_HINT`。 */
const SHORTCUT_HINT = IS_MAC ? '⌘F' : 'Ctrl+F'

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (target.isContentEditable) return true
  const tag = target.tagName
  return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT'
}

export function SearchBox() {
  const t = useT()
  const { query, setQuery } = useDownloads()
  const [value, setValue] = useState(query)
  const inputRef = useRef<HTMLInputElement>(null)
  const timer = useRef<number | null>(null)
  // 最近一次由本框写入的查询：与之不同的 `query` 变化视为外部改写（如清空筛选），需回灌到输入框。
  const lastSent = useRef(query)

  const cancelTimer = useCallback(() => {
    if (timer.current !== null) {
      window.clearTimeout(timer.current)
      timer.current = null
    }
  }, [])

  const commit = useCallback(
    (next: string) => {
      cancelTimer()
      lastSent.current = next
      setQuery(next)
    },
    [cancelTimer, setQuery],
  )

  useEffect(() => {
    if (query !== lastSent.current) {
      lastSent.current = query
      cancelTimer()
      setValue(query)
    }
  }, [query, cancelTimer])

  useEffect(() => cancelTimer, [cancelTimer])

  const onChange = (next: string) => {
    setValue(next)
    cancelTimer()
    timer.current = window.setTimeout(() => {
      timer.current = null
      lastSent.current = next
      setQuery(next)
    }, DEBOUNCE_MS)
  }

  const clear = () => {
    setValue('')
    commit('')
  }

  useEffect(() => {
    const focusInput = () => {
      const input = inputRef.current
      if (!input) return
      input.focus()
      input.select()
    }
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.isComposing || event.defaultPrevented) return
      const key = event.key.toLowerCase()
      if (key === 'f' && (event.ctrlKey || event.metaKey) && !event.altKey && !event.shiftKey) {
        event.preventDefault()
        focusInput()
        return
      }
      if (event.key === '/' && !event.ctrlKey && !event.metaKey && !event.altKey && !isEditableTarget(event.target)) {
        event.preventDefault()
        focusInput()
      }
    }
    document.addEventListener('keydown', onKeyDown)
    return () => document.removeEventListener('keydown', onKeyDown)
  }, [])

  return (
    <div
      onClick={() => inputRef.current?.focus()}
      className={cn(
        'flex h-control min-w-0 flex-1 cursor-text items-center gap-1 rounded-md border border-transparent bg-nav-hover pl-2 pr-1 transition-colors coarse:min-h-touch',
        'not-focus-within:hover:bg-nav-selected focus-within:border-primary/60 focus-within:bg-surface',
        'desktop:min-w-40 desktop:flex-[0_1_280px]',
      )}
    >
      <Icon icon={Search} size="md" className="text-muted-foreground" />
      <input
        ref={inputRef}
        type="text"
        role="searchbox"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === 'Escape') {
            // 原下载页 `escape` 语义：清空查询并把焦点交回页面。
            event.preventDefault()
            clear()
            event.currentTarget.blur()
          }
        }}
        placeholder={t('searchTasksPlaceholder')}
        aria-label={t('searchTasksPlaceholder')}
        enterKeyHint="search"
        autoComplete="off"
        autoCorrect="off"
        spellCheck={false}
        className="h-full min-w-0 flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-text-tertiary"
      />
      {value !== '' ? (
        <button
          type="button"
          aria-label={t('webSearchClear')}
          onClick={(event) => {
            event.stopPropagation()
            clear()
            inputRef.current?.focus()
          }}
          className="inline-flex size-5 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-row-hover hover:text-foreground coarse:size-9"
        >
          <Icon icon={X} size="md" />
        </button>
      ) : (
        <span className="shrink-0 rounded-sm border border-hairline bg-surface px-1 text-caption text-text-tertiary coarse:hidden mobile:hidden">
          {SHORTCUT_HINT}
        </span>
      )}
    </div>
  )
}
