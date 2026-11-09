import { describe, expect, it, vi } from 'vitest'

const systemPreferences = vi.hoisted(() => ({
  isTrustedAccessibilityClient: vi.fn(() => true)
}))

vi.mock('electron', () => ({ systemPreferences }))

const {
  MAC_ACCESSIBILITY_HINT,
  MAC_AUTOMATION_HINT,
  appleScriptString,
  pasteFailureMessage,
  pasteIntoActiveApp,
  pasteScript
} = await import('./native-paste')

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

  it('points at the automation pane for -1743', () => {
    expect(pasteFailureMessage('execution error: (-1743)', 'darwin')).toEqual({
      message: MAC_AUTOMATION_HINT,
      settings: 'automation'
    })
  })

  it('points at the accessibility pane for -25211', () => {
    expect(pasteFailureMessage('not allowed assistive access (-25211)', 'darwin')).toEqual({
      message: MAC_ACCESSIBILITY_HINT,
      settings: 'accessibility'
    })
  })

  it('keeps unknown errors as-is', () => {
    expect(pasteFailureMessage('timed out', 'darwin')).toEqual({ message: 'timed out' })
    expect(pasteFailureMessage('-1743', 'linux')).toEqual({ message: '-1743' })
    expect(pasteFailureMessage('', 'linux')).toEqual({ message: '无法触发粘贴' })
  })

  it('refuses to paste while accessibility is not granted, and says so', async () => {
    systemPreferences.isTrustedAccessibilityClient.mockReturnValue(false)
    const result = await pasteIntoActiveApp('darwin', { appName: 'Safari' })
    expect(result).toEqual({ ok: false, error: MAC_ACCESSIBILITY_HINT, settings: 'accessibility' })
    // Registered the app in the pane so the user only has to flip the switch.
    expect(systemPreferences.isTrustedAccessibilityClient).toHaveBeenCalledWith(true)
  })
})
