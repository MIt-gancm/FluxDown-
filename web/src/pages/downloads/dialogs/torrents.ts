// `.torrent` 文件提交：浏览器文件 → `/api/web/blobs/torrents` 上传 → `daemon.task.create { torrentBlobId }`。

import { describeUploadError, rpc, uploadBlob } from '../../../lib/rpc'
import { toast } from '../../../ui'

export interface TorrentSubmitOptions {
  /** 空 = 全局默认保存目录。 */
  saveDir?: string
  /** 空 = 默认队列。 */
  queueId?: string
  startPaused?: boolean
}

export function isTorrentFile(file: File): boolean {
  return file.name.toLowerCase().endsWith('.torrent')
}

/** 逐个上传并建任务；单个失败弹错误提示并继续，返回成功数。BT 文件选择由 daemon 的选择请求驱动。 */
export async function submitTorrentFiles(files: readonly File[], options: TorrentSubmitOptions = {}): Promise<number> {
  let created = 0
  for (const file of files) {
    if (!isTorrentFile(file)) continue
    try {
      const blobId = await uploadBlob('torrents', file)
      await rpc.daemon.task.create({
        request: {
          url: '',
          saveDir: options.saveDir ?? '',
          queueId: options.queueId ?? '',
          startPaused: options.startPaused ?? false,
        },
        torrentBlobId: blobId,
      })
      created += 1
    } catch (error) {
      toast.error(describeUploadError(error), file.name)
    }
  }
  return created
}
