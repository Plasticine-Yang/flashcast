import type { FlashcastApi } from '@shared/types'
import { createMockApi } from './mock/mock-api'
import { mockWallpaperDark, mockWallpaperLight } from './mock/demo-data'

const bridge = typeof window !== 'undefined' ? window.flashcast : undefined

export const isMock = !bridge
export const api: FlashcastApi = bridge ?? createMockApi()

export function isPreview(): boolean {
  return isMock
}

/** Preview-only stand-in for the desktop wallpaper used by the frosted panel. */
export function previewWallpaper(theme: 'dark' | 'light'): string {
  return theme === 'light' ? mockWallpaperLight : mockWallpaperDark
}
