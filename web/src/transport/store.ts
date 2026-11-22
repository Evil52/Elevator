import { isEnvelope } from '@elevator/protocol-v1'
import type { Alarm, Envelope, Event } from '@elevator/protocol-v1'
import { EVENT_LIMIT } from './types.ts'
import type { PanelState } from './types.ts'

export function initialState(): PanelState {
  return {
    link: 'connecting',
    stale: true,
    fresh: false,
    incompatibleVersion: null,
    invalidMessages: 0,
    identity: null,
    hello: null,
    cars: {},
    alarmPages: {},
    alarmTotal: 0,
    alarms: [],
    events: [],
    eventsDroppedBefore: 0,
    results: {},
  }
}

function invalid(state: PanelState): PanelState {
  return { ...state, invalidMessages: state.invalidMessages + 1 }
}

export function receive(state: PanelState, raw: string): PanelState {
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    return invalid(state)
  }
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return invalid(state)
  const version = (value as { version?: unknown }).version
  if (typeof version === 'number' && version !== 1) {
    return { ...state, incompatibleVersion: version }
  }
  if (!isEnvelope(value)) return invalid(state)
  return apply({ ...state, incompatibleVersion: null }, value)
}

function startOver(state: PanelState): PanelState {
  return {
    ...initialState(),
    link: state.link,
    invalidMessages: state.invalidMessages,
    incompatibleVersion: state.incompatibleVersion,
  }
}

function forgetRun(state: PanelState): PanelState {
  return {
    ...state,
    fresh: false,
    stale: true,
    cars: {},
    alarmPages: {},
    alarmTotal: 0,
    alarms: [],
    events: [],
    eventsDroppedBefore: 0,
    results: {},
  }
}

function assembleAlarms(pages: Record<number, Alarm[]>): Alarm[] {
  return Object.entries(pages)
    .sort(([a], [b]) => Number(a) - Number(b))
    .flatMap(([, items]) => items)
}

function mergeEvents(known: Event[], incoming: (Event | null)[]): Event[] {
  const byIndex = new Map(known.map((event) => [event.index, event]))
  for (const event of incoming) {
    if (event !== null) byIndex.set(event.index, event)
  }
  return [...byIndex.values()].sort((a, b) => a.index - b.index).slice(-EVENT_LIMIT)
}

function apply(state: PanelState, envelope: Envelope): PanelState {
  let next = state
  const { bootId, runId } = envelope
  if (next.identity !== null && next.identity.bootId !== bootId) {
    next = startOver(next)
  } else if (next.identity !== null && next.identity.runId !== runId) {
    next = forgetRun(next)
  }
  next = { ...next, identity: { bootId, runId } }
  const message = envelope.message
  if ('hello' in message) return { ...next, hello: message.hello }
  if ('carSnapshot' in message) {
    const snapshot = message.carSnapshot
    return { ...next, cars: { ...next.cars, [snapshot.car]: snapshot }, fresh: true, stale: false }
  }
  if ('alarmPage' in message) {
    const { offset, total, items } = message.alarmPage
    const pages: Record<number, Alarm[]> = {}
    for (const [key, page] of Object.entries(next.alarmPages)) {
      if (Number(key) < total) pages[Number(key)] = page
    }
    if (offset < total) pages[offset] = items.filter((item): item is Alarm => item !== null)
    return { ...next, alarmPages: pages, alarmTotal: total, alarms: assembleAlarms(pages) }
  }
  if ('eventLogChunk' in message) {
    const chunk = message.eventLogChunk
    return {
      ...next,
      events: mergeEvents(next.events, chunk.events),
      eventsDroppedBefore: chunk.droppedBefore,
    }
  }
  if ('commandResult' in message) {
    const { requestId, status } = message.commandResult
    return { ...next, results: { ...next.results, [requestId]: status } }
  }
  return next
}

export function linkOpened(state: PanelState): PanelState {
  return { ...state, link: 'open', fresh: false, stale: true }
}

export function linkClosed(state: PanelState): PanelState {
  return { ...state, link: 'closed', fresh: false, stale: true }
}

export function markStale(state: PanelState): PanelState {
  return { ...state, fresh: false, stale: true }
}
