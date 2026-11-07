import type { UpdatePhase } from './types'

const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'] as const

/** Human readable byte size; empty input renders as `0 B`. */
export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  let value = bytes
  let unit = 0
  while (value >= 1024 && unit < UNITS.length - 1) {
    value /= 1024
    unit += 1
  }
  const rounded = value >= 10 || unit === 0 ? Math.round(value) : Math.round(value * 10) / 10
  return `${rounded} ${UNITS[unit]}`
}

/** 0…100, rounded down so "100%" only appears when the download is finished. */
export function updatePercent(progress: number): number {
  if (!Number.isFinite(progress)) return 0
  return Math.min(100, Math.max(0, Math.floor(progress * 100)))
}

/** Whether the launcher should surface the update entry at all. */
export function isUpdateActionable(phase: UpdatePhase): boolean {
  return phase === 'available' || phase === 'downloading' || phase === 'ready' || phase === 'installing'
}

/**
 * Release notes arrive as Markdown. The About card shows them as plain text
 * (the full, linked version lives on the release page), so strip the markers
 * that read as noise when rendered verbatim.
 */
export function plainReleaseNotes(markdown: string): string {
  return markdown
    .replace(/\r\n/g, '\n')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, '')
    .replace(/\[([^\]]+)\]\([^)]*\)/g, '$1')
    .replace(/^\s{0,3}#{1,6}\s+/gm, '')
    .replace(/^\s{0,3}>\s?/gm, '')
    .replace(/\*\*([^*]+)\*\*/g, '$1')
    .replace(/`([^`]+)`/g, '$1')
    .replace(/^\s*[-*+]\s+/gm, '• ')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

export function updatePhaseLabel(phase: UpdatePhase): string {
  switch (phase) {
    case 'unsupported':
      return '不可用'
    case 'idle':
      return '未检查'
    case 'checking':
      return '检查中'
    case 'latest':
      return '已最新'
    case 'available':
      return '可更新'
    case 'downloading':
      return '下载中'
    case 'ready':
      return '待安装'
    case 'installing':
      return '安装中'
    case 'error':
      return '失败'
  }
}
