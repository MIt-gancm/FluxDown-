import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { RouterProvider } from '@tanstack/react-router'
import './index.css'
import { AppProviders } from './AppProviders'
import { router } from './router'
import { applyInitialTheme } from './theme'

// 首屏先用本地缓存的外观偏好上色，避免等待 /rpc 快照期间闪烁。
applyInitialTheme()

const rootElement = document.getElementById('root')
if (!rootElement) throw new Error('missing #root')

createRoot(rootElement).render(
  <StrictMode>
    <RouterProvider router={router} InnerWrap={AppProviders} />
  </StrictMode>,
)
