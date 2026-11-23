import { act, render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { App } from '../../src/App.tsx'
import { linkClosed } from '../../src/transport/store.ts'
import { FakeTransport, alarm, freshState } from './fake.ts'

describe('панель целиком', () => {
  it('показывает все разделы', () => {
    render(<App transport={new FakeTransport(freshState())} />)
    expect(screen.getByRole('heading', { name: 'Панель диспетчера' })).toBeTruthy()
    expect(screen.getByRole('status').textContent).toContain('Данные свежие')
    expect(screen.getByTestId('building')).toBeTruthy()
    expect(screen.getByRole('region', { name: 'Вызовы с этажей' })).toBeTruthy()
    expect(screen.getByRole('region', { name: 'Реестр тревог' })).toBeTruthy()
  })

  it('следует за транспортом: обрыв связи сразу делает данные устаревшими и блокирует кнопки', () => {
    const transport = new FakeTransport(freshState({ alarms: [alarm(1)], alarmTotal: 1 }))
    render(<App transport={transport} />)
    const call = screen.getByRole('button', { name: 'Вызов вверх с этажа 2' }) as HTMLButtonElement
    expect(call.disabled).toBe(false)
    act(() => transport.set(linkClosed(transport.getState())))
    expect(screen.getByRole('alert').textContent).toContain('Данные устарели')
    expect(call.disabled).toBe(true)
    expect(screen.getByRole('button', { name: 'Подтвердить тревогу 1' })).toHaveProperty('disabled', true)
    expect(screen.getByTestId('car-1').textContent).toContain('устарело')
  })

  it('новый снимок двигает кабину', () => {
    const transport = new FakeTransport(freshState())
    render(<App transport={transport} />)
    const before = screen.getByTestId('car-0').style.bottom
    act(() =>
      transport.set({
        ...transport.getState(),
        cars: { ...transport.getState().cars, 0: { ...transport.getState().cars[0]!, positionMm: 13500 } },
      }),
    )
    expect(screen.getByTestId('car-0').style.bottom).not.toBe(before)
    expect(parseFloat(screen.getByTestId('car-0').style.bottom)).toBeCloseTo(50, 1)
  })
})
