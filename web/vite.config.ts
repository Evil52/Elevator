import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  resolve: {
    alias: {
      '@elevator/protocol-v1': fileURLToPath(
        new URL('../protocol/v1/ts/src/index.ts', import.meta.url),
      ),
    },
  },
  server: { fs: { allow: ['..'] } },
  test: { environment: 'node', include: ['tests/**/*.test.ts'] },
})
