import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

const { version } = JSON.parse(readFileSync(resolve(__dirname, 'package.json'), 'utf8')) as {
  version: string
}

/**
 * Browser-only preview harness. It mounts the exact same renderer with a mock
 * bridge so the UI can be reviewed/screenshotted without Electron.
 */
export default defineConfig({
  root: resolve(__dirname, 'src/renderer'),
  define: {
    __FLASHCAST_VERSION__: JSON.stringify(version)
  },
  resolve: {
    alias: {
      '@shared': resolve(__dirname, 'src/shared'),
      '@renderer': resolve(__dirname, 'src/renderer/src')
    }
  },
  plugins: [react()],
  server: {
    port: 5199,
    strictPort: true
  }
})
