import { Eye, EyeOff } from 'lucide-react'
import { useState } from 'react'
import { useT } from '../../../../i18n'
import { cn } from '../../../../lib/cn'
import { Button, Icon, Input } from '../../../../ui'
import type { InputProps } from '../../../../ui'

/** 密码输入：右侧显示/隐藏切换（GPUI `mask_toggle`）。 */
export function PasswordInput(props: Omit<InputProps, 'type' | 'trailing'>) {
  const t = useT()
  const [visible, setVisible] = useState(false)
  return (
    <Input
      {...props}
      className={cn('coarse:pr-12', props.className)}
      type={visible ? 'text' : 'password'}
      trailing={
        <Button
          variant="ghost"
          iconOnly
          tabIndex={-1}
          className="text-muted-foreground hover:text-foreground"
          onClick={() => setVisible((value) => !value)}
          aria-label={visible ? t('webHidePassword') : t('webShowPassword')}
        >
          <Icon icon={visible ? EyeOff : Eye} size="md" />
        </Button>
      }
    />
  )
}
