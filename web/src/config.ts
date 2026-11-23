export const DEFAULT_GATEWAY_URL = 'ws://127.0.0.1:8787'

export interface PanelConfig {
  gatewayUrl: string
}

export function parseConfig(text: string): PanelConfig {
  const value: unknown = JSON.parse(text)
  if (typeof value !== 'object' || value === null || Array.isArray(value)) {
    throw new Error('config.json must be an object')
  }
  const url = (value as { gatewayUrl?: unknown }).gatewayUrl
  if (typeof url !== 'string' || !/^wss?:\/\/\S+$/.test(url)) {
    throw new Error('gatewayUrl must be a ws:// or wss:// address')
  }
  return { gatewayUrl: url }
}

type Fetcher = (url: string) => Promise<{ ok: boolean; text: () => Promise<string> }>

export async function loadConfig(fetcher: Fetcher): Promise<PanelConfig> {
  let response: { ok: boolean; text: () => Promise<string> }
  try {
    response = await fetcher('config.json')
  } catch {
    return { gatewayUrl: DEFAULT_GATEWAY_URL }
  }
  if (!response.ok) return { gatewayUrl: DEFAULT_GATEWAY_URL }
  return parseConfig(await response.text())
}
