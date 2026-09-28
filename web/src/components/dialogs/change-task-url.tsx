// 更换任务下载源地址对话框 —— 由 changeTaskUrlStore（TaskContextMenu 触发）驱动开关。
// 成功后引擎经 WS 推送任务列表更新；此处照既有任务操作惯例再 invalidate ['tasks'] 兜底。
// 失败时把服务端透传的稳定错误码映射为本地化文案，未识别错误走兜底键展示原文。

import { useEffect, useState } from 'react'
import * as Dialog from '@radix-ui/react-dialog'
import { useMutation, useQueryClient } from '@tanstack/react-query'
import { X } from 'lucide-react'
import { api, ApiError } from '../../lib/api'
import { changeTaskUrlStore } from '../../lib/dialogs'
import type { I18nKey } from '../../lib/i18n'
import { useI18n } from '../../lib/i18n'
import { useStore } from '../../lib/ws'

/** 引擎稳定错误码 → 文案键（见 native/engine DownloadManager::change_task_url 契约）。 */
const CHANGE_URL_ERROR_KEYS: Record<string, I18nKey> = {
  'invalid-url': 'task.changeUrlErrInvalidUrl',
  'task-active': 'task.changeUrlErrTaskActive',
  'task-completed': 'task.changeUrlErrTaskCompleted',
  'bt-unsupported': 'task.changeUrlErrBtUnsupported',
  'protocol-unsupported': 'task.changeUrlErrProtocolUnsupported',
  'protocol-mismatch': 'task.changeUrlErrProtocolMismatch',
  'not-found': 'task.changeUrlErrNotFound',
}

export function ChangeTaskUrlDialog() {
  const { t } = useI18n()
  const payload = useStore(changeTaskUrlStore)
  const open = payload !== null
  const qc = useQueryClient()
  const [url, setUrl] = useState('')
  const [error, setError] = useState('')

  // 每次新请求到达时，输入框预填当前地址并清空上次错误。
  useEffect(() => {
    if (!payload) return
    setUrl(payload.url)
    setError('')
  }, [payload])

  const changeUrlMut = useMutation({
    mutationFn: ({ taskId, url }: { taskId: string; url: string }) => api.changeTaskUrl(taskId, url),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ['tasks'] })
      changeTaskUrlStore.set(null)
    },
    onError: (err: Error) => {
      if (err instanceof ApiError && err.status === 404) {
        setError(t('task.changeUrlErrNotFound'))
        return
      }
      const key = CHANGE_URL_ERROR_KEYS[err.message.trim()]
      setError(key ? t(key) : t('task.changeUrlErrUnknown', { error: err.message }))
    },
  })

  function cancel() {
    changeTaskUrlStore.set(null)
  }

  function confirm() {
    if (!payload) return
    const trimmed = url.trim()
    if (!trimmed) {
      setError(t('task.changeUrlErrInvalidUrl'))
      return
    }
    if (trimmed === payload.url) {
      changeTaskUrlStore.set(null)
      return
    }
    setError('')
    changeUrlMut.mutate({ taskId: payload.taskId, url: trimmed })
  }

  return (
    <Dialog.Root
      open={open}
      onOpenChange={(o) => {
        if (!o) cancel()
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="wbackdrop show" />
        <Dialog.Content className="dialog sm show" aria-describedby={undefined}>
          <header className="dlg-head">
            <Dialog.Title asChild>
              <b>{t('task.changeUrlTitle')}</b>
            </Dialog.Title>
            <Dialog.Close asChild>
              <button type="button" className="icon-btn sm" aria-label={t('common.close')}>
                <X size={16} />
              </button>
            </Dialog.Close>
          </header>
          <form
            onSubmit={(e) => {
              e.preventDefault()
              confirm()
            }}
          >
            <div className="dlg-body">
              <label className="field-label" htmlFor="change-url-url">
                {t('task.changeUrlLabel')}
              </label>
              <input
                id="change-url-url"
                className="text-input"
                type="text"
                spellCheck={false}
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                placeholder={t('task.changeUrlPlaceholder')}
                autoFocus
                onFocus={(e) => {
                  // 预填地址默认全选，便于直接输入替换。
                  e.target.select()
                }}
              />
              {error && <p className="mt-2 text-xs break-all text-danger">{error}</p>}
            </div>
            <footer className="dlg-foot">
              <Dialog.Close asChild>
                <button type="button" className="btn ghost">
                  {t('common.cancel')}
                </button>
              </Dialog.Close>
              <button type="submit" className="btn primary" disabled={changeUrlMut.isPending || !url.trim()}>
                {t('task.changeUrl')}
              </button>
            </footer>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
