import { readFileSync, readdirSync } from 'node:fs'
import { expect, it } from 'vitest'
import { isEnvelope } from '../src/index'

for (const [dir, expected] of [
  ['valid', true],
  ['invalid', false],
] as const) {
  const root = new URL(`../../fixtures/${dir}/`, import.meta.url)
  for (const name of readdirSync(root).sort()) {
    it(`${dir}/${name}`, () => {
      const value: unknown = JSON.parse(readFileSync(new URL(name, root), 'utf8'))
      expect(isEnvelope(value)).toBe(expected)
    })
  }
}

it('принимает целое число в экспоненциальной записи', () => {
  const value: unknown = JSON.parse(
    '{"version":1,"bootId":1,"runId":2,"seq":0,"message":{"callFloor":{"requestId":7,"floor":5e0,"direction":"down"}}}',
  )
  expect(isEnvelope(value)).toBe(true)
})
