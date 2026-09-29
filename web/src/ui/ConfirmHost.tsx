import { useSyncExternalStore } from 'react'
import { t } from '../i18n'
import { readConfirm, settleConfirm, subscribeConfirm } from './confirm'
import { ConfirmFooter, Dialog } from './Dialog'

export function ConfirmHost() {
  const current = useSyncExternalStore(subscribeConfirm, readConfirm, readConfirm)
  return (
    <Dialog
      open={current !== null}
      onOpenChange={(open) => !open && settleConfirm(false)}
      title={current?.title ?? ''}
      description={current?.description}
      size="sm"
      footer={
        <ConfirmFooter
          cancelLabel={current?.cancelLabel ?? t('cancel')}
          okLabel={current?.okLabel ?? t('confirm')}
          intent={current?.intent}
          onCancel={() => settleConfirm(false)}
          onOk={() => settleConfirm(true)}
        />
      }
    />
  )
}
