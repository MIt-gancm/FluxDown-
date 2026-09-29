// 服务端目录选择：浏览 daemon 所在机器的文件系统（`daemon.fs.list`，仅子目录）。

import { ArrowUp, Folder } from 'lucide-react'
import { useEffect, useState } from 'react'
import { useT } from '../../../../i18n'
import { rpc } from '../../../../lib/rpc'
import type { FsListResponse } from '../../../../lib/rpc'
import { ConfirmFooter, Dialog, Icon, Input, Spinner } from '../../../../ui'
import { rpcErrorText } from '../../kit'

export function DirPickerDialog({
  open,
  onOpenChange,
  initialPath,
  onPick,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  initialPath: string
  onPick: (path: string) => void
}) {
  const t = useT()
  const [target, setTarget] = useState(initialPath)
  const [listing, setListing] = useState<FsListResponse | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [pathDraft, setPathDraft] = useState(initialPath)

  useEffect(() => {
    if (listing) setPathDraft(listing.path)
  }, [listing])

  useEffect(() => {
    if (open) setTarget(initialPath)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅在打开时以当前值为起点
  }, [open])

  useEffect(() => {
    if (!open) return
    let cancelled = false
    setError(null)
    rpc.daemon.fs
      .list(target === '' ? undefined : { path: target })
      .then((response) => {
        if (!cancelled) setListing(response)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setListing(null)
        setError(rpcErrorText(err, t))
      })
    return () => {
      cancelled = true
    }
  }, [open, target, t])

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t('webFsPickTitle')}
      size="md"
      footer={
        <ConfirmFooter
          cancelLabel={t('cancel')}
          okLabel={t('confirm')}
          okDisabled={listing === null}
          onCancel={() => onOpenChange(false)}
          onOk={() => {
            if (!listing) return
            onPick(listing.path)
            onOpenChange(false)
          }}
        />
      }
    >
      <div className="flex min-h-[240px] flex-col gap-1">
        <Input
          value={pathDraft}
          aria-label={t("webFsPickTitle")}
          autoComplete="off"
          spellCheck={false}
          className="mb-1 w-full"
          onChange={(event) => setPathDraft(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") setTarget(pathDraft.trim())
          }}
        />
        {listing?.parent != null ? (
          <button type="button" className="flex min-h-control w-full items-center gap-2 rounded-md px-2 text-left text-sm text-foreground hover:bg-row-hover coarse:min-h-touch" onClick={() => setTarget(listing.parent ?? '')}>
            <Icon icon={ArrowUp} className="text-muted-foreground" />
            <span className="min-w-0 truncate">{t('webFsParent')}</span>
          </button>
        ) : null}
        {error ? <p className="px-2 py-3 text-sm text-destructive">{error}</p> : null}
        {!error && listing === null ? (
          <div className="flex justify-center py-6">
            <Spinner />
          </div>
        ) : null}
        {listing?.denied ? <p className="px-2 py-3 text-sm text-warning">{t('webFsDenied')}</p> : null}
        {listing && !listing.denied && listing.dirs.length === 0 ? <p className="px-2 py-3 text-sm text-muted-foreground">{t('webFsEmpty')}</p> : null}
        {listing?.dirs.map((dir) => (
          <button key={dir.path} type="button" className="flex min-h-control w-full items-center gap-2 rounded-md px-2 text-left text-sm text-foreground hover:bg-row-hover coarse:min-h-touch" onClick={() => setTarget(dir.path)}>
            <Icon icon={Folder} className="shrink-0 text-muted-foreground" />
            <span className="min-w-0 truncate">{dir.name}</span>
          </button>
        ))}
      </div>
    </Dialog>
  )
}
