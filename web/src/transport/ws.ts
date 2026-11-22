import { isEnvelope } from '@elevator/protocol-v1'
import type { Envelope, Message } from '@elevator/protocol-v1'
import { initialState, linkClosed, linkOpened, markStale, receive } from './store.ts'
import type {
  BlockReason,
  CommandOutcome,
  PanelState,
  SimActionName,
  SimTransport,
} from './types.ts'

export interface SocketLike {
  onopen: ((event: unknown) => void) | null
  onmessage: ((event: { data: unknown }) => void) | null
  onclose: ((event: unknown) => void) | null
  onerror: ((event: unknown) => void) | null
  send(data: string): void
  close(): void
}

export interface WsTransportOptions {
  createSocket?: (url: string) => SocketLike
  staleAfterMs?: number
  requestTimeoutMs?: number
  reconnectBaseMs?: number
  reconnectMaxMs?: number
}

interface Pending {
  promise: Promise<CommandOutcome>
  resolve: (outcome: CommandOutcome) => void
  timer: ReturnType<typeof setTimeout>
}

const DEFAULT_STALE_MS = 3000
const DEFAULT_REQUEST_MS = 3000
const DEFAULT_RECONNECT_BASE_MS = 500
const DEFAULT_RECONNECT_MAX_MS = 5000

function browserSocket(url: string): SocketLike {
  const Socket = (globalThis as unknown as { WebSocket: new (url: string) => SocketLike }).WebSocket
  return new Socket(url)
}

export class WsTransport implements SimTransport {
  private state: PanelState = initialState()
  private socket: SocketLike | null = null
  private wanted = false
  private attempt = 0
  private seq = 0
  private nextId = 1
  private staleTimer: ReturnType<typeof setTimeout> | null = null
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private readonly listeners = new Set<() => void>()
  private readonly pending = new Map<number, Pending>()

  constructor(
    readonly url: string,
    readonly options: WsTransportOptions = {},
  ) {}

  getState(): PanelState {
    return this.state
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => {
      this.listeners.delete(listener)
    }
  }

  connect(): void {
    this.wanted = true
    if (this.socket !== null) return
    this.clearReconnect()
    this.openSocket()
  }

  close(): void {
    this.wanted = false
    this.clearReconnect()
    this.clearStale()
    const socket = this.socket
    this.socket = null
    socket?.close()
    this.resolveAllPending('unknown')
    this.update(linkClosed(this.state))
  }

