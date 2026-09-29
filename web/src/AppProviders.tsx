// 全局提供者与连接控制器。
//   ConnectionController 是 /rpc 连接的唯一所有者：登录态 → 启动/停止连接；
//   连接阶段 unauthorized → 清凭证回登录页，setupRequired → 首次运行向导。

import { useLocation, useNavigate } from '@tanstack/react-router'
import { useEffect } from 'react'
import type { ReactNode } from 'react'
import { I18nProvider, t } from './i18n'
import { clearToken, getToken, useIsAuthenticated } from './lib/access'
import { startConnection, stopConnection } from './lib/rpc/client'
import { useConnection } from './lib/rpc/hooks'
import { ThemeProvider } from './theme'
import { ConfirmHost, ToastHost, TooltipProvider, toast } from './ui'

function ConnectionController() {
  const authenticated = useIsAuthenticated()
  const connection = useConnection()
  const navigate = useNavigate()
  const { pathname } = useLocation()
  // 登录 / 向导页自己处理连接结果，控制器不介入（避免重复提示与竞态）。
  const onAuthPage = pathname === '/login' || pathname === '/setup'

  useEffect(() => {
    if (authenticated) startConnection(getToken())
    else stopConnection()
  }, [authenticated])

  useEffect(() => {
    if (!authenticated || onAuthPage) return
    if (connection.phase === 'unauthorized') {
      clearToken()
      toast.warning(t('webAuthExpired'))
      void navigate({ to: '/login', replace: true })
    } else if (connection.phase === 'setupRequired') {
      clearToken()
      void navigate({ to: '/setup', replace: true })
    }
  }, [authenticated, onAuthPage, connection.phase, navigate])

  return null
}

/** 必须位于 RouterProvider 内部（ConnectionController 用到 navigate）。 */
export function AppProviders({ children }: { children: ReactNode }) {
  return (
    <I18nProvider>
      <ThemeProvider>
        <TooltipProvider>
          <ConnectionController />
          {children}
          <ConfirmHost />
          <ToastHost />
        </TooltipProvider>
      </ThemeProvider>
    </I18nProvider>
  )
}
