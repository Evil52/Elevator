export const carIds = ['A', 'B', 'C'] as const
export type Car = (typeof carIds)[number]
export type Status = 'QUEUED' | 'ASSIGNED' | 'ARRIVED' | 'SERVED'
export type Call = { id: string; floor: number; direction: 'UP' | 'DOWN'; registeredMs: number }
export type Pair = { car: Car; column: number; etaMs: number }
export type Result = { pairs: Pair[]; totalEtaMs: number }
export type Dispatch = {
  allowed: number[][]
  rows: Car[]
  columns: string[]
  matrix: (number | null)[][]
  optimal: Result
  greedy: Result
}
export type ModelEvent = {
  tMs: number
  type: string
  car?: Car
  floor?: number
  call?: string
  state?: Status
  registeredMs?: number
}
export type Phase = {
  dtMs: number
  xMm: number
  dxMm: number
  v0MmPerSec: number
  v1MmPerSec: number
}
export type Motion = {
  car: Car
  t0Ms: number
  t1Ms: number
  fromMm: number
  toMm: number
  profile: Phase[]
}
type Door = {
  car: Car
  floor: number
  t0Ms: number
  t1Ms: number
  fromPermille: number
  toPermille: number
}
export type Timeline = {
  schemaVersion: 1
  durationMs: number
  initial: Record<Car, number>
  calls: Call[]
  events: ModelEvent[]
  motions: Motion[]
  doors: Door[]
}

function check(ok: unknown, field: string): asserts ok {
  if (!ok) throw new Error('Некорректная шкала событий · ' + field)
}
function record(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v)
}
function integer(v: unknown): v is number {
  return Number.isSafeInteger(v)
}
function car(v: unknown): v is Car {
  return carIds.some((id) => id === v)
}
function floor(v: unknown) {
  return integer(v) && v >= 1 && v <= 10
}
function span(v: Record<string, unknown>, max: number) {
  return integer(v.t0Ms) && integer(v.t1Ms) && v.t0Ms >= 0 && v.t1Ms >= v.t0Ms && v.t1Ms <= max
}

export function parseTimeline(data: unknown): Timeline {
  check(
    record(data) && data.schemaVersion === 1 && integer(data.durationMs) && data.durationMs > 0,
    'версия или длительность',
  )
  const { initial, calls, events, motions, doors, durationMs } = data
  check(record(initial) && carIds.every((id) => integer(initial[id])), 'позиции')
  check(
    Array.isArray(calls) &&
      calls.every(
        (c) =>
          record(c) &&
          typeof c.id === 'string' &&
          floor(c.floor) &&
          integer(c.registeredMs) &&
          ['UP', 'DOWN'].includes(String(c.direction)),
      ),
    'вызовы',
  )
  const ids = new Set(calls.map((c) => c.id))
  check(
    Array.isArray(motions) &&
      motions.every(
        (m) =>
          record(m) &&
          car(m.car) &&
          span(m, durationMs) &&
          integer(m.fromMm) &&
          integer(m.toMm) &&
          Array.isArray(m.profile) &&
          m.profile.every(
            (p) =>
              record(p) &&
              ['dtMs', 'xMm', 'dxMm', 'v0MmPerSec', 'v1MmPerSec'].every((k) => integer(p[k])) &&
              Number(p.dtMs) > 0 &&
              Math.abs(Number(p.v0MmPerSec)) + Math.abs(Number(p.v1MmPerSec)) > 0,
          ),
      ),
    'движение',
  )
  check(
    Array.isArray(doors) &&
      doors.every(
        (d) =>
          record(d) &&
          car(d.car) &&
          floor(d.floor) &&
          span(d, durationMs) &&
          [d.fromPermille, d.toPermille].every((v) => integer(v) && v >= 0 && v <= 1000),
      ),
    'двери',
  )
  check(Array.isArray(events) && events.length && events[0].tMs === 0, 'начало')
  let previous = 0
  for (const e of events) {
    check(
      record(e) &&
        integer(e.tMs) &&
        e.tMs >= previous &&
        e.tMs <= durationMs &&
        typeof e.type === 'string',
      'порядок событий',
    )
    previous = e.tMs
    if (e.car !== undefined) check(car(e.car), 'кабина')
    if (e.type === 'CALL' || e.type === 'REQUEUE')
      check(
        ids.has(e.call) &&
          ['QUEUED', 'ASSIGNED', 'ARRIVED', 'SERVED'].includes(String(e.state)) &&
          integer(e.registeredMs),
        'переход вызова',
      )
    if (e.type === 'DISPATCH') {
      const { rows, columns, matrix, allowed, optimal, greedy } = e
      check(
        Array.isArray(rows) &&
          rows.length === 3 &&
          rows.every((id, i) => id === carIds[i]) &&
          Array.isArray(columns) &&
          columns.every((id) => ids.has(id)) &&
          Array.isArray(matrix) &&
          matrix.length === rows.length &&
          matrix.every(
            (row) =>
              Array.isArray(row) &&
              row.length === columns.length &&
              row.every((v) => v === null || (integer(v) && v >= 0)),
          ),
        'матрица ETA',
      )
      check(
        Array.isArray(allowed) &&
          allowed.length === rows.length &&
          allowed.every(
            (row) =>
              Array.isArray(row) &&
              row.length === columns.length &&
              row.every((x) => x === 0 || x === 1),
          ),
        'допустимость пар',
      )
      for (const plan of [optimal, greedy])
        check(
          record(plan) &&
            integer(plan.totalEtaMs) &&
            Array.isArray(plan.pairs) &&
            plan.pairs.every(
              (p) =>
                record(p) &&
                car(p.car) &&
                integer(p.column) &&
                integer(p.etaMs) &&
                allowed[rows.indexOf(p.car)][p.column] === 1 &&
                matrix[rows.indexOf(p.car)][p.column] === p.etaMs,
            ),
          'назначения',
        )
    }
  }
  check(previous === durationMs && events.at(-1).type === 'RESET', 'конец')
  return data as Timeline
}
