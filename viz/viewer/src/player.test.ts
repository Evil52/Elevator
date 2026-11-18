import { describe, it, expect } from 'vitest'
import raw from '../../out/timeline.json'
import golden from '../../golden.json'
import { parseTimeline } from './timeline'
import { stateAt, positionAt, lastEventAt, stepEvent } from './player'

const line = parseTimeline(raw)

describe('проигрыватель', () => {
  it('совпадает с эталонными целыми миллиметрами Python', () => {
    for (const { car, tMs, positionMm } of golden)
      expect(positionAt(line, car as 'A'|'B'|'C', tMs)).toBe(positionMm)
  })
  it('показывает торможение, блокировку и полный сброс', () => {
    expect(stateAt(line, 3100).cars.A.mode).toBe('BRAKING')
    expect(stateAt(line, 4500).cars.A.mode).toBe('LOCKED')
    const end = stateAt(line, line.durationMs)
    expect(end.alarm).toBe(null)
    expect(end.cars.A.positionMm).toBe(line.initial.A)
    expect(end.cars.C.doorPermille).toBe(1000)
    expect(Object.keys(end.calls)).toHaveLength(0)
  })
  it('различает одновременные события и сохраняет время регистрации', () => {
    const at = line.events.findIndex(e => e.type === 'REQUEUE')
    expect(stateAt(line, 3000, at).calls.H08.state).toBe('QUEUED')
    expect(stateAt(line, 3000).calls.H08.car).toBe('C')
    expect(stateAt(line, 3000).calls.H08.registeredMs).toBe(0)
    expect(stepEvent(line, at - 1, 1)).toBe(at)
    expect(lastEventAt(line, 3000)).toBeGreaterThan(at)
    expect(stepEvent(line, 0, -1)).toBe(0)
  })
  it('двигает двери шахты только на этаже остановки', () => {
    const opening = stateAt(line, 5200)
    expect(opening.calls.H03.state).toBe('ARRIVED')
    expect(opening.cars.B.doorPermille).toBe(500)
    expect(opening.cars.B.doorFloor).toBe(3)
    expect(stateAt(line, 6400).calls.H03.state).toBe('SERVED')
  })
  it('при перемотке возвращает исходную матрицу', () => {
    expect(stateAt(line, 3100).dispatch?.columns).toEqual(['H08'])
    expect(stateAt(line, 0).dispatch?.optimal.totalEtaMs).toBe(10000)
  })
  it('отклоняет несовместимый формат, дробные величины и неверный порядок', () => {
    expect(() => parseTimeline({})).toThrow()
    expect(() => parseTimeline({...raw, durationMs: 5.3})).toThrow()
    expect(() => parseTimeline({...raw, events: [...raw.events].reverse()})).toThrow()
    expect(() => parseTimeline({...raw, motions: [{ ...raw.motions[0], profile: null }]})).toThrow()
  })
})

