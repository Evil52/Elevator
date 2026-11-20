import { Ajv2020 } from 'ajv/dist/2020.js'
import schema from '../../schema.json' with { type: 'json' }
import type { Envelope } from './protocol.ts'

export * from './protocol.ts'

// Числовые границы заданы minimum/maximum. Имена форматов указывают тип Rust.
const ajv = new Ajv2020({
  formats: { uint8: true, uint16: true, uint32: true, int32: true, uint64: true },
})
const matchesSchema = ajv.compile<Envelope>(schema)

export function isEnvelope(value: unknown): value is Envelope {
  if (!matchesSchema(value)) return false
  const message = value.message
  if ('alarmPage' in message) {
    const { items, offset, total } = message.alarmPage
    const count = items.filter((item) => item !== null).length
    return (
      offset + count <= total &&
      (count > 0 || offset === total) &&
      items.slice(0, count).every((item) => item !== null)
    )
  }
  if ('eventLogChunk' in message) {
    const { firstIndex, events } = message.eventLogChunk
    let padded = false
    for (const [slot, event] of events.entries()) {
      if (event === null) padded = true
      else if (padded || event.index !== firstIndex + slot) return false
    }
  }
  return true
}
