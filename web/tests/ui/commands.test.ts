import { describe, expect, it } from 'vitest'
import { blockText, canCommand, describeOutcome } from '../../src/ui/commands.ts'
import { freshState, staleState } from './fake.ts'

describe('можно ли отправлять команды', () => {
  it('только при открытой связи, свежих данных и совместимой версии', () => {
    expect(canCommand(freshState())).toBe(true)
    expect(canCommand(staleState())).toBe(false)
    expect(canCommand(freshState({ link: 'closed' }))).toBe(false)
    expect(canCommand(freshState({ link: 'connecting' }))).toBe(false)
    expect(canCommand(freshState({ incompatibleVersion: 3 }))).toBe(false)
    expect(canCommand(freshState({ identity: null }))).toBe(false)
  })
})

describe('текст результата команды', () => {
  const result = (status: never) => describeOutcome({ kind: 'result', requestId: 1, status })
  it('статусы по-русски', () => {
    expect(result('accepted' as never)).toBe('Вызов принят')
    expect(result('applied' as never)).toBe('Команда выполнена')
    expect(result('expired' as never)).toBe('Нет ответа от платы')
    expect(result('unknown' as never)).toBe('Результат неизвестен')
  })

  it('причины отказа по-русски', () => {
    const cases: [string, string][] = [
      ['noSuchFloor', 'такого этажа нет'],
      ['downFromFirst', 'с первого этажа вниз вызвать нельзя'],
      ['upFromTop', 'с верхнего этажа вверх вызвать нельзя'],
      ['wrongBoot', 'плата перезагружалась'],
      ['wrongRun', 'сценарий был сброшен'],
      ['noSuchAlarm', 'такой тревоги нет'],
      ['noSuchCar', 'такой кабины нет'],
      ['wrongState', 'сейчас так сделать нельзя'],
    ]
    for (const [code, text] of cases) {
      expect(result({ rejected: code } as never)).toContain(text)
      expect(result({ rejected: code } as never)).toContain('Отклонено')
    }
  })

  it('блокировка объясняется причиной', () => {
    const blocked = (reason: never) => describeOutcome({ kind: 'blocked', requestId: 1, reason })
    expect(blocked('stale' as never)).toContain('Команды недоступны')
    expect(blocked('stale' as never)).toContain('данные устарели')
    expect(blocked('closed' as never)).toContain('нет связи')
    expect(blocked('incompatible' as never)).toContain('версия протокола')
    expect(blocked('invalid' as never)).toContain('некорректна')
  })

  it('отправленный запрос без ожидания результата', () => {
    expect(describeOutcome({ kind: 'sent', requestId: 1 })).toBe('Запрос отправлен')
  })
})

describe('объяснение блокировки', () => {
  it('нет блокировки, когда команды разрешены', () => {
    expect(blockText(freshState())).toBeNull()
  })

  it('каждая причина названа отдельно, связь важнее версии, версия важнее свежести', () => {
    expect(blockText(staleState({ link: 'closed' }))).toBe('Команды недоступны: нет связи со шлюзом')
    expect(blockText(staleState({ incompatibleVersion: 2 }))).toBe(
      'Команды недоступны: несовместимая версия протокола',
    )
    expect(blockText(staleState())).toBe('Команды недоступны: данные устарели')
    expect(blockText(freshState({ link: 'connecting' }))).toBe('Команды недоступны: нет связи со шлюзом')
  })
})
