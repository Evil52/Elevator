import type { SimTransport } from './transport/types.ts'
import { AlarmList } from './ui/AlarmList.tsx'
import { Building } from './ui/Building.tsx'
import { EventLogView } from './ui/EventLogView.tsx'
import { HallCalls } from './ui/HallCalls.tsx'
import { StatusBanner } from './ui/StatusBanner.tsx'
import { useSimState } from './ui/useSimState.ts'
import './ui/panel.css'

export function App({ transport }: { transport: SimTransport }) {
  const state = useSimState(transport)
  return (
    <main className="panel">
      <h1>Панель диспетчера</h1>
      <StatusBanner state={state} />
      <div className="panel-grid">
        <Building state={state} />
        <HallCalls state={state} transport={transport} />
        <AlarmList state={state} transport={transport} />
        <EventLogView state={state} transport={transport} />
      </div>
    </main>
  )
}
