import { useState } from 'react'
import type { PanelState, SimTransport } from '../transport/types.ts'
import { blockText, canCommand, describeOutcome } from './commands.ts'

const FLOORS = 10

export function HallCalls({ state, transport }: { state: PanelState; transport: SimTransport }) {
  const [pending, setPending] = useState<ReadonlySet<string>>(new Set())
  const [message, setMessage] = useState('')
  const enabled = canCommand(state)
  const blocked = blockText(state)

  async function call(floor: number, direction: 'up' | 'down') {
    const key = `${floor}:${direction}`
    setPending((current) => new Set(current).add(key))
    const outcome = await transport.callFloor(floor, direction)
    setMessage(describeOutcome(outcome))
    setPending((current) => {
      const next = new Set(current)
      next.delete(key)
      return next
    })
  }

  function button(floor: number, direction: 'up' | 'down') {
    const key = `${floor}:${direction}`
    const word = direction === 'up' ? 'вверх' : 'вниз'
    return (
      <button
        type="button"
        aria-label={`Вызов ${word} с этажа ${floor}`}
        disabled={!enabled || pending.has(key)}
        onClick={() => void call(floor, direction)}
      >
        <span aria-hidden="true">{direction === 'up' ? '▲' : '▼'}</span>
      </button>
    )
  }

  return (
    <section className="hall-calls" aria-label="Вызовы с этажей">
      {blocked !== null && <p className="blocked">{blocked}</p>}
      <ol>
        {Array.from({ length: FLOORS }, (_, index) => FLOORS - index).map((floor) => (
          <li key={floor}>
            <span className="floor-number">Этаж {floor}</span>
            {floor < FLOORS && button(floor, 'up')}
            {floor > 1 && button(floor, 'down')}
          </li>
        ))}
      </ol>
      <div role="log" aria-live="polite" className="call-result">
        {message}
      </div>
    </section>
  )
}
