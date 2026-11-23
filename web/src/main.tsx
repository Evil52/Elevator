import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { App } from './App.tsx'
import { loadConfig } from './config.ts'
import { WsTransport } from './transport/ws.ts'

async function start() {
  const root = document.getElementById('root')
  if (root === null) return
  const config = await loadConfig((url) => fetch(url))
  const transport = new WsTransport(config.gatewayUrl)
  transport.connect()
  createRoot(root).render(
    <StrictMode>
      <App transport={transport} />
    </StrictMode>,
  )
}

void start()
