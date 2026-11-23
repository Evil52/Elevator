import type { PanelState } from '../transport/types.ts'
import { DOOR_TEXT, MODE_TEXT } from './text.ts'

const FLOORS = 10
const SHAFTS = 3
const TOP_MM = 27000

export function Building({ state }: { state: PanelState }) {
  const floors = Array.from({ length: FLOORS }, (_, index) => FLOORS - index)
  return (
    <section className="building" data-testid="building" aria-label="Схема здания">
      <ol className="floor-labels" aria-hidden="true">
        {floors.map((floor) => (
          <li key={floor} data-testid="floor-label">
            {floor}
          </li>
        ))}
      </ol>
      <ul className="shafts" aria-label="Шахты">
        {Array.from({ length: SHAFTS }, (_, index) => {
          const car = state.cars[index]
          return (
            <li
              key={index}
              className="shaft"
              aria-label={`Шахта ${index + 1}`}
              data-testid={`shaft-${index}`}
            >
              {car === undefined ? (
                <span className="shaft-empty">нет данных</span>
              ) : (
                <div className="shaft-track">
                  <div
                    className={`car car-${car.mode}`}
                    data-testid={`car-${index}`}
                    style={{
                      bottom: `${(car.positionMm / TOP_MM) * 100}%`,
                      transition: state.stale ? 'none' : 'bottom 0.9s linear',
                    }}
                  >
                    <strong>Кабина {index + 1}</strong>
                    <span>{car.floor === null ? 'между этажами' : `этаж ${car.floor}`}</span>
                    <span>{MODE_TEXT[car.mode]}</span>
                    <span>{DOOR_TEXT[car.doors]}</span>
                    {state.stale && <span className="car-stale">устарело</span>}
                  </div>
                </div>
              )}
            </li>
          )
        })}
      </ul>
    </section>
  )
}
