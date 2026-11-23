import type { CommandOutcome, PanelState } from '../transport/types.ts'
import { REJECTION_TEXT } from './text.ts'

export function canCommand(state: PanelState): boolean {
  return (
    state.link === 'open' &&
    state.fresh &&
    state.incompatibleVersion === null &&
    state.identity !== null
  )
}

export function blockText(state: PanelState): string | null {
  if (canCommand(state)) return null
  if (state.link !== 'open') return 'Команды недоступны: нет связи со шлюзом'
  if (state.incompatibleVersion !== null) {
    return 'Команды недоступны: несовместимая версия протокола'
  }
  return 'Команды недоступны: данные устарели'
}

const STATUS_TEXT: Record<string, string> = {
  accepted: 'Вызов принят',
  applied: 'Команда выполнена',
  expired: 'Нет ответа от платы',
  unknown: 'Результат неизвестен',
}

const BLOCK_TEXT: Record<string, string> = {
  stale: 'Команды недоступны: данные устарели',
  closed: 'Команды недоступны: нет связи со шлюзом',
  incompatible: 'Команды недоступны: несовместимая версия протокола',
  invalid: 'Команда некорректна и не отправлена',
}

export function describeOutcome(outcome: CommandOutcome): string {
  if (outcome.kind === 'sent') return 'Запрос отправлен'
  if (outcome.kind === 'blocked') return BLOCK_TEXT[outcome.reason] ?? 'Команда не отправлена'
  const status = outcome.status
  if (typeof status === 'string') return STATUS_TEXT[status] ?? 'Результат неизвестен'
  const reason = REJECTION_TEXT[status.rejected] ?? status.rejected
  return `Отклонено: ${reason}`
}
