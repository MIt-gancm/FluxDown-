import { describe, expect, test } from 'bun:test'
import { uploadLimitText, uploadLimitToSend } from './seedUpload'

describe('seed upload limit', () => {
  test('现值按 KB/s 预填，未设置显示为空', () => {
    expect(uploadLimitText(512 * 1024)).toBe('512')
    expect(uploadLimitText(0)).toBe('')
    expect(uploadLimitText(undefined)).toBe('')
  })

  test('未改动时保持原字节数，不因取整丢精度', () => {
    const original = 1_500_000
    const initial = uploadLimitText(original)
    expect(uploadLimitToSend(initial, initial, original)).toBe(original)
  })

  test('改动后按 KB/s 换算；清空或非法回到跟随全局', () => {
    expect(uploadLimitToSend('256', '512', 512 * 1024)).toBe(256 * 1024)
    expect(uploadLimitToSend('', '512', 512 * 1024)).toBe(0)
    expect(uploadLimitToSend('abc', '512', 512 * 1024)).toBe(0)
  })

  test('原本未设置且未填写时发送 0', () => {
    expect(uploadLimitToSend('', '', undefined)).toBe(0)
  })
})
