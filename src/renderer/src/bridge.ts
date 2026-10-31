import type { FlashcastApi } from '@shared/types'
import { createMockApi } from './mock/mock-api'

const bridge = typeof window !== 'undefined' ? window.flashcast : undefined

export const isMock = !bridge
export const api: FlashcastApi = bridge ?? createMockApi()

export function isPreview(): boolean {
  return isMock
}
