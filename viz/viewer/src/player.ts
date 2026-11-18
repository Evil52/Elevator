import {
  carIds,
  type Car,
  type Call,
  type Dispatch,
  type Motion,
  type Status,
  type Timeline,
} from './timeline'

export function floorLabel(positionMm: number) {
  const level = Math.floor(positionMm / 3000) + 1
  const label = (n: number) => String(n).padStart(2, '0')
  return positionMm % 3000 === 0 ? label(level) : label(level) + '–' + label(level + 1)
}
function round(n: bigint, d: bigint) {
  const sign = n < 0n ? -1n : 1n
  return Number(sign * ((2n * n * sign + d) / (2n * d)))
}
function sample(m: Motion, t: number) {
  if (t >= m.t1Ms) return { positionMm: m.toMm, speed: 0 }
  let elapsed = Math.max(0, t - m.t0Ms)
  for (const p of m.profile) {
    if (elapsed <= p.dtMs) {
      const dt = BigInt(p.dtMs),
        e = BigInt(elapsed)
      const a = BigInt(Math.abs(p.v0MmPerSec)),
        b = BigInt(Math.abs(p.v1MmPerSec))
      const area = 2n * a * dt * e + (b - a) * e * e
      return {
        positionMm: m.fromMm + p.xMm + round(BigInt(p.dxMm) * area, (a + b) * dt * dt),
        speed: p.v0MmPerSec + round(BigInt(p.v1MmPerSec - p.v0MmPerSec) * e, dt),
      }
    }
    elapsed -= p.dtMs
  }
  return { positionMm: m.toMm, speed: 0 }
}
function motionAt(line: Timeline, id: Car, t: number) {
  const m = line.motions.filter((m) => m.car === id && m.t0Ms <= t).at(-1)
  return m && t < line.durationMs
    ? sample(m, Math.floor(t))
    : { positionMm: line.initial[id], speed: 0 }
}
export function positionAt(line: Timeline, id: Car, t: number) {
  return motionAt(line, id, t).positionMm
}
export function lastEventAt(line: Timeline, t: number) {
  let index = 0
  while (index + 1 < line.events.length && line.events[index + 1].tMs <= t) index++
  return index
}
export function stepEvent(line: Timeline, current: number, delta: number) {
  return Math.max(0, Math.min(line.events.length - 1, current + delta))
}

type Mode = 'BRAKING' | 'LOCKED' | 'MOVING' | 'OPEN' | 'OPENING' | 'CLOSING' | 'READY'
export type Cabin = {
  positionMm: number
  speed: number
  mode: Mode
  doorFloor: number
  doorPermille: number
}
export type Snapshot = {
  cars: Record<Car, Cabin>
  calls: Record<string, Call & { state: Status; car?: Car }>
  alarm: Car | null
  dispatch?: Dispatch
  index: number
  reset: boolean
}

export function stateAt(
  line: Timeline,
  time: number,
  eventIndex = lastEventAt(line, time),
): Snapshot {
  const t = Math.max(0, Math.min(line.durationMs, Math.floor(time)))
  const state: Snapshot = {
    cars: {} as Record<Car, Cabin>,
    calls: Object.create(null),
    alarm: null,
    index: eventIndex,
    reset: false,
  }
  let locked: Car | undefined
  for (const e of line.events.slice(0, eventIndex + 1)) {
    if (e.tMs > t) break
    if (e.type === 'DISPATCH') state.dispatch = e as typeof e & Dispatch
    if (e.type === 'ALARM') state.alarm = e.car!
    if (e.type === 'LOCKED') locked = e.car
    if (e.type === 'RESET')
      Object.assign(state, {
        alarm: null,
        calls: Object.create(null),
        dispatch: undefined,
        reset: true,
      })
    if ((e.type === 'CALL' || e.type === 'REQUEUE') && e.call && e.state) {
      const registered = line.calls.find((c) => c.id === e.call)!
      state.calls[e.call] = { ...registered, state: e.state, car: e.car }
    }
  }
  const doorTime = state.reset ? 0 : t
  for (const id of carIds) {
    const { positionMm, speed } = motionAt(line, id, t)
    const d = line.doors.filter((d) => d.car === id && d.t0Ms <= doorTime).at(-1)
    let amount = d?.toPermille ?? 0
    let mode: Mode = 'READY'
    if (d && d.t1Ms > doorTime) {
      amount =
        d.fromPermille +
        round(
          BigInt(d.toPermille - d.fromPermille) * BigInt(doorTime - d.t0Ms),
          BigInt(d.t1Ms - d.t0Ms),
        )
      if (d.fromPermille !== d.toPermille)
        mode = d.fromPermille < d.toPermille ? 'OPENING' : 'CLOSING'
    }
    if (amount === 1000 && mode === 'READY') mode = 'OPEN'
    if (speed) mode = 'MOVING'
    if (state.alarm === id) mode = locked === id ? 'LOCKED' : 'BRAKING'
    state.cars[id] = { positionMm, speed, mode, doorPermille: amount, doorFloor: d?.floor ?? 1 }
  }
  return state
}
