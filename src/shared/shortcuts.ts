import type { GlobalHotkeys, PlatformId } from './types'

export interface KeyboardLike {
  key: string
  code?: string
  ctrlKey: boolean
  altKey: boolean
  shiftKey: boolean
  metaKey: boolean
}

const MODIFIER_ORDER = ['Control', 'Alt', 'Shift', 'Meta'] as const
type ModifierToken = (typeof MODIFIER_ORDER)[number]

export function isMac(platform: PlatformId): boolean {
  return platform === 'darwin'
}

/**
 * 开发版的默认组合与打包版错开：两个进程可能同时运行，同一个加速键只能被其中
 * 一方注册成功，后启动的一方会静默失败。
 */
export function defaultHotkeys(platform: PlatformId, dev = false): GlobalHotkeys {
  if (dev) {
    return {
      launcher: 'Alt+Shift+Space',
      openClipboard: platform === 'darwin' ? 'Control+Meta+Shift+C' : 'Control+Alt+Shift+C'
    }
  }
  return {
    launcher: 'Alt+Space',
    openClipboard: platform === 'darwin' ? 'Control+Meta+C' : 'Control+Alt+C'
  }
}

function parseAccelerator(accelerator: string): string[] {
  return accelerator
    .split('+')
    .map((part) => part.trim())
    .filter(Boolean)
}

/** Canonical modifier ordering keeps persisted values stable across sessions. */
export function canonicalizeAccelerator(parts: string[]): string {
  const modifiers: string[] = []
  let key = ''
  for (const part of parts) {
    const normalised = normaliseModifier(part)
    if (normalised) modifiers.push(normalised)
    else key = normaliseKeyName(part)
  }
  const ordered = MODIFIER_ORDER.filter((modifier) => modifiers.includes(modifier))
  return [...ordered, key].filter(Boolean).join('+')
}

function normaliseModifier(part: string): ModifierToken | null {
  const value = part.toLowerCase()
  if (value === 'ctrl' || value === 'control') return 'Control'
  if (value === 'alt' || value === 'option' || value === 'opt') return 'Alt'
  if (value === 'shift') return 'Shift'
  if (value === 'meta' || value === 'command' || value === 'cmd' || value === 'super' || value === 'win') {
    return 'Meta'
  }
  return null
}

function normaliseKeyName(part: string): string {
  const value = part.trim()
  if (!value) return ''
  const lower = value.toLowerCase()
  const map: Record<string, string> = {
    space: 'Space',
    spacebar: 'Space',
    backspace: 'Backspace',
    delete: 'Delete',
    del: 'Delete',
    enter: 'Enter',
    return: 'Enter',
    escape: 'Escape',
    esc: 'Escape',
    tab: 'Tab',
    arrowup: 'ArrowUp',
    up: 'ArrowUp',
    arrowdown: 'ArrowDown',
    down: 'ArrowDown',
    arrowleft: 'ArrowLeft',
    left: 'ArrowLeft',
    arrowright: 'ArrowRight',
    right: 'ArrowRight',
    home: 'Home',
    end: 'End',
    pageup: 'PageUp',
    pagedown: 'PageDown',
    insert: 'Insert',
    capslock: 'Capslock'
  }
  if (map[lower]) return map[lower]
  if (/^f([1-9]|1\d|2[0-4])$/.test(lower)) return lower.toUpperCase()
  if (/^[a-z0-9]$/i.test(value)) return value.toUpperCase()
  return value.toUpperCase()
}

/**
 * Electron accelerators are platform-specific for the Meta key. Persisted
 * shortcuts always use the canonical "Meta" token.
 */
export function toElectronAccelerator(accelerator: string, platform: PlatformId): string | null {
  const parts = parseAccelerator(accelerator)
  const modifiers: string[] = []
  let key = ''
  for (const part of parts) {
    const modifier = normaliseModifier(part)
    if (modifier) {
      if (modifier === 'Meta') modifiers.push(isMac(platform) ? 'Command' : 'Super')
      else modifiers.push(modifier)
    } else {
      key = normaliseKeyName(part)
    }
  }
  if (!key) return null
  return [...modifiers, electronKeyName(key)].filter(Boolean).join('+')
}

function electronKeyName(key: string): string {
  switch (key) {
    case 'Enter':
      return 'Return'
    case 'ArrowUp':
      return 'Up'
    case 'ArrowDown':
      return 'Down'
    case 'ArrowLeft':
      return 'Left'
    case 'ArrowRight':
      return 'Right'
    case 'Escape':
      return 'Escape'
    default:
      return key
  }
}

