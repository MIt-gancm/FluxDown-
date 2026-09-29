import { describe, expect, test } from 'bun:test'
import type { CloudDevice, RemoteTaskDto } from '../../../lib/rpc'
import { countByTargetDevice, currentDeviceId, otherDevices, visibleRemoteTasks } from './devices'

const task = (id: string, from: string, to: string) => ({ id, fromDevice: from, toDevice: to }) as RemoteTaskDto
const device = (deviceId: string, isCurrent: boolean) => ({ id: `row-${deviceId}`, deviceId, isCurrent }) as CloudDevice

describe('设备与远程任务', () => {
  test('其他设备过滤当前设备', () => {
    expect(otherDevices([device('a', true), device('b', false)]).map((d) => d.deviceId)).toEqual(['b'])
  })

  test('本机 deviceId：会话优先，其次 isCurrent 行', () => {
    expect(currentDeviceId(null, [device('a', false), device('b', true)])).toBe('b')
    expect(currentDeviceId(null, [device('a', false)])).toBeNull()
    expect(currentDeviceId({ device: device('s', true) }, [device('b', true)])).toBe('s')
  })

  test('目标为本机的任务不作镜像行；按 toDevice 而非 fromDevice 计数', () => {
    const tasks = [task('1', 'me', 'pc'), task('2', 'me', 'pc'), task('3', 'pc', 'me'), task('4', 'me', 'phone')]
    const visible = visibleRemoteTasks(tasks, 'me')
    expect(visible.map((t) => t.id)).toEqual(['1', '2', '4'])
    expect(countByTargetDevice(visible).get('pc')).toBe(2)
    expect(countByTargetDevice(visible).get('me')).toBeUndefined()
    // 本机未知时不猜测，全部保留
    expect(visibleRemoteTasks(tasks, null).length).toBe(4)
  })
})
