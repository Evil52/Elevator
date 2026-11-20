import { expect, it, vi } from 'vitest'
import html from '../index.html?raw'
import raw from '../../recordings/b1/timeline.json'

vi.mock('./scene', () => ({ createScene: async () => ({ show: vi.fn() }) }))

it('показывает назначения и переключает этапы B1 из диалога сценария', async () => {
  history.replaceState(null, '', '?scenario=b1')
  document.body.replaceChildren(...new DOMParser().parseFromString(html, 'text/html').body.childNodes)
  const dialog = document.querySelector('dialog')!
  dialog.showModal = () => { dialog.open = true }
  dialog.close = () => { dialog.open = false }
  const click = (selector: string) => document.querySelector<HTMLElement>(selector)!.click()
  const text = (selector: string) => document.querySelector(selector)?.textContent
  vi.stubGlobal('fetch', vi.fn(async () => ({ ok: true, json: async () => raw })))
  try {
    await import('./main')
    await vi.waitFor(() => expect(document.querySelectorAll('#calls tbody tr')).toHaveLength(2))
    expect(document.querySelectorAll('#matrix .selected')).toHaveLength(2)
    click('[data-plan="greedy"]')
    expect(document.querySelector('[data-plan="greedy"]')?.getAttribute('aria-pressed')).toBe('true')
    expect(document.activeElement?.getAttribute('data-plan')).toBe('greedy')
    click('#scenario-info')
    expect(dialog.open).toBe(true)
    expect([...dialog.querySelectorAll('a')].map((a) => a.search)).toEqual(['?scenario=demo', '?scenario=sil'])
    const alarm = [...dialog.querySelectorAll('button[data-go]')].find((b) => b.textContent?.includes('Авария B1'))!
    alarm.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    expect(dialog.open).toBe(false)
    expect(text('#mode-A')).toBe('Аварийное торможение')
    expect(text('#calls tbody tr:last-child .row-id')).toBe('C')
    click('[data-plan="optimal"]')
    expect(document.querySelectorAll('#matrix .blocked')).toHaveLength(1)
    expect(document.querySelectorAll('#matrix .selected')).toHaveLength(1)
    const scrub = document.querySelector<HTMLInputElement>('#scrub')!
    scrub.value = '4251'
    scrub.dispatchEvent(new Event('input'))
    expect(text('#mode-A')).toBe('Заблокирована')
  } finally {
    vi.unstubAllGlobals()
  }
})

it('отображает подписи таблицы как текст', async () => {
  const { callsPanel } = await import('./panels')
  const panel = callsPanel([
    { id: 'H08', floor: 8, direction: 'UP', state: 'QUEUED', registeredMs: 0 },
  ], () => '<em>текст</em>')
  expect(panel.querySelector('.mono')?.textContent).toBe('<em>текст</em>')
  expect(panel.querySelector('em')).toBeNull()
})
