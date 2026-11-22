import { describe, expect, it } from 'vitest'
import { WsTransport } from '../src/transport/ws.ts'
import type { PanelState } from '../src/transport/types.ts'

const url = process.env.LIVE_GATEWAY_URL

async function until(
  ws: WsTransport,
  predicate: (state: PanelState) => boolean,
  ms = 8000,
): Promise<PanelState> {
  const end = Date.now() + ms
  while (Date.now() < end) {
    const state = ws.getState()
    if (predicate(state)) return state
    await new Promise((resolve) => setTimeout(resolve, 50))
  }
  throw new Error(`timeout waiting, state: ${JSON.stringify(ws.getState().link)}`)
}

describe.skipIf(!url)('живой шлюз и плата', () => {
  it('телеметрия, команды, тревоги, журнал и сброс', async () => {
    const ws = new WsTransport(url ?? '')
    ws.connect()
    const first = await until(ws, (s) => s.fresh && Object.keys(s.cars).length === 3)
    expect(first.identity).not.toBeNull()

    expect(await ws.callFloor(3, 'down')).toMatchObject({ kind: 'result', status: 'accepted' })
    expect(await ws.callFloor(11, 'down')).toMatchObject({
      kind: 'result',
      status: { rejected: 'noSuchFloor' },
    })

    expect(await ws.injectFault(1)).toMatchObject({ kind: 'result', status: 'applied' })
    const faulted = await until(ws, (s) => s.alarms.some((a) => a.car === 1 && a.state === 'active'))
    const alarm = faulted.alarms.find((a) => a.car === 1 && a.state === 'active')
    expect(alarm?.kind).toBe('carFault')
    await until(ws, (s) => s.cars[1]?.mode === 'unavailable')

    expect(await ws.ackAlarm(alarm?.id ?? 0)).toMatchObject({ kind: 'result', status: 'applied' })
    await until(ws, (s) => s.alarms.some((a) => a.id === alarm?.id && a.state === 'acknowledged'))

    expect(await ws.recoverCar(1)).toMatchObject({ kind: 'result', status: 'applied' })
    await until(ws, (s) => s.cars[1]?.mode === 'normal')

    expect(await ws.readLog(0)).toMatchObject({ kind: 'sent' })
    const logged = await until(ws, (s) => s.events.length >= 3)
    expect(logged.events.map((e) => e.kind)).toEqual(
      expect.arrayContaining(['alarmRaised', 'alarmAcked', 'alarmCleared']),
    )

    const runBefore = ws.getState().identity?.runId ?? 0
    expect(await ws.simControl('reset')).toMatchObject({ kind: 'result', status: 'applied' })
    const after = await until(ws, (s) => s.fresh && (s.identity?.runId ?? 0) > runBefore)
    expect(after.events).toEqual([])
    expect(after.alarms.filter((a) => a.state !== 'cleared')).toEqual([])

    ws.close()
    expect(ws.getState().link).toBe('closed')
    expect(await ws.callFloor(3, 'down')).toMatchObject({ kind: 'blocked', reason: 'closed' })
  }, 40_000)
})
