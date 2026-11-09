import { describe, expect, it } from 'vitest'
import {
  MAC_PASTE_PERMISSION_HINT,
  appleScriptString,
  pasteFailureMessage,
  pasteScript
} from './native-paste'

describe('paste helpers', () => {
  it('escapes app names so they cannot break the AppleScript', () => {
    expect(appleScriptString('Safari')).toBe('"Safari"')
    expect(appleScriptString('My "App"')).toBe('"My \\"App\\""')
    expect(appleScriptString('a\\b')).toBe('"a\\\\b"')
    expect(appleScriptString('two\nlines')).toBe('"two lines"')
  })

  it('presses cmd+V on its own when the target app is unknown', () => {
    expect(pasteScript(null)).toBe(
      'tell application "System Events" to keystroke "v" using command down'
    )
  })

  it('hands focus back before the keystroke and survives a missing app', () => {
    const script = pasteScript('Visual Studio Code')
    expect(script).toContain(
      'set frontmost of (first process whose name is "Visual Studio Code") to true'
    )
    expect(script).toContain('try')
    expect(script.indexOf('set frontmost')).toBeLessThan(script.indexOf('keystroke "v"'))
  })

  it('explains macOS permission errors instead of echoing them', () => {
    expect(pasteFailureMessage('execution error: (-1743)', 'darwin')).toBe(MAC_PASTE_PERMISSION_HINT)
    expect(pasteFailureMessage('not allowed assistive access (-25211)', 'darwin')).toBe(
      MAC_PASTE_PERMISSION_HINT
    )
    expect(pasteFailureMessage('timed out', 'darwin')).toBe('timed out')
    expect(pasteFailureMessage('-1743', 'linux')).toBe('-1743')
    expect(pasteFailureMessage('', 'linux')).toBe('无法触发粘贴')
  })
})
