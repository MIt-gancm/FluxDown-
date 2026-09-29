// 多设备协同的纯逻辑：其他设备、远程任务可见性与按目标设备计数（契约 §5）。

import type { AgentSessionDto, CloudDevice, RemoteTaskDto } from '../../../lib/rpc'

/** 本机在 FluxCloud 的 deviceId：优先会话，其次设备列表里的 `isCurrent`。 */
export function currentDeviceId(
  session: Pick<AgentSessionDto, 'device'> | null,
  cloudDevices: readonly CloudDevice[],
): string | null {
  return session?.device.deviceId ?? cloudDevices.find((device) => device.isCurrent)?.deviceId ?? null
}

/** 「其他设备」：账号设备去掉本机。 */
export function otherDevices(cloudDevices: readonly CloudDevice[]): CloudDevice[] {
  return cloudDevices.filter((device) => !device.isCurrent)
}

/** 目标为本机的远程任务在本地已有真实任务，不再作为镜像行显示。 */
export function visibleRemoteTasks(tasks: readonly RemoteTaskDto[], currentId: string | null): RemoteTaskDto[] {
  return currentId === null ? tasks.slice() : tasks.filter((task) => task.toDevice !== currentId)
}

/** 远程任务按目标设备计数。 */
export function countByTargetDevice(tasks: readonly Pick<RemoteTaskDto, 'toDevice'>[]): Map<string, number> {
  const counts = new Map<string, number>()
  for (const task of tasks) counts.set(task.toDevice, (counts.get(task.toDevice) ?? 0) + 1)
  return counts
}
