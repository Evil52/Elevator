import * as THREE from 'three'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { RoomEnvironment } from 'three/examples/jsm/environments/RoomEnvironment.js'
import { SVGRenderer } from 'three/examples/jsm/renderers/SVGRenderer.js'
import sceneUrl from '../../out/scene.glb?url'
import { carIds } from './timeline'
import { floorLabel, type Snapshot } from './player'

const centreX = { A: -4.4, B: 0, C: 4.4 }
const initialAngle = 0.49
export async function createScene(
  host: HTMLElement,
  labels: HTMLElement,
  onAngle: (front: boolean) => void,
) {
  const gltf = await new GLTFLoader().loadAsync(sceneUrl)
  const scene = new THREE.Scene()
  let renderer: THREE.WebGLRenderer | SVGRenderer
  try {
    const gpu = new THREE.WebGLRenderer({
      alpha: true,
      antialias: true,
      powerPreference: 'low-power',
    })
    gpu.setPixelRatio(Math.min(window.devicePixelRatio, 2))
    gpu.setClearColor(0, 0)
    gpu.toneMapping = THREE.ACESFilmicToneMapping
    gpu.toneMappingExposure = 1.15
    const room = new RoomEnvironment()
    const generator = new THREE.PMREMGenerator(gpu)
    const env = generator.fromScene(room, 0.04)
    scene.environment = env.texture
    scene.environmentIntensity = 0.8
    room.dispose()
    generator.dispose()
    renderer = gpu
  } catch {
    // При отсутствии WebGL сохраняем интерактивную проекцию той же геометрии.
    renderer = new SVGRenderer()
    renderer.setPrecision(1)
    renderer.setQuality('low')
  }
  host.replaceChildren(renderer.domElement)
  renderer.domElement.setAttribute('role', 'img')
  renderer.domElement.setAttribute('aria-label', 'Анимированный разрез здания с кабинами A, B и C')
  scene.add(gltf.scene)
  scene.add(new THREE.HemisphereLight(0xf5fff5, 0x8e9b91, 1.7))
  const key = new THREE.DirectionalLight(0xffefd4, 2.15)
  key.position.set(-10, 35, 24)
  scene.add(key)
  const rim = new THREE.DirectionalLight(0xc9dfff, 1.55)
  rim.position.set(22, 23, 8)
  scene.add(rim)
  const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0.1, 200)
  const nodes = new Map<string, THREE.Object3D>()
  gltf.scene.traverse((n) => nodes.set(n.name, n))
  function node(name: string) {
    const obj = nodes.get(name)
    if (!obj) throw new Error('В GLB отсутствует узел ' + name)
    return obj
  }
  const elements = carIds.map((id) => {
    const pin = document.createElement('div')
    pin.className = 'pin' + (id === 'A' ? ' left' : '')
    pin.dataset.car = id
    pin.innerHTML = '<strong>' + id + '</strong><span></span>'
    labels.append(pin)
    return {
      id,
      pin,
      text: pin.lastElementChild!,
      car: node('Car_' + id),
      lamp: node('Lamp_' + id),
      doors: ['L', 'R'].map((side) => node('Door_' + id + '_' + side)),
      landings: Array.from({ length: 10 }, (_, f) =>
        ['L', 'R'].map((side) =>
          node('Landing_' + id + '_' + String(f + 1).padStart(2, '0') + '_' + side),
        ),
      ),
    }
  })
  let width = 1,
    height = 1,
    angle = initialAngle,
    tilt = 0.22,
    zoom = 1
  let state: Snapshot | undefined
  const target = new THREE.Vector3(0, 15.7, 0),
    anchor = new THREE.Vector3()
  function draw() {
    const aspect = width / height
    const span = Math.max(37.7, 23 / aspect) / zoom
    camera.left = (-span * aspect) / 2
    camera.right = -camera.left
    camera.top = span / 2
    camera.bottom = -camera.top
    camera.position.set(65 * Math.sin(angle), target.y + 65 * Math.tan(tilt), 65 * Math.cos(angle))
    camera.lookAt(target)
    camera.updateProjectionMatrix()
    camera.updateMatrixWorld()
    renderer.render(scene, camera)
    if (renderer instanceof SVGRenderer) renderer.domElement.style.backgroundColor = 'transparent'
    if (!state) return
    for (const { id, pin, text } of elements) {
      const car = state.cars[id],
        left = id === 'A',
        sign = left ? -1 : 1
      anchor.set(centreX[id] + sign * 1.6, car.positionMm / 1000 + 1.4, 1.5).project(camera)
      const x = ((anchor.x + 1) * width) / 2,
        y = ((1 - anchor.y) * height) / 2
      const leader = Math.max(10, Math.min(width * 0.075, 64, left ? x - 61 : width - x - 61))
      pin.style.left = x + sign * leader + 'px'
      pin.style.top = y + 'px'
      pin.style.setProperty('--leader', leader + 'px')
      pin.hidden = x < 5 || x > width - 5 || y < 6 || y > height - 6
      pin.classList.toggle('fault', state.alarm === id)
      text.textContent = floorLabel(car.positionMm) + ' эт.'
    }
  }
  function resize() {
    width = host.clientWidth
    height = host.clientHeight
    renderer.setSize(width, height)
    draw()
  }
  const observer = new ResizeObserver(resize)
  observer.observe(host)
  let drag: { x: number; y: number; angle: number; tilt: number } | null = null
  host.onpointerdown = (e) => {
    if (e.button !== 0) return
    drag = { x: e.clientX, y: e.clientY, angle, tilt }
    host.setPointerCapture(e.pointerId)
  }
  host.onpointermove = (e) => {
    if (!drag) return
    angle = THREE.MathUtils.clamp(drag.angle - (e.clientX - drag.x) * 0.006, -1.18, 1.18)
    tilt = THREE.MathUtils.clamp(drag.tilt + (e.clientY - drag.y) * 0.002, -0.05, 0.53)
    onAngle(Math.abs(angle) < 0.025 && Math.abs(tilt) < 0.025)
    draw()
  }
  host.onpointerup = host.onpointercancel = () => {
    drag = null
  }
  resize()
  return {
    show(next: Snapshot) {
      state = next
      for (const { id, car, lamp, doors, landings } of elements) {
        const snap = next.cars[id],
          open = snap.doorPermille / 1000
        car.position.y = snap.positionMm / 1000
        lamp.scale.setScalar(next.alarm === id ? 1 : 0.00001)
        doors.forEach((d, i) => (d.position.x = (i ? 1 : -1) * (0.523 + 1.05 * open)))
        landings.forEach((pair, f) =>
          pair.forEach((d, i) => {
            d.position.x =
              centreX[id] + (i ? 1 : -1) * (0.526 + (f + 1 === snap.doorFloor ? 1.05 * open : 0))
          }),
        )
      }
      draw()
    },
    project(front: boolean) {
      angle = front ? 0 : initialAngle
      tilt = front ? 0 : 0.22
      onAngle(front)
      draw()
    },
    zoom(delta: number) {
      zoom = THREE.MathUtils.clamp(zoom + delta, 0.75, 1.5)
      draw()
      return zoom
    },
    fit() {
      zoom = 1
      angle = initialAngle
      tilt = 0.22
      onAngle(false)
      draw()
    },
  }
}
