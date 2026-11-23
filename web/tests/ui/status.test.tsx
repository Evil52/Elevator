import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { initialState } from '../../src/transport/store.ts'
import { StatusBanner } from '../../src/ui/StatusBanner.tsx'
import { freshState, staleState } from './fake.ts'

describe('баннер состояния', () => {
  it('до первого соединения показывает подключение', () => {
    render(<StatusBanner state={initialState()} />)
    expect(screen.getByRole('status').textContent).toContain('Подключение к шлюзу')
  })

  it('при свежих данных сообщает об этом текстом и значком', () => {
    render(<StatusBanner state={freshState()} />)
    const banner = screen.getByRole('status')
    expect(banner.textContent).toContain('Данные свежие')
    expect(banner.textContent).toContain('✓')
  })

  it('при обрыве связи показывает «Данные устарели» и последний кадр не скрывает', () => {
    render(<StatusBanner state={staleState({ link: 'closed' })} />)
    const alert = screen.getByRole('alert')
    expect(alert.textContent).toContain('Нет связи со шлюзом')
    expect(alert.textContent).toContain('Данные устарели')
    expect(alert.textContent).toContain('✕')
  })

  it('если связи нет и кадров не было, про последний кадр не говорится', () => {
    render(<StatusBanner state={{ ...initialState(), link: 'closed' }} />)
    const alert = screen.getByRole('alert')
    expect(alert.textContent).toContain('Нет связи со шлюзом')
    expect(alert.textContent).not.toContain('последний кадр')
  })

  it('связь есть, а плата молчит: данные устарели по другой причине', () => {
    render(<StatusBanner state={staleState()} />)
    const alert = screen.getByRole('alert')
    expect(alert.textContent).toContain('Данные устарели')
    expect(alert.textContent).toContain('плата не отвечает')
    expect(alert.textContent).not.toContain('Нет связи со шлюзом')
  })

  it('связь открыта, но данных платы ещё не было', () => {
    render(<StatusBanner state={{ ...initialState(), link: 'open' }} />)
    expect(screen.getByRole('status').textContent).toContain('Ожидание данных платы')
  })

  it('несовместимая версия показывается отдельно от недоступности источника', () => {
    render(<StatusBanner state={freshState({ incompatibleVersion: 2 })} />)
    const alerts = screen.getAllByRole('alert')
    expect(alerts.map((a) => a.textContent).join(' ')).toContain('Несовместимая версия протокола: 2')
    expect(screen.queryByText(/Нет связи/)).toBeNull()
  })

  it('число отброшенных сообщений видно, когда они были', () => {
    render(<StatusBanner state={freshState({ invalidMessages: 3 })} />)
    expect(screen.getByText(/Отброшено некорректных сообщений: 3/)).toBeTruthy()
  })

  it('без отброшенных сообщений строки нет', () => {
    render(<StatusBanner state={freshState()} />)
    expect(screen.queryByText(/Отброшено/)).toBeNull()
  })
})
