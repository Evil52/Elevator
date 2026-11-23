import { useEffect } from 'react'
import type { PanelState, SimTransport } from '../transport/types.ts'
import { canCommand } from './commands.ts'
import { EVENT_KIND_TEXT, formatSeconds } from './text.ts'

const SHOWN = 100

export function EventLogView({ state, transport }: { state: PanelState; transport: SimTransport }) {
  const enabled = canCommand(state)
  const next = (state.events.at(-1)?.index ?? -1) + 1

  useEffect(() => {
    if (enabled) void transport.readLog(next)
  }, [enabled, next, state.alarms, transport])

  const shown = state.events.slice(-SHOWN).reverse()
  return (
    <section className="event-log" aria-label="Журнал событий">
      <button type="button" disabled={!enabled} onClick={() => void transport.readLog(next)}>
        Обновить журнал
      </button>
      {state.eventsDroppedBefore > 0 && (
        <p>Потеряно событий до этого журнала: {state.eventsDroppedBefore}</p>
      )}
      <div data-testid="events">
        {shown.length === 0 ? (
          <p>Событий нет</p>
        ) : (
          <ul>
            {shown.map((event) => (
              <li key={event.index} data-index={event.index}>
                <span>{formatSeconds(event.atMs)}</span>
                <strong>{EVENT_KIND_TEXT[event.kind]}</strong>
                {typeof event.car === 'number' && <span>Кабина {event.car + 1}</span>}
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}