function eventKeyName(event: KeyboardLike): string | null {
  const code = event.code ?? ''
  if (/^Key[A-Z]$/.test(code)) return code.slice(3)
  if (/^Digit[0-9]$/.test(code)) return code.slice(5)
  if (/^F([1-9]|1\d|2[0-4])$/.test(code)) return code
  const codeMap: Record<string, string> = {
    Space: 'Space',
    Backspace: 'Backspace',
    Delete: 'Delete',
    Enter: 'Enter',
    NumpadEnter: 'Enter',
    Escape: 'Escape',
    Tab: 'Tab',
    ArrowUp: 'ArrowUp',
    ArrowDown: 'ArrowDown',
    ArrowLeft: 'ArrowLeft',
    ArrowRight: 'ArrowRight',
    Home: 'Home',
    End: 'End',
    PageUp: 'PageUp',
    PageDown: 'PageDown',
    Insert: 'Insert',
    Minus: '-',
    Equal: '=',
    BracketLeft: '[',
    BracketRight: ']',
    Backslash: '\\',
    Semicolon: ';',
    Quote: "'",
    Comma: ',',
    Period: '.',
    Slash: '/',
    Backquote: '`'
  }
  if (code && codeMap[code]) return codeMap[code]

  if (!event.key) return null
  const key = event.key
  if (key.length === 1 && /[a-z0-9]/i.test(key)) return key.toUpperCase()
  const named = normaliseKeyName(key)
  if (['Space', 'Backspace', 'Delete', 'Enter', 'Escape', 'Tab'].includes(named)) return named
  return null
}

/**
 * Records a keyboard event as a canonical accelerator. Returns null for
 * modifier-only presses, Escape (reserved by the host) and unsupported keys.
 */
export function acceleratorFromEvent(event: KeyboardLike): string | null {
  const key = eventKeyName(event)
  if (!key) return null
  if (key === 'Escape' || key === 'Tab') return null

  const modifiers: string[] = []
  if (event.ctrlKey) modifiers.push('Control')
  if (event.altKey) modifiers.push('Alt')
  if (event.shiftKey) modifiers.push('Shift')
  if (event.metaKey) modifiers.push('Meta')

  const hasPrimaryModifier = event.ctrlKey || event.altKey || event.metaKey
  if (!hasPrimaryModifier && !/^F([1-9]|1\d|2[0-4])$/.test(key)) return null

  return canonicalizeAccelerator([...modifiers, key])
}

export function eventMatchesAccelerator(
  accelerator: string,
  event: KeyboardLike
): boolean {
  const parts = parseAccelerator(accelerator)
  let expectedKey = ''
  const expected = { Control: false, Alt: false, Shift: false, Meta: false }
  for (const part of parts) {
    const modifier = normaliseModifier(part)
    if (modifier) expected[modifier] = true
    else expectedKey = normaliseKeyName(part)
  }
  const key = eventKeyName(event)
  if (!key || !expectedKey) return false
  if (key !== expectedKey) return false
  return (
    expected.Control === event.ctrlKey &&
    expected.Alt === event.altKey &&
    expected.Shift === event.shiftKey &&
    expected.Meta === event.metaKey
  )
}

export interface ParsedShortcut {
  modifiers: string[]
  key: string
}

export function parseShortcut(accelerator: string): ParsedShortcut {
  const parts = parseAccelerator(accelerator)
  const modifiers: string[] = []
  let key = ''
  for (const part of parts) {
    const modifier = normaliseModifier(part)
    if (modifier) modifiers.push(modifier)
    else key = normaliseKeyName(part)
  }
  return { modifiers, key }
}

const MAC_SYMBOLS: Record<string, string> = {
  Control: '⌃',
  Alt: '⌥',
  Shift: '⇧',
  Meta: '⌘',
  Command: '⌘',
  Super: '⌘',
  Win: '⌘'
}

const KEY_LABELS: Record<string, string> = {
  Space: 'Space',
  Backspace: '⌫',
  Delete: '⌦',
  Enter: '↵',
  Escape: 'esc',
  Tab: '⇥',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  PageUp: 'PgUp',
  PageDown: 'PgDn',
  Capslock: 'Caps'
}

function modifierLabel(modifier: string, platform: PlatformId): string {
  if (isMac(platform)) return MAC_SYMBOLS[modifier] ?? modifier
  if (modifier === 'Control') return 'Ctrl'
  if (modifier === 'Meta') return platform === 'win32' ? 'Win' : 'Super'
  return modifier
}

/** Returns display-ready keycap labels for a canonical accelerator. */
export function keycapParts(accelerator: string, platform: PlatformId): string[] {
  if (!accelerator) return []
  const { modifiers, key } = parseShortcut(accelerator)
  const labels = modifiers.map((modifier) => modifierLabel(modifier, platform))
  if (key) labels.push(KEY_LABELS[key] ?? key)
  return labels
}

export function formatAccelerator(accelerator: string, platform: PlatformId): string {
  return keycapParts(accelerator, platform).join(isMac(platform) ? ' ' : ' ')
}

export function clearAllAccelerator(platform: PlatformId): string {
  return isMac(platform) ? 'Alt+Meta+Backspace' : 'Meta+Alt+Backspace'
}

export function vimKeycap(letter: 'j' | 'k' | 'h' | 'l', platform: PlatformId): string[] {
  return [isMac(platform) ? '⌃' : 'Ctrl', letter.toUpperCase()]
}

export function primaryActionModifier(platform: PlatformId): string {
  return isMac(platform) ? '⌥' : 'Alt'
}
