import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import type { Event } from '@elevator/protocol-v1'
import { describe, expect, it } from 'vitest'
import { EventLogView } from '../../src/ui/EventLogView.tsx'
import { FakeTransport, alarm, freshState, staleState } from './fake.ts'

function event(index: number, extra: Partial<Event> = {}): Event {
  return { index, atMs: index * 1000, kind: 'assigned', car: 0, id: index + 100, ...extra }
}

function setup(state = freshState()) {
  const transport = new FakeTransport(state)
  const view = render(<EventLogView state={state} transport={transport} />)
  return { transport, view }
}

describe('журнал событий', () => {
  it('пустой журнал сообщает об этом', () => {
    setup()
    expect(screen.getByText('Событий нет')).toBeTruthy()
  })

  it('события показаны по-русски с кабиной и временем', () => {
    setup(
      freshState({
        events: [
          event(0, { kind: 'alarmRaised', car: 1, atMs: 12300 }),
          event(1, { kind: 'requeued', car: 2 }),
          event(2, { kind: 'cancelled', car: null }),
        ],
      }),
    )
    const text = screen.getByTestId('events').textContent ?? ''
    for (const word of ['Тревога поднята', 'Вызов возвращён в очередь', 'Вызов отменён', 'Кабина 2', 'Кабина 3', '12,3 с']) {
      expect(text).toContain(word)
    }
    expect(text).not.toContain('alarmRaised')
    expect(text).not.toContain('Кабина 1')
    expect(text).not.toContain('NaN')
  })

  it('все виды событий названы', () => {
    const kinds = ['assigned', 'arrived', 'served', 'requeued', 'cancelled', 'alarmRaised', 'alarmAcked', 'alarmCleared'] as const
    setup(freshState({ events: kinds.map((kind, i) => event(i, { kind })) }))
    const text = screen.getByTestId('events').textContent ?? ''
    for (const word of ['Назначен вызов', 'Кабина прибыла', 'Вызов обслужен', 'Тревога подтверждена', 'Тревога снята']) {
      expect(text).toContain(word)
    }
  })

  it('новые события сверху', () => {
    setup(freshState({ events: [event(0), event(1), event(2)] }))
    const items = screen.getAllByRole('listitem')
    expect(items[0]?.getAttribute('data-index')).toBe('2')
    expect(items.at(-1)?.getAttribute('data-index')).toBe('0')
  })

  it('потерянные события названы числом', () => {
    setup(freshState({ events: [event(5)], eventsDroppedBefore: 5 }))
    expect(screen.getByText(/Потеряно событий до этого журнала: 5/)).toBeTruthy()
  })

  it('без потерь строки нет', () => {
    setup(freshState({ events: [event(0)] }))
    expect(screen.queryByText(/Потеряно/)).toBeNull()
  })
})

describe('чтение журнала', () => {
  it('при свежих данных читает журнал с начала', async () => {
    const { transport } = setup()
    await waitFor(() => expect(transport.calls).toEqual(['readLog:0']))
  })

  it('при устаревших данных ничего не запрашивает', async () => {
    const { transport } = setup(staleState())
    await new Promise((resolve) => setTimeout(resolve, 30))
    expect(transport.calls).toEqual([])
  })

  it('пустой ответ не вызывает повторных запросов', async () => {
    const { transport } = setup()
    await waitFor(() => expect(transport.calls).toHaveLength(1))
    transport.set({ ...transport.getState() })
    await new Promise((resolve) => setTimeout(resolve, 30))
    expect(transport.calls).toEqual(['readLog:0'])
  })

  it('если пришли события, дочитывает со следующего индекса', async () => {
    const { transport, view } = setup()
    await waitFor(() => expect(transport.calls).toEqual(['readLog:0']))
    const next = { ...transport.getState(), events: [event(0), event(1), event(2)] }
    transport.set(next)
    view.rerender(<EventLogView state={next} transport={transport} />)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:0', 'readLog:3']))
  })

  it('кнопка «Обновить журнал» читает с места, где остановились', async () => {
    const state = freshState({ events: [event(0), event(1)] })
    const { transport } = setup(state)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:2']))
    await userEvent.click(screen.getByRole('button', { name: 'Обновить журнал' }))
    await waitFor(() => expect(transport.calls).toEqual(['readLog:2', 'readLog:2']))
  })

  it('кнопка недоступна без свежих данных', () => {
    setup(staleState())
    expect((screen.getByRole('button', { name: 'Обновить журнал' }) as HTMLButtonElement).disabled).toBe(true)
  })

  it('смена тревог запускает перечитывание журнала', async () => {
    const first = freshState({ events: [event(0)], alarms: [alarm(1)], alarmTotal: 1 })
    const { transport, view } = setup(first)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:1']))
    const second = { ...first, alarms: [{ ...alarm(1), state: 'acknowledged' as const }] }
    transport.set(second)
    view.rerender(<EventLogView state={second} transport={transport} />)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:1', 'readLog:1']))
  })

  it('после восстановления связи журнал читается снова с того же места', async () => {
    const live = freshState({ events: [event(0), event(1)] })
    const { transport, view } = setup(live)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:2']))
    const down = { ...live, fresh: false, stale: true }
    transport.set(down)
    view.rerender(<EventLogView state={down} transport={transport} />)
    await new Promise((resolve) => setTimeout(resolve, 20))
    expect(transport.calls).toEqual(['readLog:2'])
    transport.set(live)
    view.rerender(<EventLogView state={live} transport={transport} />)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:2', 'readLog:2']))
  })

  it('после сброса запуска журнал читается с нуля', async () => {
    const first = freshState({ events: [event(0), event(1), event(2)] })
    const { transport, view } = setup(first)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:3']))
    const reset = { ...transport.getState(), identity: { bootId: 1, runId: 2 }, events: [] }
    transport.set(reset)
    view.rerender(<EventLogView state={reset} transport={transport} />)
    await waitFor(() => expect(transport.calls).toEqual(['readLog:3', 'readLog:0']))
  })
})
