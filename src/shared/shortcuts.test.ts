import { describe, expect, it } from 'vitest'
import {
  acceleratorFromEvent,
  canonicalizeAccelerator,
  clearAllAccelerator,
  defaultHotkeys,
  eventMatchesAccelerator,
  formatAccelerator,
  keycapParts,
  toElectronAccelerator
} from './shortcuts'

const base = { ctrlKey: false, altKey: false, shiftKey: false, metaKey: false }

describe('shortcut handling', () => {
  it('records canonical shortcuts from keyboard events', () => {
    expect(
      acceleratorFromEvent({ ...base, key: 'c', code: 'KeyC', ctrlKey: true, altKey: true })
    ).toBe('Control+Alt+C')
    expect(
      acceleratorFromEvent({ ...base, key: ' ', code: 'Space', altKey: true })
    ).toBe('Alt+Space')
  })

  it('rejects modifier-only, Escape and Tab events', () => {
    expect(acceleratorFromEvent({ ...base, key: 'Control', code: 'ControlLeft', ctrlKey: true })).toBeNull()
    expect(acceleratorFromEvent({ ...base, key: 'Escape', code: 'Escape' })).toBeNull()
    expect(acceleratorFromEvent({ ...base, key: 'Tab', code: 'Tab' })).toBeNull()
  })

  it('canonicalises modifier order', () => {
    expect(canonicalizeAccelerator(['Alt', 'Control', 'C'])).toBe('Control+Alt+C')
    expect(canonicalizeAccelerator(['meta', 'shift', 'k'])).toBe('Shift+Meta+K')
  })

  it('maps the canonical Meta token to the platform accelerator', () => {
    expect(toElectronAccelerator('Control+Meta+C', 'darwin')).toBe('Control+Command+C')
    expect(toElectronAccelerator('Control+Meta+C', 'linux')).toBe('Control+Super+C')
    expect(toElectronAccelerator('Alt+Space', 'win32')).toBe('Alt+Space')
  })

  it('formats platform-specific keycaps', () => {
    expect(keycapParts('Alt+Space', 'darwin')).toEqual(['⌥', 'Space'])
    expect(keycapParts('Control+Alt+C', 'linux')).toEqual(['Ctrl', 'Alt', 'C'])
    expect(keycapParts('Meta+Alt+Backspace', 'win32')).toEqual(['Win', 'Alt', '⌫'])
    expect(formatAccelerator('Control+Meta+C', 'darwin')).toBe('⌃ ⌘ C')
  })

  it('matches events against accelerators', () => {
    expect(
      eventMatchesAccelerator(clearAllAccelerator('linux'), {
        ...base,
        key: 'Backspace',
        code: 'Backspace',
        altKey: true,
        metaKey: true
      })
    ).toBe(true)
    expect(
      eventMatchesAccelerator('Control+Alt+C', {
        ...base,
        key: 'c',
        code: 'KeyC',
        ctrlKey: true,
        altKey: true
      })
    ).toBe(true)
    expect(
      eventMatchesAccelerator('Control+Alt+C', {
        ...base,
        key: 'c',
        code: 'KeyC',
        ctrlKey: true
      })
    ).toBe(false)
  })
})

describe('defaultHotkeys', () => {
  it('uses the same defaults for dev and packaged builds', () => {
    expect(defaultHotkeys('linux')).toEqual({
      launcher: 'Alt+Space',
      openClipboard: 'Control+Alt+C'
    })
    expect(defaultHotkeys('darwin')).toEqual({
      launcher: 'Alt+Space',
      openClipboard: 'Control+Meta+C'
    })
  })
})
