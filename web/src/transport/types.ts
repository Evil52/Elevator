import type { Alarm, CarSnapshot, CommandStatus, Event, Hello } from '@elevator/protocol-v1'

export type LinkStatus = 'connecting' | 'open' | 'closed'

export interface Identity {
  bootId: number
  runId: number
}

export interface PanelState {
  link: LinkStatus
  stale: boolean
  fresh: boolean
  incompatibleVersion: number | null
  invalidMessages: number
  identity: Identity | null
  hello: Hello | null
  cars: Record<number, CarSnapshot>
  alarmPages: Record<number, Alarm[]>
  alarmTotal: number
  alarms: Alarm[]
  events: Event[]
  eventsDroppedBefore: number
  results: Record<number, CommandStatus>
}

export const EVENT_LIMIT = 256

export type BlockReason = 'closed' | 'stale' | 'incompatible' | 'invalid'

export type CommandOutcome =
  | { kind: 'result'; requestId: number; status: CommandStatus }
  | { kind: 'sent'; requestId: number }
  | { kind: 'blocked'; requestId: number; reason: BlockReason }

export type SimActionName = 'pause' | 'resume' | 'reset' | 'fireRecall' | 'clearFire'

export interface SimTransport {
  getState(): PanelState
  subscribe(listener: () => void): () => void
  connect(): void
  close(): void
  callFloor(floor: number, direction: 'up' | 'down', requestId?: number): Promise<CommandOutcome>
  ackAlarm(alarmId: number, requestId?: number): Promise<CommandOutcome>
  injectFault(car: number, requestId?: number): Promise<CommandOutcome>
  recoverCar(car: number, requestId?: number): Promise<CommandOutcome>
  simControl(action: SimActionName, requestId?: number): Promise<CommandOutcome>
  readLog(fromIndex: number, requestId?: number): Promise<CommandOutcome>
}
