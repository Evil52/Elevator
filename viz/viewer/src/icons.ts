const paths: Record<string, string> = {
  grid: '<rect x="3" y="3" width="7" height="7" rx="1.3"/><rect x="14" y="3" width="7" height="7" rx="1.3"/><rect x="3" y="14" width="7" height="7" rx="1.3"/><rect x="14" y="14" width="7" height="7" rx="1.3"/>',
  list: '<path d="M8 6h13M8 12h10M8 18h13M3 6h.1M3 12h.1M3 18h.1"/>',
  cube: '<path d="m12 2 9 5v10l-9 5-9-5V7l9-5Zm-9 5 9 5 9-5M12 12v10M7.5 4.5l9 5"/>',
  route:
    '<circle cx="5" cy="5" r="2"/><circle cx="18" cy="18" r="2"/><path d="M5 7v6c0 3 2 5 5 5h6M10 5h9m-4-4 4 4-4 4"/>',
  'chevron-down': '<path d="m6 9 6 6 6-6"/>',
  right: '<path d="m9 5 7 7-7 7"/>',
  up: '<path d="M12 19V5m-6 6 6-6 6 6"/>',
  down: '<path d="M12 5v14m-6-6 6 6 6-6"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  minus: '<path d="M5 12h14"/>',
  fit: '<path d="M3 9V4h5m8 0h5v5m0 7v5h-5m-8 0H3v-5"/>',
  play: '<path fill="currentColor" stroke-width="1.3" d="m7 4 13 8-13 8z"/>',
  pause: '<path stroke-width="4" d="M8 5v14m8-14v14"/>',
  step: '<path d="m5 5 10 7-10 7zM19 5v14"/>',
  back: '<path d="m19 5-10 7 10 7zM5 5v14"/>',
  clock: '<circle cx="12" cy="12" r="8.5"/><path d="M12 7v5l3 2"/>',
  activity: '<path d="M2 12h4l3-8 5 16 3-9h5"/>',
  matrix: '<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 10h18M10 4v16"/>',
  calls: '<path d="M8 20V4m-4 4 4-4 4 4m4-4v16m-4-4 4 4 4-4"/>',
  info: '<circle cx="12" cy="12" r="9"/><path d="M12 11v6m0-10v.1"/>',
  help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.6 2.6 0 0 1 5 .9c0 1.8-2.5 2-2.5 4M12 17v.1"/>',
  close: '<path d="m6 6 12 12M18 6 6 18"/>',
  check: '<path d="m5 12 4 4L19 6"/>',
  alarm: '<path d="M10.3 4.9a2 2 0 0 1 3.4 0L22 19H2zM12 9v4m0 3v.1"/>',
  rewind: '<path d="M3 10a9 9 0 1 1 1 7M3 4v6h6"/>',
  lift: '<rect x="4" y="3" width="16" height="18" rx="2"/><path d="M7 18h10M8 9l2-2 2 2m-2-2v7m3-1 2 2 2-2m-2 2V8"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5zm-8 10 8 5 8-5m-16 5 8 4 8-4"/>',
  trend: '<path d="m3 6 6 6 4-3 7 8m-6 0h6v-6"/>',
}
export function icon(name: string) {
  return (
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">' +
    (paths[name] || '') +
    '</svg>'
  )
}
export function iconsIn(root: ParentNode) {
  root
    .querySelectorAll<HTMLElement>('[data-icon]')
    .forEach((el) => (el.innerHTML = icon(el.dataset.icon!)))
}
