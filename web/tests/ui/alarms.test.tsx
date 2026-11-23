import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { AlarmList } from '../../src/ui/AlarmList.tsx'
import { FakeTransport, alarm, freshState, staleState } from './fake.ts'

function setup(state = freshState()) {
  const transport = new FakeTransport(state)
  render(<AlarmList state={state} transport={transport} />)
  return transport
}

describe('реестр тревог', () => {
  it('без тревог написано об этом', () => {
    setup()
    expect(screen.getByText('Тревог нет')).toBeTruthy()
  })

  it('тревога показывает вид, кабину и состояние текстом', () => {
    setup(freshState({ alarms: [alarm(7, { car: 1 })], alarmTotal: 1 }))
    const row = screen.getByTestId('alarm-7')
    expect(row.textContent).toContain('Отказ кабины')
    expect(row.textContent).toContain('Кабина 2')
    expect(row.textContent).toContain('Активна')
  })

  it('тревога без кабины, например потеря связи, не придумывает кабину', () => {
    setup(freshState({ alarms: [alarm(2, { kind: 'linkLost', car: null })], alarmTotal: 1 }))
    const row = screen.getByTestId('alarm-2')
    expect(row.textContent).toContain('Потеряна связь')
    expect(row.textContent).not.toContain('Кабина')
  })

  it('все виды и состояния названы по-русски', () => {
    const alarms = [
      alarm(1, { kind: 'fireRecall', car: null }),
      alarm(2, { kind: 'logOverflow', car: null, state: 'acknowledged' }),
      alarm(3, { state: 'cleared' }),
    ]
    setup(freshState({ alarms, alarmTotal: 3 }))
    const text = screen.getByTestId('alarms').textContent ?? ''
    for (const word of ['Пожарная опасность', 'Переполнение журнала', 'Подтверждена', 'Снята']) {
      expect(text).toContain(word)
    }
    expect(text).not.toContain('fireRecall')
  })

  it('подтвердить можно только активную тревогу', () => {
    setup(
      freshState({
        alarms: [alarm(1), alarm(2, { state: 'acknowledged' }), alarm(3, { state: 'cleared' })],
        alarmTotal: 3,
      }),
    )
    expect(screen.getAllByRole('button')).toHaveLength(1)
    expect(screen.getByRole('button', { name: 'Подтвердить тревогу 1' })).toBeTruthy()
  })

  it('подтверждение отправляет номер тревоги', async () => {
    const transport = setup(freshState({ alarms: [alarm(5)], alarmTotal: 1 }))
    await userEvent.click(screen.getByRole('button', { name: 'Подтвердить тревогу 5' }))
    expect(transport.calls).toEqual(['ackAlarm:5'])
  })

  it('при устаревших данных подтверждение недоступно', async () => {
    const transport = setup(staleState({ alarms: [alarm(5)], alarmTotal: 1 }))
    const button = screen.getByRole('button', { name: 'Подтвердить тревогу 5' }) as HTMLButtonElement
    expect(button.disabled).toBe(true)
    await userEvent.click(button)
    expect(transport.calls).toEqual([])
  })

  it('если тревог больше, чем показано, об этом сказано', () => {
    setup(freshState({ alarms: [alarm(1)], alarmTotal: 12 }))
    expect(screen.getByText(/Показано 1 из 12/)).toBeTruthy()
  })
})
