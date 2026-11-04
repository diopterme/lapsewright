// node --test src/lib/crop.test.ts
import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cropBox, isCropped } from './crop.ts'

const near = (a: number, b: number) => assert.ok(Math.abs(a - b) < 1e-9, `${a} != ${b}`)

test('3:2 photo in 16:9 video loses top and bottom', () => {
  const box = cropBox(6000, 4000, 3840, 2160)
  near(box.width, 1)
  near(box.height, (3 / 2) / (16 / 9))
  near(box.top, (1 - box.height) / 2)
  assert.ok(isCropped(box))
})

test('portrait photo in 16:9 video loses top and bottom more', () => {
  const box = cropBox(3000, 4000, 1920, 1080)
  near(box.height, (3 / 4) / (16 / 9))
})

test('wider photo than video loses the sides', () => {
  const box = cropBox(6000, 2000, 1920, 1080)
  near(box.height, 1)
  near(box.width, (16 / 9) / 3)
  near(box.left, (1 - box.width) / 2)
})

test('same aspect is not cropped', () => {
  assert.equal(isCropped(cropBox(3840, 2160, 1920, 1080)), false)
})
