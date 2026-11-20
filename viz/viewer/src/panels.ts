import { icon } from './icons'
import { type Call, type Car, type Dispatch, type Status } from './timeline'
import { type Snapshot } from './player'

export function element<K extends keyof HTMLElementTagNameMap>(
  tag: K, className = '', ...children: (Node | string)[]
) {
  const node = document.createElement(tag)
  if (className) node.className = className
  node.append(...children)
  return node
}

function iconNode(name: string) {
  const template = document.createElement('template')
  template.innerHTML = icon(name)
  return template.content
}

function carLabel(car: Car) {
  return element('span', 'row-id', element('b', 'dot-id ' + car), car)
}

export const states: Record<Status, string> = {
  QUEUED: 'В очереди', ASSIGNED: 'Назначен', ARRIVED: 'Прибытие', SERVED: 'Обслужен',
}

export function callsPanel(active: Snapshot['calls'][string][], clock: (ms: number) => string) {
  if (!active.length) return element('p', 'empty', 'Зарегистрированных вызовов пока нет.')
  const table = element('table', 'queue-table')
  const headings = table.createTHead().insertRow()
  for (const title of ['Этаж', 'Кабина', 'Состояние', 'Регистрация']) {
    headings.append(element('th', '', title))
  }
  const body = table.createTBody()
  for (const call of active) {
    const row = body.insertRow()
    const arrow = iconNode(call.direction === 'UP' ? 'up' : 'down')
    row.insertCell().append(element('span', 'floor-call', arrow, call.floor + ' этаж'))
    row.insertCell().append(call.car ? carLabel(call.car) : element('span', 'row-id', '—'))
    const chip = element('span', 'state-chip ' + call.state, call.state)
    chip.title = states[call.state]
    row.insertCell().append(chip)
    row.insertCell().append(element('span', 'mono', clock(call.registeredMs)))
  }
  return table
}

export type Strategy = 'optimal' | 'greedy'

export function matrixPanel(
  d: Dispatch | undefined, calls: Call[], strategy: Strategy, sec: (ms: number) => string,
) {
  if (!d) return element('p', 'empty', 'Ожидание расчёта диспетчера.')
  const table = element('table', 'eta-table')
  const headings = table.createTHead().insertRow()
  const columnNames = d.columns.map((id) => {
    const call = calls.find((c) => c.id === id)!
    return call.floor + ' этаж ' + (call.direction === 'UP' ? '↑' : '↓')
  })
  for (const title of ['Кабина', ...columnNames]) {
    const th = element('th', '', title)
    th.scope = 'col'
    headings.append(th)
  }
  const body = table.createTBody()
  d.rows.forEach((id, r) => {
    const row = body.insertRow()
    const heading = element('th', '', carLabel(id))
    heading.scope = 'row'
    row.append(heading)
    d.matrix[r].forEach((ms, column) => {
      const value = element('span', 'eta-value')
      row.insertCell().append(value)
      if (ms === null || !d.allowed[r][column]) {
        value.classList.add('blocked')
        value.title = 'Назначение запрещено'
        value.setAttribute('aria-label', value.title)
        value.textContent = '—'
        return
      }
      if (d[strategy].pairs.some((p) => p.car === id && p.column === column)) {
        value.classList.add('selected')
        value.title = 'Выбрано в этом плане'
        value.append(iconNode('check'))
      }
      value.append(sec(ms) + ' с')
    })
  })
  const comparison = element('div', 'comparison')
  comparison.setAttribute('aria-label', 'Сравнить выбранные пары')
  for (const key of ['optimal', 'greedy'] as const) {
    const plan = d[key]
    const value = element('strong')
    if (plan.pairs.length) value.append(sec(plan.totalEtaMs) + ' ', element('small', '', 'с'))
    else value.textContent = '—'
    const name = key === 'optimal' ? 'Оптимум' : 'Жадный выбор'
    const button = element('button', '', element('span', '', name + ' · Σ ETA'), value)
    button.dataset.plan = key
    button.title = 'Показать выбранные пары'
    button.setAttribute('aria-pressed', String(strategy === key))
    comparison.append(button)
  }
  const diff = d.greedy.totalEtaMs - d.optimal.totalEtaMs
  let note = 'Суммы ETA совпадают'
  if (d.greedy.pairs.length !== d.optimal.pairs.length) {
    note = 'Назначено пар: оптимум ' + d.optimal.pairs.length + ', жадный ' + d.greedy.pairs.length
  } else if (diff > 0) note = 'Выигрыш по ETA — ' + sec(diff) + ' с'
  const result = document.createDocumentFragment()
  result.append(table, comparison, element('p', 'matrix-note', iconNode(diff > 0 ? 'trend' : 'check'), note))
  return result
}
