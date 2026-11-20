import { describe, it, expect } from 'vitest'
import raw from '../../out/timeline.json'
import golden from '../../golden.json'
import { parseTimeline } from './timeline'
import { stateAt, positionAt, lastEventAt, stepEvent, floorLabel } from './player'

const line = parseTimeline(raw)

describe('проигрыватель', () => {
  it('читает источник и сохраняет совместимость с первой версией демо', () => {
    expect(line.source).toBe('python-demo')
    expect(parseTimeline({ ...raw, source: 'rust-sil' }).source).toBe('rust-sil')
    expect(parseTimeline({ ...raw, source: undefined }).source).toBeUndefined()
    expect(() => parseTimeline({ ...raw, source: 42 })).toThrow()
  })
  it('применяет переходы вызовов только для событий CALL и REQUEUE', () => {
    const extra = { tMs: 0, type: 'NOTE', call: 'H08', state: 'комментарий' }
    const timeline = parseTimeline({
      ...raw,
      events: [raw.events[0], extra, ...raw.events.slice(1)],
    })
    expect(stateAt(timeline, 0, 1).calls).toEqual({})
  })
  it('совпадает с эталонными целыми миллиметрами Python', () => {
    for (const { car, tMs, positionMm } of golden)
      expect(positionAt(line, car as 'A' | 'B' | 'C', tMs)).toBe(positionMm)
  })
  it('различает точный этаж и остановку между этажами', () => {
    expect(floorLabel(9000)).toBe('04')
    expect(floorLabel(16500)).toBe('06–07')
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
    const at = line.events.findIndex((e) => e.type === 'REQUEUE')
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
    expect(() => parseTimeline({ ...raw, durationMs: 5.3 })).toThrow()
    expect(() => parseTimeline({ ...raw, events: [...raw.events].reverse() })).toThrow()
    expect(() =>
      parseTimeline({ ...raw, motions: [{ ...raw.motions[0], profile: null }] }),
    ).toThrow()
  })
})

it('проверяет непрерывность, длительность и путь профиля', () => {
  for (const fault of ['empty', 'duration', 'offset', 'distance']) {
    const changed = structuredClone(raw)
    const m = changed.motions[0]
    if (fault === 'empty') m.profile = []
    if (fault === 'duration') m.profile[0].dtMs++
    if (fault === 'offset') m.profile[1].xMm++
    if (fault === 'distance') m.toMm++
    expect(() => parseTimeline(changed)).toThrow()
  }
})

it('проверяет сумму и уникальность назначений в обоих планах', () => {
  for (const name of ['optimal', 'greedy'] as const) {
    for (const fault of ['total', 'car', 'column']) {
      const changed = structuredClone(raw)
      const dispatch = changed.events.find((e) => e.type === 'DISPATCH')!
      const plan = dispatch[name]!
      const pair = { ...plan.pairs[0] }
      if (fault === 'total') plan.totalEtaMs++
      else {
        if (fault === 'column') pair.car = plan.pairs[1].car
        else pair.column = plan.pairs[1].column
        pair.etaMs = dispatch.matrix![dispatch.rows!.indexOf(pair.car)][pair.column]!
        plan.pairs = [plan.pairs[0], pair]
        plan.totalEtaMs = plan.pairs.reduce((sum, p) => sum + p.etaMs, 0)
      }
      expect(() => parseTimeline(changed)).toThrow()
    }
  }
})
