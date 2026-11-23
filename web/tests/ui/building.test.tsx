import { render, screen, within } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { Building } from '../../src/ui/Building.tsx'
import { car, freshState, staleState } from './fake.ts'

describe('схема здания', () => {
  it('рисует три шахты и десять этажей', () => {
    render(<Building state={freshState()} />)
    expect(screen.getAllByRole('listitem', { name: /Шахта/ })).toHaveLength(3)
    expect(screen.getAllByTestId('floor-label')).toHaveLength(10)
  })

  it('кабина стоит строго по телеметрии: позиция в процентах от высоты шахты', () => {
    render(<Building state={freshState()} />)
    expect(screen.getByTestId('car-0').style.bottom).toBe('0%')
    expect(parseFloat(screen.getByTestId('car-1').style.bottom)).toBeCloseTo(44.444, 2)
    expect(screen.getByTestId('car-2').style.bottom).toBe('100%')
  })

  it('подпись кабины содержит этаж, режим и двери текстом', () => {
    render(
      <Building
        state={freshState({
          cars: { 0: car(0, { floor: 3, mode: 'unavailable', doors: 'open', positionMm: 6000 }) },
        })}
      />,
    )
    const label = within(screen.getByTestId('car-0')).getByText(/Кабина 1/).closest('[data-testid]')
    const text = label?.textContent ?? ''
    expect(text).toContain('этаж 3')
    expect(text).toContain('Недоступна')
    expect(text).toContain('двери открыты')
  })

  it('между этажами этаж не показывается', () => {
    render(<Building state={freshState({ cars: { 0: car(0, { floor: null, positionMm: 1500 }) } })} />)
    expect(screen.getByTestId('car-0').textContent).toContain('между этажами')
  })

  it('у шахты без данных стоит пометка, а не выдуманная кабина', () => {
    render(<Building state={freshState({ cars: { 0: car(0) } })} />)
    expect(screen.getByTestId('shaft-1').textContent).toContain('нет данных')
    expect(screen.queryByTestId('car-1')).toBeNull()
  })

  it('при устаревших данных движение замирает и кабины помечены текстом', () => {
    render(<Building state={staleState()} />)
    const element = screen.getByTestId('car-1')
    expect(element.style.transition).toBe('none')
    expect(element.textContent).toContain('устарело')
  })

  it('при свежих данных движение плавное и пометки нет', () => {
    render(<Building state={freshState()} />)
    const element = screen.getByTestId('car-1')
    expect(element.style.transition).not.toBe('none')
    expect(element.textContent).not.toContain('устарело')
  })

  it('режимы названы по-русски, а не кодами', () => {
    render(
      <Building
        state={freshState({
          cars: {
            0: car(0, { mode: 'fireRecall' }),
            1: car(1, { mode: 'firefighters' }),
            2: car(2, { mode: 'normal' }),
          },
        })}
      />,
    )
    const text = screen.getByTestId('building').textContent ?? ''
    expect(text).toContain('Пожарная опасность')
    expect(text).toContain('Перевозка пожарных')
    expect(text).toContain('Работает')
    expect(text).not.toContain('fireRecall')
  })
})
