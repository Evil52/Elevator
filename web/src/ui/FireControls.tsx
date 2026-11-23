import { useState } from 'react'
import type { PanelState, SimActionName, SimTransport } from '../transport/types.ts'
import { blockText, canCommand, describeOutcome } from './commands.ts'

export function FireControls({ state, transport }: { state: PanelState; transport: SimTransport }) {
  const [confirming, setConfirming] = useState(false)
  const [pending, setPending] = useState<SimActionName | null>(null)
  const [message, setMessage] = useState('')
  const enabled = canCommand(state) && pending === null
  const fireActive = state.alarms.some((alarm) => alarm.kind === 'fireRecall' && alarm.state !== 'cleared')
  const blocked = blockText(state)

  async function send(action: SimActionName) {
    setPending(action)
    setConfirming(false)
    const outcome = await transport.simControl(action)
    setMessage(describeOutcome(outcome))
    setPending(null)
  }

  return (
    <section className="fire-controls" aria-label="Управление пожарным режимом">
      <h2>Пожарный режим</h2>
      <p>{fireActive ? 'Пожарный возврат активен' : 'Пожарный возврат не активен'}</p>
      {blocked !== null && <p className="blocked">{blocked}</p>}
      <div className="fire-actions">
        {!fireActive && (
          <button type="button" disabled={!enabled} onClick={() => setConfirming(true)}>
            Включить пожарный возврат
          </button>
        )}
        {fireActive && (
          <button type="button" disabled={!enabled} onClick={() => void send('clearFire')}>
            {pending === 'clearFire' ? 'Снятие режима…' : 'Снять пожарный режим'}
          </button>
        )}
      </div>
      {confirming && (
        <div role="alertdialog" aria-modal="true" aria-labelledby="fire-confirm-title" className="fire-confirm">
          <h3 id="fire-confirm-title">Подтвердить пожарный возврат?</h3>
          <p>Все кабины вернутся на первый этаж. Текущие вызовы будут отменены.</p>
          <button type="button" disabled={!enabled} onClick={() => void send('fireRecall')}>
            {pending === 'fireRecall' ? 'Включение…' : 'Подтвердить пожарный возврат'}
          </button>
          <button type="button" disabled={pending !== null} onClick={() => setConfirming(false)}>
            Отмена
          </button>
        </div>
      )}
      <div role="log" aria-live="polite" className="fire-result">
        {message}
      </div>
    </section>
  )
}
