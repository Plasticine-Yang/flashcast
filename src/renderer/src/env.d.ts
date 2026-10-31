/// <reference types="vite/client" />

import type { FlashcastApi } from '@shared/types'

declare global {
  interface Window {
    flashcast?: FlashcastApi
  }
}
