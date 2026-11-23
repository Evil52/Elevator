import type { Alarm, CarSnapshot } from '@elevator/protocol-v1'
import { initialState } from '../../src/transport/store.ts'
import type {
  CommandOutcome,
  PanelState,
  SimActionName,
  SimTransport,
} from '../../src/transport/types.ts'

export class FakeTransport implements SimTransport {
  state: PanelState
  calls: string[] = []
  outcome: CommandOutcome | Promise<CommandOutcome> = { kind: 'result', requestId: 1, status: 'accepted' }
  private readonly listeners = new Set<() => void>()

  constructor(state: PanelState) {
    this.state = state
  }

  set(state: PanelState): void {
    this.state = state
    for (const listener of [...this.listeners]) listener()
  }

  getState(): PanelState {
    return this.state
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  connect(): void {}

  close(): void {}

  private record(name: string): Promise<CommandOutcome> {
    this.calls.push(name)
    return Promise.resolve(this.outcome)
  }

  callFloor(floor: number, direction: 'up' | 'down'): Promise<CommandOutcome> {
    return this.record(`callFloor:${floor}:${direction}`)
  }

  ackAlarm(alarmId: number): Promise<CommandOutcome> {
    return this.record(`ackAlarm:${alarmId}`)
  }

  injectFault(car: number): Promise<CommandOutcome> {
    return this.record(`injectFault:${car}`)
  }

  recoverCar(car: number): Promise<CommandOutcome> {
    return this.record(`recoverCar:${car}`)
  }

  simControl(action: SimActionName): Promise<CommandOutcome> {
    return this.record(`simControl:${action}`)
  }

  readLog(fromIndex: number): Promise<CommandOutcome> {
    return this.record(`readLog:${fromIndex}`)
  }
}

export function car(index: number, extra: Partial<CarSnapshot> = {}): CarSnapshot {
  return {
    car: index,
    mode: 'normal',
    doors: 'closed',
    floor: 1,
    positionMm: 0,
    speedMmPerSec: 0,
    etaMs: null,
    upCalls: 0,
    downCalls: 0,
    modelTimeMs: 1,
    ...extra,
  }
}

export function alarm(id: number, extra: Partial<Alarm> = {}): Alarm {
  return { id, kind: 'carFault', car: 0, state: 'active', raisedAtMs: 1000, ...extra }
}

export function freshState(extra: Partial<PanelState> = {}): PanelState {
  return {
    ...initialState(),
    link: 'open',
    fresh: true,
    stale: false,
    identity: { bootId: 1, runId: 1 },
    cars: { 0: car(0), 1: car(1, { positionMm: 12000, floor: 5 }), 2: car(2, { positionMm: 27000, floor: 10 }) },
    ...extra,
  }
}

export function staleState(extra: Partial<PanelState> = {}): PanelState {
  return freshState({ fresh: false, stale: true, ...extra })
}
