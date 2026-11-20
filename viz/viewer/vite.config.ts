import { defineConfig } from 'vitest/config'

export default defineConfig({
  server: { host: '127.0.0.1', fs: { allow: ['..'] } },
  preview: { host: '127.0.0.1' },
  test: { environment: 'jsdom' },
  build: {
    assetsInlineLimit: 0,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.endsWith('/three/build/three.core.js')) return 'three-core'
          if (id.endsWith('/three/build/three.module.js')) return 'three-renderer'
        },
      },
    },
  },
})
