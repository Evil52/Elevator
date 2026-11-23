import { useState } from 'react'
import type { PanelState, SimTransport } from '../transport/types.ts'
import { canCommand, describeOutcome } from './commands.ts'
import { ALARM_KIND_TEXT, ALARM_STATE_TEXT } from './text.ts'

export function AlarmList({ state, transport }: { state: PanelState; transport: SimTransport }) {
  const [message, setMessage] = useState('')
  const enabled = canCommand(state)

  async function acknowledge(id: number) {
    const outcome = await transport.ackAlarm(id)
    setMessage(describeOutcome(outcome))
  }

  return (
    <section className="alarms" data-testid="alarms" aria-label="Реестр тревог">
      {state.alarms.length === 0 ? (
        <p>Тревог нет</p>
      ) : (
        <ul>
          {state.alarms.map((alarm) => (
            <li key={alarm.id} data-testid={`alarm-${alarm.id}`} className={`alarm alarm-${alarm.state}`}>
              <strong>{ALARM_KIND_TEXT[alarm.kind]}</strong>
              {typeof alarm.car === 'number' && <span>Кабина {alarm.car + 1}</span>}
              <span>{ALARM_STATE_TEXT[alarm.state]}</span>
              {alarm.state === 'active' && (
                <button
                  type="button"
                  aria-label={`Подтвердить тревогу ${alarm.id}`}
                  disabled={!enabled}
                  onClick={() => void acknowledge(alarm.id)}
                >
                  Подтвердить
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
      {state.alarmTotal > state.alarms.length && (
        <p>
          Показано {state.alarms.length} из {state.alarmTotal}
        </p>
      )}
      <div role="log" aria-live="polite" className="alarm-result">
        {message}
      </div>
    </section>
  )
}
