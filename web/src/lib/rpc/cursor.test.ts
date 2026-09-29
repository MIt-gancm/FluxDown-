import { describe, expect, test } from 'bun:test'
import { judgeFrame } from './cursor'

const cursor = { epoch: 'a', sequence: 5 }

describe('event cursor', () => {
  test('连续帧接受', () => {
    expect(judgeFrame(cursor, { epoch: 'a', sequence: 6 })).toBe('accept')
  })
  test('快照之前的旧帧/重复帧跳过', () => {
    expect(judgeFrame(cursor, { epoch: 'a', sequence: 5 })).toBe('skip')
    expect(judgeFrame(cursor, { epoch: 'a', sequence: 1 })).toBe('skip')
  })
  test('断档与 epoch 变更要求重新 snapshot', () => {
    expect(judgeFrame(cursor, { epoch: 'a', sequence: 7 })).toBe('resync')
    expect(judgeFrame(cursor, { epoch: 'b', sequence: 6 })).toBe('resync')
  })
})
