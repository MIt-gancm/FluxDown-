// 下载页命令：对应 GPUI 的 DownloadsCommand（downloads_port.rs），全部走 agent /rpc。
// 本地任务 → `daemon.*`；远程任务 → `agent.remote.command`。失败统一 toast。
// 桌面专属的「打开文件 / 在文件夹中显示」在 Web 里替换为浏览器下载已完成文件。

import { copyText } from '../../../lib/copy'
import { t } from '../../../i18n'
import { downloadTaskFile, rpc } from '../../../lib/rpc'
import type { CreateTaskRequest, RemoteCommandParams } from '../../../lib/rpc'
import { confirmDialog, toast } from '../../../ui'
import { toastRpcError } from './errors'
import { PLUGIN_ERROR_PREFIX, shareUrl } from './task'
import type { DownloadTaskView } from './task'

async function guarded(work: () => Promise<unknown>): Promise<boolean> {
  try {
    await work()
    return true
  } catch (error) {
    toastRpcError(error)
    return false
  }
}

async function guardedAll(jobs: readonly (() => Promise<unknown>)[]): Promise<boolean> {
  if (jobs.length === 0) return true
  const results = await Promise.allSettled(jobs.map((job) => job()))
  const failed = results.find((result) => result.status === 'rejected')
  if (failed && failed.status === 'rejected') {
    toastRpcError(failed.reason)
    return false
  }
  return true
}

/**
 * 远程任务的云端命令是否适用（镜像 crates/downloads/src/model/dispatch.rs
 * `remote_action_applies`）：状态未知一律不控制；Pause 只对进行中，Resume 只对已暂停；
 * Delete 对任何已知状态成立。本地任务恒为 true。
 */
export function remoteCan(view: DownloadTaskView, action: RemoteCommandParams['action']): boolean {
  if (view.source === 'local') return true
  switch (view.remoteStatus) {
    case null:
    case 'unknown':
      return false
    case 'pending':
    case 'accepted':
    case 'downloading':
      return action === 'pause' || action === 'cancel' || action === 'delete'
    case 'paused':
      return action === 'resume' || action === 'cancel' || action === 'delete'
    default:
      return action === 'delete'
  }
}

function remoteCommand(view: DownloadTaskView, action: RemoteCommandParams['action'], deleteFiles = false) {
  return () =>
    rpc.agent.remote.command(
      action === 'delete' ? { taskId: view.taskId, action, deleteFiles } : { taskId: view.taskId, action },
    )
}

export function pauseViews(views: readonly DownloadTaskView[]): Promise<boolean> {
  return guardedAll(
    views
      .filter((view) => remoteCan(view, 'pause'))
      .map((view) =>
        view.source === 'local' ? () => rpc.daemon.task.pause({ taskId: view.taskId }) : remoteCommand(view, 'pause'),
      ),
  )
}

/** 继续（失败任务同为 resume，UI 上叫重试）。 */
export function resumeViews(views: readonly DownloadTaskView[]): Promise<boolean> {
  return guardedAll(
    views
      .filter((view) => remoteCan(view, 'resume'))
      .map((view) =>
        view.source === 'local' ? () => rpc.daemon.task.resume({ taskId: view.taskId }) : remoteCommand(view, 'resume'),
      ),
  )
}

export const pauseAll = () => guarded(() => rpc.daemon.task.pauseAll())
export const resumeAll = () => guarded(() => rpc.daemon.task.resumeAll())

export function deleteViews(views: readonly DownloadTaskView[], deleteFiles: boolean): Promise<boolean> {
  return guardedAll(
    views
      .filter((view) => remoteCan(view, 'delete'))
      .map((view) =>
        view.source === 'local'
          ? () => rpc.daemon.task.delete({ taskId: view.taskId, deleteFiles })
          : remoteCommand(view, 'delete', deleteFiles),
      ),
  )
}

/** 「删除任务及文件」二次确认；确认后执行。 */
export async function confirmDeleteWithFiles(views: readonly DownloadTaskView[]): Promise<boolean> {
  if (views.length === 0) return false
  const [only] = views
  const description =
    views.length === 1 && only
      ? t('deleteConfirmDescWithFile', { fileName: only.name })
      : t('batchDeleteConfirmDescWithFile', { count: views.length })
  const ok = await confirmDialog({
    title: t('deleteTaskAndFile'),
    description,
    okLabel: t('delete'),
    intent: 'destructive',
  })
  return ok ? deleteViews(views, true) : false
}

