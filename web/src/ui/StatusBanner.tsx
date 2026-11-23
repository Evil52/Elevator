import type { PanelState } from '../transport/types.ts'

type Tone = 'ok' | 'warn' | 'error'

function banner(state: PanelState): { tone: Tone; icon: string; text: string; alert: boolean } {
  const hasData = state.identity !== null || Object.keys(state.cars).length > 0
  if (state.link === 'closed') {
    return {
      tone: 'error',
      icon: '✕',
      text: hasData
        ? 'Нет связи со шлюзом. Показан последний кадр. Данные устарели.'
        : 'Нет связи со шлюзом.',
      alert: true,
    }
  }
  if (state.link === 'connecting') {
    if (state.identity === null) {
      return { tone: 'warn', icon: '…', text: 'Подключение к шлюзу…', alert: false }
    }
    return {
      tone: 'warn',
      icon: '⚠',
      text: 'Подключение к шлюзу… Данные устарели.',
      alert: true,
    }
  }
  if (state.identity === null && Object.keys(state.cars).length === 0) {
    return { tone: 'warn', icon: '…', text: 'Ожидание данных платы…', alert: false }
  }
  if (state.stale) {
    return { tone: 'warn', icon: '⚠', text: 'Данные устарели: плата не отвечает.', alert: true }
  }
  return { tone: 'ok', icon: '✓', text: 'Данные свежие', alert: false }
}

export function StatusBanner({ state }: { state: PanelState }) {
  const main = banner(state)
  return (
    <header className="status-banner">
      <div role={main.alert ? 'alert' : 'status'} className={`banner banner-${main.tone}`}>
        <span aria-hidden="true">{main.icon}</span> {main.text}
      </div>
      {state.incompatibleVersion !== null && (
        <div role="alert" className="banner banner-error">
          <span aria-hidden="true">⚠</span> Несовместимая версия протокола:{' '}
          {state.incompatibleVersion}
        </div>
      )}
      {state.invalidMessages > 0 && (
        <p className="banner-note">Отброшено некорректных сообщений: {state.invalidMessages}</p>
      )}
    </header>
  )
}
