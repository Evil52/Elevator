import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { FireControls } from '../../src/ui/FireControls.tsx'
import type { CommandOutcome } from '../../src/transport/types.ts'
import { FakeTransport, alarm, freshState, staleState } from './fake.ts'

function setup(state = freshState()) {
  const transport = new FakeTransport(state)
  render(<FireControls state={state} transport={transport} />)
  return transport
}

function fireState(state: 'active' | 'acknowledged' | 'cleared' = 'active') {
  return freshState({ alarms: [alarm(1, { kind: 'fireRecall', car: null, state })], alarmTotal: 1 })
}

describe('управление пожарным режимом', () => {
  it('без тревоги предлагает включить возврат, но не показывает его снятие', () => {
    setup()
    expect(screen.getByText('Пожарный возврат не активен')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Включить пожарный возврат' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Снять пожарный режим' })).toBeNull()
  })

  it('перед включением требует подтверждение и позволяет отменить его', async () => {
    const transport = setup()
    await userEvent.click(screen.getByRole('button', { name: 'Включить пожарный возврат' }))
    expect(screen.getByRole('alertdialog').textContent).toContain('Текущие вызовы будут отменены')
    expect(transport.calls).toEqual([])
    await userEvent.click(screen.getByRole('button', { name: 'Отмена' }))
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(transport.calls).toEqual([])
  })

  it('подтверждение отправляет fireRecall и показывает результат', async () => {
    const transport = setup()
    await userEvent.click(screen.getByRole('button', { name: 'Включить пожарный возврат' }))
    await userEvent.click(screen.getByRole('button', { name: 'Подтвердить пожарный возврат' }))
    expect(transport.calls).toEqual(['simControl:fireRecall'])
    expect(screen.queryByRole('alertdialog')).toBeNull()
    expect(await screen.findByText('Вызов принят')).toBeTruthy()
  })

  it.each(['active', 'acknowledged'] as const)('состояние %s допускает снятие режима', async (alarmState) => {
    const transport = setup(fireState(alarmState))
    await userEvent.click(screen.getByRole('button', { name: 'Снять пожарный режим' }))
    expect(transport.calls).toEqual(['simControl:clearFire'])
  })

  it('снятая тревога снова открывает кнопку включения', () => {
    setup(fireState('cleared'))
    expect(screen.getByRole('button', { name: 'Включить пожарный возврат' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Снять пожарный режим' })).toBeNull()
  })

  it('при устаревших данных команды заблокированы', async () => {
    const transport = setup(staleState())
    const button = screen.getByRole('button', { name: 'Включить пожарный возврат' }) as HTMLButtonElement
    expect(button.disabled).toBe(true)
    await userEvent.click(button)
    expect(transport.calls).toEqual([])
  })

  it('не отправляет повторную команду, пока первая ещё выполняется', async () => {
    const transport = setup()
    let finish: (outcome: CommandOutcome) => void = () => {}
    transport.outcome = new Promise<CommandOutcome>((resolve) => {
      finish = resolve
    })
    await userEvent.click(screen.getByRole('button', { name: 'Включить пожарный возврат' }))
    await userEvent.click(screen.getByRole('button', { name: 'Подтвердить пожарный возврат' }))
    expect(transport.calls).toEqual(['simControl:fireRecall'])
    const repeat = screen.getByRole('button', { name: 'Включить пожарный возврат' }) as HTMLButtonElement
    expect(repeat.disabled).toBe(true)
    await userEvent.click(repeat)
    expect(transport.calls).toEqual(['simControl:fireRecall'])
    await act(async () => finish({ kind: 'result', requestId: 1, status: 'applied' }))
    expect(await screen.findByText('Команда выполнена')).toBeTruthy()
  })
})
