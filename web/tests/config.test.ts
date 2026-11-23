import { describe, expect, it } from 'vitest'
import { DEFAULT_GATEWAY_URL, loadConfig, parseConfig } from '../src/config.ts'

describe('разбор config.json', () => {
  it('берёт адрес шлюза', () => {
    expect(parseConfig('{"gatewayUrl":"wss://panel.example/ws"}')).toEqual({
      gatewayUrl: 'wss://panel.example/ws',
    })
  })

  it('принимает только ws и wss', () => {
    for (const bad of ['http://x', 'ftp://x', 'javascript:alert(1)', '/relative', '']) {
      expect(() => parseConfig(JSON.stringify({ gatewayUrl: bad }))).toThrow()
    }
    expect(parseConfig('{"gatewayUrl":"ws://127.0.0.1:8787"}').gatewayUrl).toBe('ws://127.0.0.1:8787')
  })

  it('отвергает не объект, отсутствие поля и не строку', () => {
    for (const bad of ['null', '[]', '42', '{}', '{"gatewayUrl":5}', '{broken']) {
      expect(() => parseConfig(bad)).toThrow()
    }
  })
})

describe('загрузка config.json', () => {
  it('читает файл рядом со сборкой', async () => {
    const config = await loadConfig(async (url) => {
      expect(url).toBe('config.json')
      return { ok: true, text: async () => '{"gatewayUrl":"wss://a.b/ws"}' }
    })
    expect(config.gatewayUrl).toBe('wss://a.b/ws')
  })

  it('без файла или при ошибке берёт локальный адрес по умолчанию', async () => {
    expect(
      (await loadConfig(async () => ({ ok: false, text: async () => '' }))).gatewayUrl,
    ).toBe(DEFAULT_GATEWAY_URL)
    expect(
      (await loadConfig(async () => {
        throw new Error('offline')
      })).gatewayUrl,
    ).toBe(DEFAULT_GATEWAY_URL)
  })

  it('испорченный файл не подменяется молча: ошибка пробрасывается', async () => {
    await expect(
      loadConfig(async () => ({ ok: true, text: async () => '{"gatewayUrl":"http://x"}' })),
    ).rejects.toThrow()
  })
})
