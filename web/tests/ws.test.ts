import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { CarSnapshot } from '@elevator/protocol-v1'
import { WsTransport } from '../src/transport/ws.ts'
import type { SocketLike } from '../src/transport/ws.ts'

class FakeSocket implements SocketLike {
  static all: FakeSocket[] = []
  onopen: ((event: unknown) => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: ((event: unknown) => void) | null = null
  onerror: ((event: unknown) => void) | null = null
  sent: string[] = []
  closed = false

  constructor(readonly url: string) {
    FakeSocket.all.push(this)
  }

  send(data: string): void {
    this.sent.push(data)
  }

  close(): void {
    this.closed = true
    this.onclose?.({})
  }

  serverOpen(): void {
    this.onopen?.({})
  }

  serverSends(data: unknown): void {
    this.onmessage?.({ data })
  }

  serverDrops(): void {
    this.onclose?.({})
  }
}

function snapshot(car = 0, positionMm = 0): CarSnapshot {
  return {
    car,
    mode: 'normal',
    doors: 'closed',
    floor: 1,
    positionMm,
    speedMmPerSec: 0,
    etaMs: null,
    upCalls: 0,
    downCalls: 0,
    modelTimeMs: 1,
  }
}

function frame(message: object, boot = 10, run = 2, seq = 0): string {
  return JSON.stringify({ version: 1, bootId: boot, runId: run, seq, message })
}

function transport(options = {}): WsTransport {
  return new WsTransport('ws://gateway.test/ws', {
    createSocket: (url) => new FakeSocket(url),
    ...options,
  })
}

function live(options = {}): { ws: WsTransport; socket: FakeSocket } {
  const ws = transport(options)
  ws.connect()
  const socket = FakeSocket.all.at(-1)
  if (!socket) throw new Error('no socket')
  socket.serverOpen()
  socket.serverSends(frame({ carSnapshot: snapshot() }))
  return { ws, socket }
}

beforeEach(() => {
  FakeSocket.all = []
  vi.useFakeTimers()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('подключение', () => {
  it('открывает один сокет по адресу, пока идёт подключение связь connecting', () => {
    const ws = transport()
    ws.connect()
    expect(FakeSocket.all).toHaveLength(1)
    expect(FakeSocket.all[0]?.url).toBe('ws://gateway.test/ws')
    expect(ws.getState().link).toBe('connecting')
  })

  it('повторный connect при живом сокете ничего не создаёт', () => {
    const ws = transport()
    ws.connect()
    ws.connect()
    expect(FakeSocket.all).toHaveLength(1)
  })

  it('после открытия связь open, но данные ещё устарели', () => {
    const ws = transport()
    ws.connect()
    FakeSocket.all[0]?.serverOpen()
    expect(ws.getState().link).toBe('open')
    expect(ws.getState().stale).toBe(true)
  })

  it('снимок делает данные свежими и уведомляет подписчиков', () => {
    const ws = transport()
    const listener = vi.fn()
    ws.subscribe(listener)
    ws.connect()
    FakeSocket.all[0]?.serverOpen()
    listener.mockClear()
    FakeSocket.all[0]?.serverSends(frame({ carSnapshot: snapshot(1, 500) }))
    expect(listener).toHaveBeenCalledTimes(1)
    expect(ws.getState().cars[1]?.positionMm).toBe(500)
    expect(ws.getState().fresh).toBe(true)
  })

  it('после отписки уведомления прекращаются', () => {
    const ws = transport()
    const listener = vi.fn()
    const off = ws.subscribe(listener)
    off()
    ws.connect()
    FakeSocket.all[0]?.serverOpen()
    expect(listener).not.toHaveBeenCalled()
  })

  it('не текстовые и испорченные сообщения считаются и не портят состояние', () => {
    const { ws, socket } = live()
    socket.serverSends(new Uint8Array([1, 2, 3]))
    socket.serverSends('{broken')
    expect(ws.getState().invalidMessages).toBe(2)
    expect(ws.getState().cars[0]).toBeDefined()
    expect(ws.getState().fresh).toBe(true)
  })

  it('несовместимая версия видна отдельно, связь остаётся открытой', () => {
    const { ws, socket } = live()
    socket.serverSends('{"version":2,"bootId":1,"runId":1,"seq":0,"message":{}}')
    expect(ws.getState().incompatibleVersion).toBe(2)
    expect(ws.getState().link).toBe('open')
  })
})

describe('обрыв и переподключение', () => {
  it('обрыв оставляет последний кадр и помечает его устаревшим', () => {
    const { ws, socket } = live()
    socket.serverDrops()
    expect(ws.getState().link).toBe('closed')
    expect(ws.getState().stale).toBe(true)
    expect(ws.getState().cars[0]).toBeDefined()
  })

  it('переподключение с удвоением паузы и потолком, успешное открытие сбрасывает паузу', () => {
    const ws = transport({ reconnectBaseMs: 500, reconnectMaxMs: 2000 })
    ws.connect()
    const waits: number[] = []
    for (let attempt = 0; attempt < 5; attempt += 1) {
      const before = FakeSocket.all.length
      FakeSocket.all.at(-1)?.serverDrops()
      let waited = 0
      while (FakeSocket.all.length === before && waited < 10_000) {
        vi.advanceTimersByTime(100)
        waited += 100
      }
      waits.push(waited)
    }
    expect(waits).toEqual([500, 1000, 2000, 2000, 2000])
    FakeSocket.all.at(-1)?.serverOpen()
    const before = FakeSocket.all.length
    FakeSocket.all.at(-1)?.serverDrops()
    vi.advanceTimersByTime(500)
    expect(FakeSocket.all).toHaveLength(before + 1)
  })

  it('запоздавшие сообщения старого сокета после переподключения игнорируются', () => {
    const { ws, socket } = live({ reconnectBaseMs: 100 })
    socket.serverDrops()
    vi.advanceTimersByTime(100)
    const next = FakeSocket.all.at(-1)
    next?.serverOpen()
    socket.serverSends(frame({ carSnapshot: snapshot(0, 12345) }))
    socket.serverOpen()
    expect(ws.getState().cars[0]?.positionMm).toBe(0)
    expect(ws.getState().fresh).toBe(false)
    expect(ws.getState().link).toBe('open')
  })

  it('закрытие пользователем не запускает переподключение', () => {
    const ws = transport()
    ws.connect()
    ws.close()
    vi.advanceTimersByTime(60_000)
    expect(FakeSocket.all).toHaveLength(1)
    expect(ws.getState().link).toBe('closed')
  })

  it('после переподключения данные устаревшие до первого снимка', () => {
    const { ws, socket } = live({ reconnectBaseMs: 100 })
    socket.serverDrops()
    vi.advanceTimersByTime(100)
    const next = FakeSocket.all.at(-1)
    next?.serverOpen()
    expect(ws.getState().link).toBe('open')
    expect(ws.getState().stale).toBe(true)
    next?.serverSends(frame({ carSnapshot: snapshot(0, 9) }))
    expect(ws.getState().stale).toBe(false)
  })
})

describe('молчание источника', () => {
  it('без новых снимков данные становятся устаревшими, связь остаётся открытой', () => {
    const { ws } = live({ staleAfterMs: 3000 })
    vi.advanceTimersByTime(2999)
    expect(ws.getState().fresh).toBe(true)
    vi.advanceTimersByTime(2)
    expect(ws.getState().fresh).toBe(false)
    expect(ws.getState().stale).toBe(true)
    expect(ws.getState().link).toBe('open')
  })

  it('регулярные снимки держат данные свежими', () => {
    const { ws, socket } = live({ staleAfterMs: 3000 })
    for (let i = 0; i < 10; i += 1) {
      vi.advanceTimersByTime(1000)
      socket.serverSends(frame({ carSnapshot: snapshot(0, i) }))
    }
    expect(ws.getState().fresh).toBe(true)
  })

  it('испорченные сообщения не продлевают свежесть', () => {
    const { ws, socket } = live({ staleAfterMs: 3000 })
    vi.advanceTimersByTime(2000)
    socket.serverSends('{broken')
    vi.advanceTimersByTime(1100)
    expect(ws.getState().fresh).toBe(false)
  })
})

describe('команды', () => {
  it('до подключения заблокированы, в сокет ничего не уходит', async () => {
    const ws = transport()
    const outcome = await ws.callFloor(3, 'down')
    expect(outcome).toMatchObject({ kind: 'blocked', reason: 'closed' })
  })

  it('при открытой связи, но без свежего снимка заблокированы', async () => {
    const ws = transport()
    ws.connect()
    const socket = FakeSocket.all[0]
    socket?.serverOpen()
    const outcome = await ws.callFloor(3, 'down')
    expect(outcome).toMatchObject({ kind: 'blocked', reason: 'stale' })
    expect(socket?.sent).toEqual([])
  })

  it('после обрыва заблокированы до свежего снимка нового соединения', async () => {
    const { ws, socket } = live({ reconnectBaseMs: 100 })
    socket.serverDrops()
    expect(await ws.callFloor(3, 'down')).toMatchObject({ kind: 'blocked', reason: 'closed' })
    vi.advanceTimersByTime(100)
    const next = FakeSocket.all.at(-1)
    next?.serverOpen()
    expect(await ws.callFloor(3, 'down')).toMatchObject({ kind: 'blocked', reason: 'stale' })
    next?.serverSends(frame({ carSnapshot: snapshot() }))
    const pending = ws.callFloor(3, 'down')
    expect(next?.sent).toHaveLength(1)
    next?.serverSends(frame({ commandResult: { requestId: 1, status: 'applied' } }))
    await pending
  })

  it('при несовместимой версии заблокированы', async () => {
    const { ws, socket } = live()
    socket.serverSends('{"version":2}')
    expect(await ws.callFloor(3, 'down')).toMatchObject({ kind: 'blocked', reason: 'incompatible' })
  })

  it('уходят конвертом с загрузкой и запуском платы и растущим номером', async () => {
    const { ws, socket } = live()
    const first = ws.callFloor(3, 'down')
    const second = ws.ackAlarm(5)
    const [a, b] = socket.sent.map((text) => JSON.parse(text) as Record<string, unknown>)
    expect(a).toMatchObject({
      version: 1,
      bootId: 10,
      runId: 2,
      message: { callFloor: { requestId: 1, floor: 3, direction: 'down' } },
    })
    expect(b).toMatchObject({ message: { ackAlarm: { requestId: 2, alarmId: 5 } } })
    expect((b?.seq as number) - (a?.seq as number)).toBe(1)
    socket.serverSends(frame({ commandResult: { requestId: 1, status: 'accepted' } }))
    socket.serverSends(frame({ commandResult: { requestId: 2, status: 'applied' } }))
    await Promise.all([first, second])
  })

  it('все пять команд и чтение журнала формируют нужные сообщения', () => {
    const { ws, socket } = live()
    void ws.injectFault(1)
    void ws.recoverCar(1)
    void ws.simControl('pause')
    void ws.readLog(7)
    const kinds = socket.sent.map((text) => Object.keys(JSON.parse(text).message)[0])
    expect(kinds).toEqual(['injectFault', 'recoverCar', 'simControl', 'readLog'])
    expect(JSON.parse(socket.sent[2] ?? '{}').message.simControl.action).toBe('pause')
    expect(JSON.parse(socket.sent[3] ?? '{}').message.readLog.fromIndex).toBe(7)
  })

  it('чтение журнала возвращает sent сразу, остальные ждут результата', async () => {
    const { ws } = live()
    expect(await ws.readLog(0)).toMatchObject({ kind: 'sent' })
  })

  it('результат разрешает ожидание, причина отказа сохраняется', async () => {
    const { ws, socket } = live()
    const pending = ws.callFloor(11, 'down')
    socket.serverSends(frame({ commandResult: { requestId: 1, status: { rejected: 'noSuchFloor' } } }))
    expect(await pending).toEqual({ kind: 'result', requestId: 1, status: { rejected: 'noSuchFloor' } })
  })

  it('повтор того же requestId, пока ответа нет, не отправляется второй раз', async () => {
    const { ws, socket } = live()
    const first = ws.callFloor(3, 'down', 77)
    const again = ws.callFloor(3, 'down', 77)
    expect(socket.sent).toHaveLength(1)
    socket.serverSends(frame({ commandResult: { requestId: 77, status: 'accepted' } }))
    expect(await first).toEqual(await again)
  })

  it('без ответа за отведённое время команда считается просроченной', async () => {
    const { ws, socket } = live({ requestTimeoutMs: 1000, staleAfterMs: 60_000 })
    const pending = ws.callFloor(3, 'down')
    vi.advanceTimersByTime(1001)
    expect(await pending).toEqual({ kind: 'result', requestId: 1, status: 'expired' })
    socket.serverSends(frame({ commandResult: { requestId: 1, status: 'applied' } }))
    expect(ws.getState().link).toBe('open')
  })

  it('обрыв связи разрешает ожидающие команды статусом unknown', async () => {
    const { ws, socket } = live()
    const pending = ws.callFloor(3, 'down')
    socket.serverDrops()
    expect(await pending).toEqual({ kind: 'result', requestId: 1, status: 'unknown' })
    expect(ws.getState().link).toBe('closed')
  })

  it('недопустимая команда не уходит в сокет', async () => {
    const { ws, socket } = live()
    const outcome = await ws.callFloor(300, 'down')
    expect(outcome).toMatchObject({ kind: 'blocked', reason: 'invalid' })
    expect(socket.sent).toEqual([])
  })

  it('номер, уже получивший ответ, можно использовать снова: старый ответ не засчитывается', async () => {
    const { ws, socket } = live()
    const first = ws.callFloor(3, 'down', 5)
    socket.serverSends(frame({ commandResult: { requestId: 5, status: 'applied' } }))
    await first
    const second = ws.callFloor(3, 'down', 5)
    socket.serverSends(frame({ carSnapshot: snapshot(0, 1) }))
    const early = await Promise.race([second, Promise.resolve('waiting' as const)])
    expect(early).toBe('waiting')
    socket.serverSends(frame({ commandResult: { requestId: 5, status: 'accepted' } }))
    expect(await second).toEqual({ kind: 'result', requestId: 5, status: 'accepted' })
  })

  it('автоматические номера не пересекаются с вручную заданными', async () => {
    const { ws, socket } = live()
    void ws.callFloor(3, 'down')
    void ws.callFloor(4, 'down', 50)
    void ws.callFloor(5, 'down')
    const ids = socket.sent.map((text) => JSON.parse(text).message.callFloor.requestId)
    expect(ids).toEqual([1, 50, 2])
  })
})
