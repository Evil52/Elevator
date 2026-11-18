import './style.css'
import demoUrl from '../../out/timeline.json?url'
import silUrl from '../../recordings/single-trip/timeline.json?url'
import { parseTimeline, carIds, type ModelEvent, type Status } from './timeline'
import { floorLabel, lastEventAt, stateAt, stepEvent, type Snapshot, type Cabin } from './player'
import { icon, iconsIn } from './icons'
import type { createScene } from './scene'

function el<T extends HTMLElement = HTMLElement>(id: string) {
  return document.getElementById(id) as T
}
const fmt = (value: number, digits: number) =>
  value.toLocaleString('ru-RU', { minimumFractionDigits: digits, maximumFractionDigits: digits })
const sec = (ms: number) =>
  (ms / 1000).toLocaleString('ru-RU', { minimumFractionDigits: 1, maximumFractionDigits: 3 })
const pad = (n: number) => String(n).padStart(2, '0')
const shortClock = (ms: number) =>
  pad(Math.floor(ms / 60000)) + ':' + pad(Math.floor(ms / 1000) % 60)
const modeNames: Record<Cabin['mode'], string> = {
  READY: 'Готова',
  MOVING: 'В движении',
  OPEN: 'Двери открыты',
  OPENING: 'Открывает двери',
  CLOSING: 'Закрывает двери',
  BRAKING: 'Аварийное торможение',
  LOCKED: 'Заблокирована',
}
const states: Record<Status, string> = {
  QUEUED: 'В очереди',
  ASSIGNED: 'Назначен',
  ARRIVED: 'Прибытие',
  SERVED: 'Обслужен',
}
function description(e: ModelEvent) {
  if (e.type === 'CALL')
    return e.floor + ' этаж · ' + states[e.state!] + (e.car ? ' · кабина ' + e.car : '')
  if (e.type === 'DEPART') return 'Кабина ' + e.car + ' направлена на ' + e.floor + ' этаж'
  if (e.type === 'DESTINATION')
    return 'Кабина ' + e.car + ' доставила пассажира на ' + e.floor + ' этаж'
  if (e.type === 'DOOR_CLOSING')
    return 'Кабина ' + e.car + ' закрывает двери · ' + e.floor + ' этаж'
  const titles: Record<string, string> = {
    START: 'Сценарий запущен',
    SAMPLE_ACCEL: 'Контрольная точка разгона',
    SAMPLE_CRUISE: 'Движение на постоянной скорости',
    SAMPLE_BRAKE: 'Контрольная точка торможения',
    END: 'Поездка завершена',
    DISPATCH: 'Диспетчер рассчитал матрицу ETA',
    ALARM: 'B1 · потеряно подтверждение замка',
    LOCKED: 'Кабина ' + e.car + ' остановлена и заблокирована',
    REQUEUE: 'Вызов ' + e.floor + ' этажа возвращён в очередь',
    DIAGNOSTICS: 'Диагностика блокировки кабины ' + e.car,
    RESET: 'Сценарий сброшен',
  }
  return Object.hasOwn(titles, e.type) ? titles[e.type] : e.type
}
iconsIn(document)
const dialog = el<HTMLDialogElement>('dialog')
el('close-dialog').onclick = () => dialog.close()
dialog.onclick = (e) => {
  if (e.target === dialog) {
    const r = dialog.getBoundingClientRect()
    if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom)
      dialog.close()
  }
}
function explain(title: string, body: string, symbol: string) {
  el('dialog-icon').innerHTML = icon(symbol)
  el('dialog-content').innerHTML = '<h2>' + title + '</h2>' + body
  dialog.showModal()
}
async function start() {
  const singleTrip = new URLSearchParams(location.search).get('scenario') === 'sil'
  const response = await fetch(singleTrip ? silUrl : demoUrl)
  if (!response.ok) throw new Error('Не удалось загрузить timeline.json')
  const line = parseTimeline(await response.json())
  const demo = line.source === 'python-demo'
  const title = singleTrip ? 'Поездка с 1-го на 4-й этаж' : 'Отказ замка двери'
  const summary = singleTrip
    ? 'Кабина A едет с 1-го на 4-й этаж. Траекторию и ETA рассчитывает Rust-ядро.'
    : 'Два одновременных вызова, три кабины и аварийное переназначение.'
  const clock = (ms: number) =>
    singleTrip
      ? shortClock(ms) + ',' + String(Math.floor(ms) % 1000).padStart(3, '0')
      : shortClock(ms)
  el('scenario-title').textContent = title
  el('scenario-summary').textContent = summary
  el('scenario-number').textContent = singleTrip ? 'СЦЕНАРИЙ 02 · SIL' : 'СЦЕНАРИЙ 01'
  el('scenario-info').setAttribute('aria-label', 'Выбор сценария. ' + title)
  el('matrix-source').textContent = demo
    ? 'Демо Python · приближённая ETA'
    : line.source === 'rust-sil'
      ? 'Источник: Rust SIL'
      : line.source
        ? 'Источник: ' + line.source
        : 'Источник записи не указан'
  let time = 0,
    index = lastEventAt(line, 0),
    playing = false,
    rate = 1,
    previousStamp = 0,
    lastPanel = -1
  let view: Awaited<ReturnType<typeof createScene>> | undefined
  const play = el<HTMLButtonElement>('play'),
    scrub = el<HTMLInputElement>('scrub')
  const alarmIndex = line.events.findIndex((e) => e.type === 'ALARM')
  const phases = [
    { name: 'Назначение', index: line.events.findIndex((e) => e.type === 'DISPATCH') },
    { name: 'Разгон', index: line.events.findIndex((e) => e.type === 'SAMPLE_ACCEL') },
    { name: 'Полный ход', index: line.events.findIndex((e) => e.type === 'SAMPLE_CRUISE') },
    { name: 'Торможение', index: line.events.findIndex((e) => e.type === 'SAMPLE_BRAKE') },
    {
      name: 'Остановка',
      index: singleTrip ? line.events.findIndex((e) => e.state === 'ARRIVED') : -1,
    },
    { name: 'Авария B1', index: alarmIndex },
    { name: 'Обслуживание', index: line.events.findIndex((e) => e.state === 'SERVED') },
    { name: 'Диагностика', index: line.events.findIndex((e) => e.type === 'DIAGNOSTICS') },
  ].filter((phase) => phase.index >= 0)
  const phaseButtons = phases.map((phase) => {
    const button = document.createElement('button')
    button.dataset.go = String(phase.index)
    button.textContent = phase.name
    return button
  })
  el('chapters').replaceChildren(...phaseButtons)
  el('total').textContent = clock(line.durationMs)
  scrub.max = String(line.durationMs)
  const marks = [...new Set(line.events.map((e) => e.tMs))].map((t) => {
    const mark = document.createElement('b')
    mark.className = t === line.events[alarmIndex]?.tMs ? 'tick alarm' : 'tick'
    mark.style.left = (100 * t) / line.durationMs + '%'
    return mark
  })
  el('event-marks').replaceChildren(...marks)
  el('cars').innerHTML = carIds
    .map(
      (id) =>
        '<div class="car-row"><span class="car-square ' +
        id +
        '">' +
        id +
        '</span><div><span class="car-name">Кабина ' +
        id +
        '</span><span class="car-state" id="mode-' +
        id +
        '"></span></div><div class="car-pos"><strong><span id="floor-' +
        id +
        '"></span> <small>эт.</small></strong><span id="speed-' +
        id +
        '"></span></div></div>',
    )
    .join('')

  let strategy: 'optimal' | 'greedy' = 'optimal'
  function matrix(s: Snapshot) {
    const d = s.dispatch
    if (!d) return '<p class="empty">Ожидание расчёта диспетчера.</p>'
    const heading = d.columns
      .map((id) => {
        const c = line.calls.find((c) => c.id === id)!
        return (
          '<th scope="col">' + c.floor + ' этаж ' + (c.direction === 'UP' ? '↑' : '↓') + '</th>'
        )
      })
      .join('')
    const rows = d.rows
      .map(
        (id, r) =>
          '<tr><th scope="row"><span class="row-id"><b class="dot-id ' +
          id +
          '"></b>' +
          id +
          '</span></th>' +
          d.matrix[r]
            .map((ms, c) => {
              const selected = d[strategy].pairs.some((p) => p.car === id && p.column === c)
              if (ms === null || !d.allowed[r][c])
                return '<td><span class="eta-value blocked" aria-label="Назначение запрещено" title="Назначение запрещено">—</span></td>'
              return (
                '<td><span class="eta-value ' +
                (selected ? 'selected' : '') +
                '" ' +
                (selected ? 'title="Выбрано в этом плане"' : '') +
                '>' +
                (selected ? icon('check') : '') +
                sec(ms) +
                ' с</span></td>'
              )
            })
            .join('') +
          '</tr>',
      )
      .join('')
    const diff = d.greedy.totalEtaMs - d.optimal.totalEtaMs
    const sameCount = d.greedy.pairs.length === d.optimal.pairs.length
    const comparisons = (['optimal', 'greedy'] as const)
      .map((key) => {
        const plan = d[key]
        const name = key === 'optimal' ? 'Оптимум' : 'Жадный выбор'
        const value = plan.pairs.length ? sec(plan.totalEtaMs) + ' <small>с</small>' : '—'
        return (
          '<button data-plan="' +
          key +
          '" aria-pressed="' +
          (strategy === key) +
          '" title="Показать выбранные пары">' +
          '<span>' +
          name +
          ' · Σ ETA</span><strong>' +
          value +
          '</strong></button>'
        )
      })
      .join('')
    const note = !sameCount
      ? 'Назначено пар: оптимум ' + d.optimal.pairs.length + ', жадный ' + d.greedy.pairs.length
      : diff > 0
        ? 'Выигрыш по ETA — ' + sec(diff) + ' с'
        : 'Суммы ETA совпадают'
    return (
      '<table class="eta-table"><thead><tr><th scope="col">Кабина</th>' +
      heading +
      '</tr></thead><tbody>' +
      rows +
      '</tbody></table><div class="comparison" aria-label="Сравнить выбранные пары">' +
      comparisons +
      '</div><p class="matrix-note">' +
      icon(diff > 0 ? 'trend' : 'check') +
      note +
      '</p>'
    )
  }
  el('matrix').onclick = (e) => {
    const button = (e.target as HTMLElement).closest<HTMLButtonElement>('[data-plan]')
    if (!button) return
    strategy = button.dataset.plan === 'greedy' ? 'greedy' : 'optimal'
    el('matrix').innerHTML = matrix(stateAt(line, time, index))
    el('matrix')
      .querySelector<HTMLButtonElement>('[data-plan="' + strategy + '"]')
      ?.focus({ preventScroll: true })
  }
  function calls(s: Snapshot) {
    const active = line.calls.filter((c) => s.calls[c.id])
    el('call-count').textContent =
      active.length +
      ' ' +
      (active.length === 1 ? 'вызов' : active.length === 0 ? 'вызовов' : 'вызова')
    if (!active.length) return '<p class="empty">Зарегистрированных вызовов пока нет.</p>'
    return (
      '<table class="queue-table"><thead><tr><th>Этаж</th><th>Кабина</th><th>Состояние</th><th>Регистрация</th></tr></thead><tbody>' +
      active
        .map((c) => {
          const v = s.calls[c.id]
          return (
            '<tr><td><span class="floor-call">' +
            icon(c.direction === 'UP' ? 'up' : 'down') +
            c.floor +
            ' этаж</span></td><td><span class="row-id">' +
            (v.car ? '<b class="dot-id ' + v.car + '"></b>' + v.car : '—') +
            '</span></td><td><span class="state-chip ' +
            v.state +
            '" title="' +
            states[v.state] +
            '">' +
            v.state +
            '</span></td><td><span class="mono">' +
            clock(v.registeredMs) +
            '</span></td></tr>'
          )
        })
        .join('') +
      '</tbody></table>'
    )
  }
  function render() {
    const s = stateAt(line, time, index)
    el('time').textContent = clock(time)
    scrub.value = String(time)
    scrub.setAttribute('aria-valuetext', clock(time) + ' · ' + description(line.events[index]))
    scrub.parentElement!.style.setProperty('--progress', (100 * time) / line.durationMs + '%')
    for (const id of carIds) {
      const car = s.cars[id],
        mode = el('mode-' + id)
      mode.textContent = singleTrip && id !== 'A' ? 'В резерве' : modeNames[car.mode]
      mode.classList.toggle('fault', s.alarm === id)
      el('floor-' + id).textContent = floorLabel(car.positionMm)
      el('floor-' + id).parentElement!.title = car.positionMm + ' мм'
      el('speed-' + id).textContent =
        (car.speed === 0 ? '' : car.speed > 0 ? '↑ ' : '↓ ') +
        fmt(Math.abs(car.speed) / 1000, 1) +
        ' м/с' +
        (singleTrip && id === 'A' ? ' · ' + car.positionMm + ' мм' : '')
    }
    if (index !== lastPanel) {
      el('matrix').innerHTML = matrix(s)
      el('calls').innerHTML = calls(s)
      const logRows = line.events.slice(0, index + 1).map((event, i) => {
        const button = document.createElement('button')
        button.className = 'log-row'
        button.dataset.log = String(i)
        const time = document.createElement('time')
        time.textContent = clock(event.tMs)
        const dot = document.createElement('span')
        dot.className = 'log-dot'
        if (['ALARM', 'LOCKED', 'DISPATCH'].includes(event.type)) dot.classList.add(event.type)
        button.append(time, dot, description(event))
        return button
      })
      el('log').replaceChildren(...logRows.reverse())
      el('journal-count').textContent = index + 1 + ' / ' + line.events.length
      const chapter = phases.filter((p) => p.index <= index).at(-1)
      el('chapters')
        .querySelectorAll<HTMLButtonElement>('button')
        .forEach((b) => b.classList.toggle('active', Number(b.dataset.go) === chapter?.index))
      const a = el('alert')
      a.hidden = s.alarm === null
      if (s.alarm)
        a.innerHTML =
          icon('alarm') +
          'B1 · ' +
          (s.cars[s.alarm].mode === 'LOCKED'
            ? 'Кабина ' + s.alarm + ' заблокирована'
            : 'Потеря замка · кабина ' + s.alarm)
      el<HTMLButtonElement>('prev').disabled = index === 0
      el<HTMLButtonElement>('next').disabled = index === line.events.length - 1
      lastPanel = index
    }
    view?.show(s)
  }
  let frameId = 0
  function setPlaying(next: boolean) {
    playing = next
    previousStamp = 0
    cancelAnimationFrame(frameId)
    play.innerHTML =
      icon(playing ? 'pause' : time === line.durationMs ? 'rewind' : 'play') +
      '<span>' +
      (playing
        ? 'Пауза'
        : time === line.durationMs
          ? 'Повторить'
          : time
            ? 'Продолжить'
            : 'Запустить') +
      '</span>'
    if (playing) frameId = requestAnimationFrame(frame)
  }
  let exactTime = 0
  function frame(stamp: number) {
    if (!playing) return
    if (previousStamp)
      exactTime = Math.min(line.durationMs, exactTime + (stamp - previousStamp) * rate)
    previousStamp = stamp
    time = Math.floor(exactTime)
    index = lastEventAt(line, time)
    render()
    if (time === line.durationMs) setPlaying(false)
    else frameId = requestAnimationFrame(frame)
  }
  function seek(i: number) {
    index = stepEvent(line, i, 0)
    time = line.events[index].tMs
    exactTime = time
    setPlaying(false)
    render()
  }
  function toggle() {
    if (time === line.durationMs) seek(lastEventAt(line, 0))
    setPlaying(!playing)
  }
  play.onclick = toggle
  el('prev').onclick = () => seek(stepEvent(line, index, -1))
  el('next').onclick = () => seek(stepEvent(line, index, 1))
  el<HTMLSelectElement>('speed').onchange = (e) => {
    rate = Number((e.target as HTMLSelectElement).value)
  }
  scrub.oninput = () => {
    const requested = Number(scrub.value)
    const nearest = line.events.reduce((a, b) =>
      Math.abs(b.tMs - requested) < Math.abs(a.tMs - requested) ? b : a,
    )
    seek(lastEventAt(line, nearest.tMs))
  }
  scrub.onkeydown = (e) => {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(e.key)) return
    e.preventDefault()
    seek(
      e.key === 'Home'
        ? 0
        : e.key === 'End'
          ? line.events.length - 1
          : stepEvent(line, index, e.key === 'ArrowRight' ? 1 : -1),
    )
  }
  document.addEventListener('click', (e) => {
    const button = (e.target as HTMLElement).closest<HTMLElement>('[data-go],[data-log]')
    if (!button) return
    if (dialog.open) dialog.close()
    if (button.dataset.go !== undefined)
      seek(lastEventAt(line, line.events[Number(button.dataset.go)].tMs))
    else seek(Number(button.dataset.log))
  })
  document.addEventListener('keydown', (e) => {
    if (
      dialog.open ||
      e.altKey ||
      e.metaKey ||
      e.ctrlKey ||
      (e.target as HTMLElement).closest('button,input,select,a')
    )
      return
    if (e.code === 'Space') {
      e.preventDefault()
      toggle()
    } else if (e.key === 'ArrowRight' || e.key === 'ArrowLeft') {
      e.preventDefault()
      seek(stepEvent(line, index, e.key === 'ArrowRight' ? 1 : -1))
    }
  })
  document.addEventListener('visibilitychange', () => {
    if (document.hidden && playing) setPlaying(false)
  })
  el('help').onclick = () =>
    explain(
      'Управление симулятором',
      '<p>Модель показывает работу группы из трёх лифтов. Цвет и буква связывают кабину в разрезе с её телеметрией и строкой матрицы.</p><ul><li>Пробел запускает и приостанавливает показ.</li><li>Стрелки переключают события, включая одновременные.</li><li>Потяните модель, чтобы повернуть её. Кнопки + и − меняют масштаб.</li></ul>',
      'cube',
    )
  el('matrix-info').onclick = () =>
    explain(
      'Как считается ETA',
      (demo
        ? '<p>В демо Python время до остановки рассчитано по прямому пути с примерным ускорением и остатком цикла дверей. Контрольный сценарий Rust использует собственную модель движения и остановок.</p>'
        : '<p>Время до остановки, допустимые пары и назначения взяты из загруженной записи.</p>') +
        '<p>Диспетчер сначала назначает максимум допустимых пар, затем минимизирует сумму ETA. Нажмите итог «Оптимум» или «Жадный выбор», чтобы увидеть соответствующие пары с галочками. Прочерк означает запрет назначения.</p><p>Для сравнения жадный алгоритм обрабатывает вызовы по порядку регистрации. Каждый раз он выбирает ближайшую по расстоянию свободную кабину с завершённым циклом дверей.</p>',
      'matrix',
    )
  el('scenario-info').onclick = () =>
    explain(
      title,
      '<p>' +
        (singleTrip
          ? 'Кабина A начинает движение из покоя. Выберите контрольную точку, чтобы увидеть координату и скорость из записи. ETA заканчивается в момент точной остановки перед открытием дверей.'
          : 'Одновременно поступают вызовы с 3-го этажа вниз и с 8-го вверх. В пути кабина A теряет подтверждение замка. Диспетчер передаёт вызов другой кабине.') +
        '</p><div class="jump-list">' +
        phases
          .map(
            (p) =>
              '<button data-go="' +
              p.index +
              '">' +
              p.name +
              '<time>' +
              clock(line.events[p.index].tMs) +
              '</time></button>',
          )
          .join('') +
        '</div><a class="switch-recording" href="' +
        (singleTrip ? '?scenario=demo' : '?scenario=sil') +
        '">' +
        (singleTrip ? 'Открыть демо с отказом B1' : 'Открыть поездку Rust SIL') +
        ' →</a>',
      'route',
    )
  function projection(front: boolean) {
    el('front').setAttribute('aria-pressed', String(front))
    el('iso').setAttribute('aria-pressed', String(!front))
  }
  function setZoom(delta: number) {
    if (!view) return
    const z = view.zoom(delta)
    el<HTMLButtonElement>('zoom-in').disabled = z >= 1.5
    el<HTMLButtonElement>('zoom-out').disabled = z <= 0.75
  }
  el('iso').onclick = () => view?.project(false)
  el('front').onclick = () => view?.project(true)
  el('zoom-in').onclick = () => setZoom(0.125)
  el('zoom-out').onclick = () => setZoom(-0.125)
  el('fit').onclick = () => {
    view?.fit()
    setZoom(0)
  }
  function nav() {
    const active = window.location.hash === '#journal' ? 'journal' : 'simulation'
    document.querySelectorAll<HTMLElement>('[data-nav]').forEach((a) => {
      a.classList.toggle('current', a.dataset.nav === active)
      if (a.dataset.nav === active) a.setAttribute('aria-current', 'location')
      else a.removeAttribute('aria-current')
    })
  }
  nav()
  window.addEventListener('hashchange', nav)
  render()
  try {
    const { createScene } = await import('./scene')
    view = await createScene(el('scene'), el('labels'), projection)
    render()
  } catch (error) {
    el('scene').textContent = 'Модель здания не загрузилась.'
    throw error
  }
}
start().catch((error) => {
  console.error(error)
  const loader = el('scene-loading')
  if (loader) loader.textContent = 'Ошибка загрузки сценария. Обновите страницу.'
})
