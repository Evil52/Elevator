import { expect, it } from 'vitest'
import recording from '../../recordings/single-trip/timeline.json'
import { parseTimeline } from './timeline'
import { positionAt, stateAt } from './player'

it('воспроизводит все отсчёты поездки Rust и сохраняет конечный этаж', () => {
  const line = parseTimeline(recording)
  expect(line.source).toBe('rust-sil')
  const m = line.motions[0]
  let t = m.t0Ms
  for (const phase of m.profile) {
    t += phase.dtMs
    const car = stateAt(line, t).cars.A
    expect(car.positionMm).toBe(m.fromMm + phase.xMm + phase.dxMm)
    expect(car.speed).toBe(phase.v1MmPerSec)
  }
  expect(positionAt(line, 'A', 1250)).toBe(766)
  expect(positionAt(line, 'A', 4250)).toBe(8234)
  const eta = stateAt(line, 0).dispatch!.optimal.totalEtaMs
  expect(eta).toBe(5500)
  expect(stateAt(line, eta - 1).calls.H04.state).toBe('ASSIGNED')
  expect(stateAt(line, eta).calls.H04.state).toBe('ARRIVED')
  expect(stateAt(line, eta).cars.A).toMatchObject({ positionMm: 9000, speed: 0, doorPermille: 0 })
  const end = stateAt(line, line.durationMs)
  expect(end.calls.H04.state).toBe('SERVED')
  expect(end.cars.A).toMatchObject({ positionMm: 9000, speed: 0, doorPermille: 1000 })
  expect(end.reset).toBe(false)
})