  callFloor(floor: number, direction: 'up' | 'down', requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, true, (id) => ({ callFloor: { requestId: id, floor, direction } }))
  }

  ackAlarm(alarmId: number, requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, true, (id) => ({ ackAlarm: { requestId: id, alarmId } }))
  }

  injectFault(car: number, requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, true, (id) => ({ injectFault: { requestId: id, car } }))
  }

  recoverCar(car: number, requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, true, (id) => ({ recoverCar: { requestId: id, car } }))
  }

  simControl(action: SimActionName, requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, true, (id) => ({ simControl: { requestId: id, action } }))
  }

  readLog(fromIndex: number, requestId?: number): Promise<CommandOutcome> {
    return this.submit(requestId, false, (id) => ({ readLog: { requestId: id, fromIndex } }))
  }

  private update(next: PanelState): void {
    if (next === this.state) return
    this.state = next
    this.settlePending()
    for (const listener of [...this.listeners]) listener()
  }

  private openSocket(): void {
    const socket = (this.options.createSocket ?? browserSocket)(this.url)
    this.socket = socket
    this.update({ ...this.state, link: 'connecting' })
    socket.onopen = () => {
      if (this.socket !== socket) return
      this.attempt = 0
      this.update(linkOpened(this.state))
    }
    socket.onmessage = (event) => {
      if (this.socket !== socket) return
      this.onMessage(event.data)
    }
    socket.onclose = () => {
      if (this.socket !== socket) return
      this.onSocketClosed()
    }
    socket.onerror = () => {}
  }

  private onMessage(data: unknown): void {
    const before = this.state
    const next =
      typeof data === 'string'
        ? receive(before, data)
        : { ...before, invalidMessages: before.invalidMessages + 1 }
    this.update(next)
    if (next.cars !== before.cars) this.restartStaleTimer()
  }

  private onSocketClosed(): void {
    this.socket = null
    this.clearStale()
    this.resolveAllPending('unknown')
    this.update(linkClosed(this.state))
    const base = this.options.reconnectBaseMs ?? DEFAULT_RECONNECT_BASE_MS
    const max = this.options.reconnectMaxMs ?? DEFAULT_RECONNECT_MAX_MS
    const delay = Math.min(base * 2 ** this.attempt, max)
    this.attempt += 1
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      if (this.wanted && this.socket === null) this.openSocket()
    }, delay)
  }

  private restartStaleTimer(): void {
    this.clearStale()
    this.staleTimer = setTimeout(() => {
      this.staleTimer = null
      this.update(markStale(this.state))
    }, this.options.staleAfterMs ?? DEFAULT_STALE_MS)
  }

  private clearStale(): void {
    if (this.staleTimer !== null) clearTimeout(this.staleTimer)
    this.staleTimer = null
  }

  private clearReconnect(): void {
    if (this.reconnectTimer !== null) clearTimeout(this.reconnectTimer)
    this.reconnectTimer = null
  }

  private blockReason(): BlockReason | null {
    if (this.state.link !== 'open' || this.socket === null) return 'closed'
    if (this.state.incompatibleVersion !== null) return 'incompatible'
    if (!this.state.fresh || this.state.identity === null) return 'stale'
    return null
  }

  private peekId(): number {
    let id = this.nextId
    while (this.pending.has(id)) id += 1
    return id
  }

  private submit(
    requestId: number | undefined,
    awaitResult: boolean,
    build: (requestId: number) => Message,
  ): Promise<CommandOutcome> {
    const id = requestId ?? this.peekId()
    const existing = this.pending.get(id)
    if (existing) return existing.promise
    const reason = this.blockReason()
    if (reason !== null) return Promise.resolve({ kind: 'blocked', requestId: id, reason })
    const identity = this.state.identity
    if (identity === null || this.socket === null) {
      return Promise.resolve({ kind: 'blocked', requestId: id, reason: 'stale' })
    }
    const envelope: Envelope = {
      version: 1,
      bootId: identity.bootId,
      runId: identity.runId,
      seq: this.seq + 1,
      message: build(id),
    }
    if (!isEnvelope(envelope)) {
      return Promise.resolve({ kind: 'blocked', requestId: id, reason: 'invalid' })
    }
    this.seq += 1
    if (requestId === undefined) this.nextId = id + 1
    const { [id]: _stale, ...results } = this.state.results
    this.update({ ...this.state, results })
    this.socket.send(JSON.stringify(envelope))
    if (!awaitResult) return Promise.resolve({ kind: 'sent', requestId: id })
    return this.waitFor(id)
  }

  private waitFor(id: number): Promise<CommandOutcome> {
    let resolve: (outcome: CommandOutcome) => void = () => {}
    const promise = new Promise<CommandOutcome>((done) => {
      resolve = done
    })
    const timer = setTimeout(() => {
      this.pending.delete(id)
      resolve({ kind: 'result', requestId: id, status: 'expired' })
    }, this.options.requestTimeoutMs ?? DEFAULT_REQUEST_MS)
    this.pending.set(id, { promise, resolve, timer })
    return promise
  }

  private settlePending(): void {
    for (const [id, entry] of this.pending) {
      const status = this.state.results[id]
      if (status === undefined) continue
      clearTimeout(entry.timer)
      this.pending.delete(id)
      entry.resolve({ kind: 'result', requestId: id, status })
    }
  }

  private resolveAllPending(status: 'unknown'): void {
    for (const [id, entry] of this.pending) {
      clearTimeout(entry.timer)
      this.pending.delete(id)
      entry.resolve({ kind: 'result', requestId: id, status })
    }
  }
}