export function moveViewsToQueue(views: readonly DownloadTaskView[], queueId: string): Promise<boolean> {
  return guardedAll(
    views
      .filter((view) => view.source === 'local')
      .map((view) => () => rpc.daemon.queue.moveTask({ taskId: view.taskId, queueId })),
  )
}

/** Boost：同 id 再次调用即取消。 */
export const toggleBoost = (taskId: string) => guarded(() => rpc.daemon.queue.boost({ id: taskId }))

export function isPluginRetryError(view: DownloadTaskView): boolean {
  return view.state === 'failed' && view.errorMessage.startsWith(PLUGIN_ERROR_PREFIX)
}

export async function confirmIgnorePluginRetry(taskId: string): Promise<boolean> {
  const ok = await confirmDialog({
    title: t('taskIgnorePluginRetryTitle'),
    description: t('taskIgnorePluginRetryMsg'),
    okLabel: t('taskIgnorePluginRetry'),
  })
  return ok ? guarded(() => rpc.daemon.plugin.ignoreRetry({ taskId })) : false
}

/** 可重新下载：本地已完成 / 失败且非 `torrent-file://` 哨兵。 */
export function canRedownload(view: DownloadTaskView): boolean {
  return (
    view.source === 'local' &&
    (view.state === 'completed' || view.state === 'failed') &&
    !view.url.startsWith('torrent-file://')
  )
}

/** 删除（含文件）后按原参数重建。 */
export function redownloadViews(views: readonly DownloadTaskView[]): Promise<boolean> {
  return guardedAll(
    views.filter(canRedownload).map((view) => async () => {
      const dto = view.dto
      if (!dto) return
      const request: CreateTaskRequest = {
        url: dto.originUrl === '' ? dto.url : dto.originUrl,
        fileName: dto.fileName,
        saveDir: dto.saveDir,
        segments: 0,
        cookies: '',
        referrer: dto.referrer,
        proxyUrl: dto.proxyUrl,
        userAgent: '',
        queueId: dto.queueId,
        checksum: dto.checksum,
        ignoreTlsErrors: dto.ignoreTlsErrors,
        startPaused: false,
      }
      await rpc.daemon.task.delete({ taskId: dto.taskId, deleteFiles: true })
      await rpc.daemon.task.create({ request })
    }),
  )
}

export function copyUrls(views: readonly DownloadTaskView[]): void {
  const urls = views.map(shareUrl).filter((url) => url !== '')
  if (urls.length === 0) return
  copyText(urls.join('\n'))
  toast.key('urlCopied', 'success')
}

/** 只有本地已完成且文件仍在磁盘上的任务有可下载的最终文件。 */
export const isDownloadable = (view: DownloadTaskView): boolean =>
  view.source === 'local' && view.state === 'completed' && !view.fileMissing

/** 浏览器下载已完成文件（替代 GPUI 的「打开文件」）；多文件间隔触发以免被浏览器合并拦截。 */
export function downloadViewsFiles(views: readonly DownloadTaskView[]): void {
  views.filter(isDownloadable).forEach((view, index) => {
    setTimeout(() => downloadTaskFile(view.taskId), index * 400)
  })
}

// ── 任务组 ──

export const pauseGroup = (groupId: string) => guarded(() => rpc.daemon.group.pause({ groupId }))
export const resumeGroup = (groupId: string) => guarded(() => rpc.daemon.group.resume({ groupId }))

/** 组内失败成员逐个恢复。 */
export function retryFailedInGroup(views: readonly DownloadTaskView[], groupId: string): Promise<boolean> {
  return resumeViews(views.filter((view) => view.source === 'local' && view.groupId === groupId && view.state === 'failed'))
}

export const deleteGroup = (groupId: string, deleteFiles: boolean) =>
  guarded(() => rpc.daemon.group.delete({ groupId, deleteFiles }))

export async function confirmDeleteGroupWithFiles(groupId: string, groupName: string): Promise<boolean> {
  const ok = await confirmDialog({
    title: t('groupDeleteWithFiles'),
    description: t('deleteConfirmDescWithFile', { fileName: groupName }),
    okLabel: t('delete'),
    intent: 'destructive',
  })
  return ok ? deleteGroup(groupId, true) : false
}

/** 复制任务组来源链接。 */
export function copySourceLink(url: string): void {
  if (url === '') return
  copyText(url)
  toast.key('urlCopied', 'success')
}
