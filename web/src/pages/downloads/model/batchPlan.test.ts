import { describe, expect, test } from 'bun:test'
import { planTaskCommand } from './batchPlan'
import type { DownloadTaskView } from './task'

function local(taskId: string): DownloadTaskView {
  return { source: 'local', taskId, remoteStatus: null } as unknown as DownloadTaskView
}

function remote(taskId: string, remoteStatus: string | null): DownloadTaskView {
  return { source: 'remote', taskId, remoteStatus } as unknown as DownloadTaskView
}

describe('planTaskCommand', () => {
  test('多个本地任务合并为一次批量调用，并按选择顺序去重', () => {
    const plan = planTaskCommand([local('a'), local('b'), local('a'), local('c')], 'delete')
    expect(plan.batch).toBe(true)
    expect(plan.localIds).toEqual(['a', 'b', 'c'])
    expect(plan.remote).toEqual([])
  })

  test('单个本地任务走单任务方法', () => {
    const plan = planTaskCommand([local('a'), local('a')], 'pause')
    expect(plan.batch).toBe(false)
    expect(plan.localIds).toEqual(['a'])
  })

  test('远程任务按状态过滤后逐个发送，不进入批量', () => {
    const paused = remote('r1', 'paused')
    const running = remote('r2', 'downloading')
    const unknown = remote('r3', 'unknown')
    const plan = planTaskCommand([local('a'), paused, running, unknown], 'resume')
    expect(plan.localIds).toEqual(['a'])
    expect(plan.batch).toBe(false)
    expect(plan.remote).toEqual([paused])
  })

  test('空选择无任何调用', () => {
    const plan = planTaskCommand([], 'delete')
    expect(plan.localIds).toEqual([])
    expect(plan.batch).toBe(false)
    expect(plan.remote).toEqual([])
  })
})
