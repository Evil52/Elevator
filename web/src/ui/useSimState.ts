import { useSyncExternalStore } from 'react'
import type { PanelState, SimTransport } from '../transport/types.ts'

export function useSimState(transport: SimTransport): PanelState {
  return useSyncExternalStore(
    (listener) => transport.subscribe(listener),
    () => transport.getState(),
  )
}
