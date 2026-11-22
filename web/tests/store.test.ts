import { readFileSync, readdirSync } from 'node:fs'
import { describe, expect, it } from 'vitest'
import type { Alarm, CarSnapshot, Envelope, Event } from '@elevator/protocol-v1'
import { EVENT_LIMIT } from '../src/transport/types.ts'
import { initialState, linkClosed, linkOpened, markStale, receive } from '../src/transport/store.ts'

function envelope(message: Envelope['message'], boot = 1, run = 1, seq = 0): string {
  return JSON.stringify({ version: 1, bootId: boot, runId: run, seq, message })
}

function snapshot(car: number, extra: Partial<CarSnapshot> = {}): CarSnapshot {
  return {
    car,
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

function alarm(id: number, car: number | null = 0): Alarm {
  return { id, kind: 'carFault', car, state: 'active', raisedAtMs: id }
}

function page(offset: number, total: number, alarms: Alarm[]): Envelope['message'] {
  const items: (Alarm | null)[] = [...alarms]
  while (items.length < 8) items.push(null)
  return { alarmPage: { total, offset, items: items as never } }
}

function chunk(firstIndex: number, count: number, droppedBefore = 0): Envelope['message'] {
  const events: (Event | null)[] = []
  for (let i = 0; i < 8; i += 1) {
    events.push(
      i < count
        ? { index: firstIndex + i, atMs: i, kind: 'assigned', car: 0, id: i }
        : null,
    )
  }
  return { eventLogChunk: { firstIndex, droppedBefore, events: events as never } }
}

function open(): ReturnType<typeof initialState> {
  return linkOpened(initialState())
}

describe('начальное состояние', () => {
  it('пустое, данные считаются устаревшими, связи ещё нет', () => {
    const state = initialState()
    expect(state.link).toBe('connecting')
    expect(state.stale).toBe(true)
    expect(state.fresh).toBe(false)
    expect(state.identity).toBeNull()
    expect(state.cars).toEqual({})
  })
})

describe('снимки кабин', () => {
  it('снимок попадает в состояние, данные становятся свежими, запоминается запуск', () => {
    const next = receive(open(), envelope({ carSnapshot: snapshot(2) }, 7, 3))
    expect(next.cars[2]).toEqual(snapshot(2))
    expect(next.fresh).toBe(true)
    expect(next.stale).toBe(false)
    expect(next.identity).toEqual({ bootId: 7, runId: 3 })
  })

  it('новый снимок той же кабины заменяет старый, другие кабины остаются', () => {
    let state = open()
    state = receive(state, envelope({ carSnapshot: snapshot(0, { positionMm: 100 }) }))
    state = receive(state, envelope({ carSnapshot: snapshot(1) }))
    state = receive(state, envelope({ carSnapshot: snapshot(0, { positionMm: 200 }) }))
    expect(state.cars[0]?.positionMm).toBe(200)
    expect(state.cars[1]).toBeDefined()
  })

  it('исходное состояние не изменяется', () => {
    const before = open()
    const copy = structuredClone(before)
    receive(before, envelope({ carSnapshot: snapshot(0) }))
    expect(before).toEqual(copy)
  })
})

describe('приветствие', () => {
  it('сохраняется вместе с причиной сброса', () => {
    const next = receive(
      open(),
      envelope({ hello: { deviceId: 9, resetReason: 'watchdog' } }, 5, 1),
    )
    expect(next.hello).toEqual({ deviceId: 9, resetReason: 'watchdog' })
    expect(next.identity).toEqual({ bootId: 5, runId: 1 })
    expect(next.fresh).toBe(false)
  })
})

describe('некорректные сообщения', () => {
  it('не портят последнее валидное состояние и считаются', () => {
    let state = receive(open(), envelope({ carSnapshot: snapshot(0) }))
    const good = structuredClone(state)
    for (const raw of ['{broken', 'null', '42', '[]', '{"version":1}', envelope({ carSnapshot: snapshot(300) })]) {
      state = receive(state, raw)
    }
    expect(state.invalidMessages).toBe(6)
    expect({ ...state, invalidMessages: 0 }).toEqual({ ...good, invalidMessages: 0 })
  })

  it('несовместимая версия показывается отдельно и не считается ошибкой данных', () => {
    const state = receive(open(), '{"version":2,"bootId":1,"runId":1,"seq":0,"message":{}}')
    expect(state.incompatibleVersion).toBe(2)
    expect(state.invalidMessages).toBe(0)
    expect(state.link).toBe('open')
  })

  it('версия ниже первой тоже несовместима', () => {
    const state = receive(open(), '{"version":0,"bootId":1,"runId":1,"seq":0,"message":{}}')
    expect(state.incompatibleVersion).toBe(0)
  })

  it('корректное сообщение версии 1 снимает отметку о несовместимости', () => {
    let state = receive(open(), '{"version":2}')
    state = receive(state, envelope({ carSnapshot: snapshot(0) }))
    expect(state.incompatibleVersion).toBeNull()
  })
})

describe('смена запуска', () => {
  it('новый bootId стирает всё, включая приветствие', () => {
    let state = receive(open(), envelope({ hello: { deviceId: 1, resetReason: 'powerOn' } }, 1, 1))
    state = receive(state, envelope({ carSnapshot: snapshot(0) }, 1, 1))
    state = receive(state, envelope({ carSnapshot: snapshot(1) }, 2, 1))
    expect(Object.keys(state.cars)).toEqual(['1'])
    expect(state.hello).toBeNull()
    expect(state.identity).toEqual({ bootId: 2, runId: 1 })
  })

  it('новый runId стирает кабины, тревоги и журнал, но оставляет приветствие', () => {
    let state = receive(open(), envelope({ hello: { deviceId: 1, resetReason: 'powerOn' } }, 1, 1))
    state = receive(state, envelope({ carSnapshot: snapshot(0) }, 1, 1))
    state = receive(state, envelope(page(0, 1, [alarm(1)]), 1, 1))
    state = receive(state, envelope(chunk(0, 3), 1, 1))
    state = receive(state, envelope({ carSnapshot: snapshot(2) }, 1, 2))
    expect(Object.keys(state.cars)).toEqual(['2'])
    expect(state.alarms).toEqual([])
    expect(state.events).toEqual([])
    expect(state.hello).not.toBeNull()
  })

  it('результаты команд прошлого запуска не переживают смену запуска', () => {
    let state = receive(open(), envelope({ commandResult: { requestId: 4, status: 'applied' } }, 1, 1))
    state = receive(state, envelope({ carSnapshot: snapshot(0) }, 1, 2))
    expect(state.results).toEqual({})
  })
})

describe('тревоги', () => {
  it('страницы собираются по смещению в один список по порядку', () => {
    let state = open()
    state = receive(state, envelope(page(8, 10, [alarm(9), alarm(10)])))
    state = receive(state, envelope(page(0, 10, [1, 2, 3, 4, 5, 6, 7, 8].map((id) => alarm(id)))))
    expect(state.alarmTotal).toBe(10)
    expect(state.alarms.map((a) => a.id)).toEqual([1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
  })

  it('когда тревог стало мало, старая вторая страница пропадает', () => {
    let state = open()
    state = receive(state, envelope(page(0, 10, [1, 2, 3, 4, 5, 6, 7, 8].map((id) => alarm(id)))))
    state = receive(state, envelope(page(8, 10, [alarm(9), alarm(10)])))
    state = receive(state, envelope(page(0, 3, [alarm(1), alarm(2), alarm(3)])))
    expect(state.alarmTotal).toBe(3)
    expect(state.alarms.map((a) => a.id)).toEqual([1, 2, 3])
  })

  it('пустая страница очищает список', () => {
    let state = receive(open(), envelope(page(0, 1, [alarm(1)])))
    state = receive(state, envelope(page(0, 0, [])))
    expect(state.alarms).toEqual([])
    expect(state.alarmTotal).toBe(0)
  })

  it('новая версия тревоги заменяет прежнюю по номеру', () => {
    let state = receive(open(), envelope(page(0, 1, [alarm(1)])))
    state = receive(state, envelope(page(0, 1, [{ ...alarm(1), state: 'acknowledged' }])))
    expect(state.alarms).toHaveLength(1)
    expect(state.alarms[0]?.state).toBe('acknowledged')
  })
})

describe('журнал событий', () => {
  it('порции складываются по индексу без повторов', () => {
    let state = open()
    state = receive(state, envelope(chunk(0, 8)))
    state = receive(state, envelope(chunk(4, 8)))
    expect(state.events.map((e) => e.index)).toEqual([...Array(12).keys()])
  })

  it('запоминается, сколько событий потеряно до порции', () => {
    const state = receive(open(), envelope(chunk(5, 3, 5)))
    expect(state.eventsDroppedBefore).toBe(5)
    expect(state.events.map((e) => e.index)).toEqual([5, 6, 7])
  })

  it(`хранится не больше ${EVENT_LIMIT} последних событий`, () => {
    let state = open()
    for (let first = 0; first < 400; first += 8) {
      state = receive(state, envelope(chunk(first, 8)))
    }
    expect(state.events).toHaveLength(EVENT_LIMIT)
    expect(state.events.at(-1)?.index).toBe(399)
    expect(state.events[0]?.index).toBe(400 - EVENT_LIMIT)
  })
})

describe('результаты команд', () => {
  it('запоминаются по номеру запроса, в том числе отказ с причиной', () => {
    let state = receive(open(), envelope({ commandResult: { requestId: 1, status: 'applied' } }))
    state = receive(state, envelope({ commandResult: { requestId: 2, status: { rejected: 'noSuchFloor' } } }))
    expect(state.results).toEqual({ 1: 'applied', 2: { rejected: 'noSuchFloor' } })
  })
})

describe('связь', () => {
  it('обрыв оставляет последний кадр, но помечает его устаревшим', () => {
    let state = receive(open(), envelope({ carSnapshot: snapshot(0, { positionMm: 5 }) }))
    state = linkClosed(state)
    expect(state.link).toBe('closed')
    expect(state.stale).toBe(true)
    expect(state.fresh).toBe(false)
    expect(state.cars[0]?.positionMm).toBe(5)
  })

  it('после повторного открытия данные остаются устаревшими до первого снимка', () => {
    let state = linkClosed(receive(open(), envelope({ carSnapshot: snapshot(0) })))
    state = linkOpened(state)
    expect(state.link).toBe('open')
    expect(state.stale).toBe(true)
    expect(state.fresh).toBe(false)
    state = receive(state, envelope({ carSnapshot: snapshot(0) }))
    expect(state.stale).toBe(false)
    expect(state.fresh).toBe(true)
  })

  it('новое открытие связи поверх свежего состояния снова ждёт первого снимка', () => {
    let state = receive(open(), envelope({ carSnapshot: snapshot(0) }))
    expect(state.fresh).toBe(true)
    state = linkOpened(state)
    expect(state.fresh).toBe(false)
    expect(state.stale).toBe(true)
  })

  it('молчание источника помечает данные устаревшими, не закрывая связь', () => {
    let state = receive(open(), envelope({ carSnapshot: snapshot(0) }))
    state = markStale(state)
    expect(state.stale).toBe(true)
    expect(state.fresh).toBe(false)
    expect(state.link).toBe('open')
  })

  it('открытие связи не стирает счётчик ошибок и отметку версии', () => {
    let state = receive(open(), 'garbage')
    state = receive(state, '{"version":3}')
    state = linkOpened(linkClosed(state))
    expect(state.invalidMessages).toBe(1)
    expect(state.incompatibleVersion).toBe(3)
  })
})

describe('фикстуры контракта', () => {
  const root = (dir: string) => new URL(`../../protocol/v1/fixtures/${dir}/`, import.meta.url)

  for (const name of readdirSync(root('valid')).sort()) {
    it(`valid/${name} принимается`, () => {
      const raw = readFileSync(new URL(name, root('valid')), 'utf8')
      const state = receive(open(), raw)
      expect(state.invalidMessages).toBe(0)
      expect(state.incompatibleVersion).toBeNull()
      expect(state.identity).not.toBeNull()
    })
  }

  for (const name of readdirSync(root('invalid')).sort()) {
    it(`invalid/${name} отвергается`, () => {
      const raw = readFileSync(new URL(name, root('invalid')), 'utf8')
      const before = open()
      const state = receive(before, raw)
      const flagged = state.invalidMessages === 1 || state.incompatibleVersion !== null
      expect(flagged).toBe(true)
      expect(state.identity).toBeNull()
      expect(state.cars).toEqual({})
    })
  }
})
