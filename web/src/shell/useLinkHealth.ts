import { useAgent, useConnection } from '../lib/rpc/hooks'

/** 当前呈现的连接状态。 */
export type LinkHealth = 'ok' | 'connecting' | 'reconnecting' | 'daemonOffline' | 'stopped' | 'incompatible'

const noDaemonConnected = false

export function useLinkHealth(): LinkHealth {
  const connection = useConnection()
  const daemonConnected = useAgent((snapshot) => snapshot.daemonConnected, noDaemonConnected)
  switch (connection.phase) {
    case 'connecting':
    case 'syncing':
      return 'connecting'
    case 'reconnecting':
      return 'reconnecting'
    case 'stopped':
      return 'stopped'
    case 'incompatible':
      return 'incompatible'
    case 'ready':
      return daemonConnected ? 'ok' : 'daemonOffline'
    default:
      return 'ok'
  }
}
