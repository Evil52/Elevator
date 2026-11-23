import { fileURLToPath } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

const alias = {
  '@elevator/protocol-v1': fileURLToPath(new URL('../protocol/v1/ts/src/index.ts', import.meta.url)),
}

export default defineConfig({
  plugins: [react()],
  resolve: { alias },
  server: { fs: { allow: ['..'] } },
  test: {
    projects: [
      {
        extends: true,
        test: { name: 'unit', environment: 'node', include: ['tests/*.test.ts'] },
      },
      {
        extends: true,
        test: {
          name: 'ui',
          environment: 'jsdom',
          setupFiles: ['tests/ui/setup.ts'],
          include: ['tests/ui/**/*.test.tsx', 'tests/ui/**/*.test.ts'],
        },
      },
    ],
  },
})
