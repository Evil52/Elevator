import type { Alarm, CarSnapshot, Event } from '@elevator/protocol-v1'

export const MODE_TEXT: Record<CarSnapshot['mode'], string> = {
  normal: 'Работает',
  fireRecall: 'Пожарная опасность',
  firefighters: 'Перевозка пожарных',
  unavailable: 'Недоступна',
}

export const DOOR_TEXT: Record<CarSnapshot['doors'], string> = {
  closed: 'двери закрыты',
  opening: 'двери открываются',
  open: 'двери открыты',
  closing: 'двери закрываются',
}

export const ALARM_KIND_TEXT: Record<Alarm['kind'], string> = {
  fireRecall: 'Пожарная опасность',
  carFault: 'Отказ кабины',
  linkLost: 'Потеряна связь',
  logOverflow: 'Переполнение журнала',
}

export const ALARM_STATE_TEXT: Record<Alarm['state'], string> = {
  active: 'Активна',
  acknowledged: 'Подтверждена',
  cleared: 'Снята',
}

export const REJECTION_TEXT: Record<string, string> = {
  noSuchFloor: 'такого этажа нет',
  downFromFirst: 'с первого этажа вниз вызвать нельзя',
  upFromTop: 'с верхнего этажа вверх вызвать нельзя',
  wrongBoot: 'плата перезагружалась, обновите данные',
  wrongRun: 'сценарий был сброшен, обновите данные',
  noSuchAlarm: 'такой тревоги нет',
  noSuchCar: 'такой кабины нет',
  wrongState: 'сейчас так сделать нельзя',
}

export const EVENT_KIND_TEXT: Record<Event['kind'], string> = {
  assigned: 'Назначен вызов',
  arrived: 'Кабина прибыла',
  served: 'Вызов обслужен',
  requeued: 'Вызов возвращён в очередь',
  cancelled: 'Вызов отменён',
  alarmRaised: 'Тревога поднята',
  alarmAcked: 'Тревога подтверждена',
  alarmCleared: 'Тревога снята',
}

export function formatSeconds(milliseconds: number): string {
  return `${(milliseconds / 1000).toFixed(1).replace('.', ',')} с`
}
