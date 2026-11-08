/// <reference types="vite/client" />

import type { FlashcastApi } from '@shared/types'

declare global {
  /** Injected by electron.vite.config.ts / vite.preview.config.ts from package.json. */
  const __FLASHCAST_VERSION__: string

  interface Window {
    flashcast?: FlashcastApi
  }
}
