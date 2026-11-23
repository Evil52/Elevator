import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { HallCalls } from '../../src/ui/HallCalls.tsx'
import { FakeTransport, freshState, staleState } from './fake.ts'

function setup(state = freshState()) {
  const transport = new FakeTransport(state)
  render(<HallCalls state={state} transport={transport} />)
  return transport
}

describe('вызовы с этажей', () => {
  it('на первом этаже нет кнопки «вниз», на десятом нет кнопки «вверх»', () => {
    setup()
    expect(screen.queryByRole('button', { name: 'Вызов вниз с этажа 1' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Вызов вверх с этажа 1' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Вызов вверх с этажа 10' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Вызов вниз с этажа 10' })).toBeTruthy()
  })

  it('кнопок восемнадцать: девять вверх и девять вниз', () => {
    setup()
    expect(screen.getAllByRole('button')).toHaveLength(18)
  })

  it('нажатие отправляет вызов нужного этажа и направления', async () => {
    const transport = setup()
    await userEvent.click(screen.getByRole('button', { name: 'Вызов вниз с этажа 3' }))
    expect(transport.calls).toEqual(['callFloor:3:down'])
  })

  it('кнопки работают с клавиатуры', async () => {
    const transport = setup()
    screen.getByRole('button', { name: 'Вызов вверх с этажа 8' }).focus()
    await userEvent.keyboard('{Enter}')
    expect(transport.calls).toEqual(['callFloor:8:up'])
  })

  it('при устаревших данных все кнопки недоступны и вызов не уходит', async () => {
    const transport = setup(staleState())
    const buttons = screen.getAllByRole('button')
    expect(buttons.every((b) => (b as HTMLButtonElement).disabled)).toBe(true)
    await userEvent.click(buttons[0] as HTMLElement)
    expect(transport.calls).toEqual([])
    expect(screen.getByText(/Команды недоступны/)).toBeTruthy()
  })

  it('при обрыве связи команды недоступны', () => {
    setup(staleState({ link: 'closed' }))
    expect(screen.getAllByRole('button').every((b) => (b as HTMLButtonElement).disabled)).toBe(true)
  })

  it('при несовместимой версии команды недоступны', () => {
    setup(freshState({ incompatibleVersion: 2 }))
    expect(screen.getAllByRole('button').every((b) => (b as HTMLButtonElement).disabled)).toBe(true)
  })

  it('результат объявляется в области aria-live, причина отказа по-русски', async () => {
    const transport = setup()
    transport.outcome = { kind: 'result', requestId: 1, status: { rejected: 'noSuchFloor' } }
    await userEvent.click(screen.getByRole('button', { name: 'Вызов вниз с этажа 3' }))
    const live = await screen.findByRole('log')
    expect(live.getAttribute('aria-live')).toBe('polite')
    await waitFor(() => expect(live.textContent).toContain('такого этажа нет'))
  })

  it('принятый вызов подтверждается текстом', async () => {
    setup()
    await userEvent.click(screen.getByRole('button', { name: 'Вызов вверх с этажа 2' }))
    expect((await screen.findByRole('log')).textContent).toContain('Вызов принят')
  })

  it('пока нет ответа, та же кнопка недоступна и второй вызов не уходит', async () => {
    const transport = setup()
    let release: (value: never) => void = () => {}
    transport.outcome = new Promise((resolve) => {
      release = resolve as (value: never) => void
    })
    const button = screen.getByRole('button', { name: 'Вызов вниз с этажа 4' }) as HTMLButtonElement
    await userEvent.click(button)
    expect(button.disabled).toBe(true)
    await userEvent.click(button)
    expect(transport.calls).toEqual(['callFloor:4:down'])
    release({ kind: 'result', requestId: 1, status: 'accepted' } as never)
    await waitFor(() => expect(button.disabled).toBe(false))
  })
})
