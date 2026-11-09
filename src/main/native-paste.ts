import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { delimiter, join } from 'node:path'
import { promisify } from 'node:util'
import type { PlatformId } from '@shared/types'
import { log } from './logger'

const execFileAsync = promisify(execFile)

/** Give the window server a moment to finish hiding the launcher. */
const SETTLE_MS = 120

export interface PasteTarget {
  /** macOS process name of the app that was frontmost before the launcher. */
  appName?: string | null
}

function commandExists(command: string): boolean {
  if (command.includes('/')) return existsSync(command)
  return (process.env.PATH ?? '')
    .split(delimiter)
    .filter(Boolean)
    .some((dir) => existsSync(join(dir, command)))
}

async function run(command: string, args: string[]): Promise<void> {
  await execFileAsync(command, args, { timeout: 4000 })
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms))
}

/** Quotes a value for AppleScript so an app name cannot break the script. */
export function appleScriptString(value: string): string {
  const escaped = value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')
  return `"${escaped.replace(/[\r\n]+/g, ' ')}"`
}

/**
 * One osascript run that hands focus back to the app the palette was opened
 * from and then presses ⌘V. Activating first is what makes the paste land:
 * hiding the launcher leaves this accessory app frontmost, so the keystroke
 * would otherwise be delivered to our own hidden window. The activation is
 * best effort — the keystroke still has to run when the app is gone.
 */
export function pasteScript(appName?: string | null): string {
  const keystroke = 'keystroke "v" using command down'
  if (!appName) return `tell application "System Events" to ${keystroke}`
  return [
    'tell application "System Events"',
    '  try',
    `    set frontmost of (first process whose name is ${appleScriptString(appName)}) to true`,
    '    delay 0.12',
    '  end try',
    `  ${keystroke}`,
    'end tell'
  ].join('\n')
}

export const MAC_PASTE_PERMISSION_HINT =
  '请在「系统设置 → 隐私与安全性 → 辅助功能」中允许 Flashcast，并在「自动化」中允许它控制 System Events，然后重启 Flashcast'

/**
 * Turns the raw osascript error into something the user can act on. macOS
 * reports missing Accessibility / Automation grants as -1743 and -25211.
 */
export function pasteFailureMessage(error: string, platform: PlatformId): string {
  const raw = error.trim() || '无法触发粘贴'
  if (platform !== 'darwin') return raw
  if (/-1743|-25211|not authori[sz]ed|not allowed assistive access|不允许/i.test(raw)) {
    return MAC_PASTE_PERMISSION_HINT
  }
  return raw
}

/**
 * Synthesises a paste keystroke in the previously focused app. There is no
 * portable Electron API for this, so the host relies on small platform helpers:
 * wtype/xdotool/ydotool on Linux, AppleScript on macOS, PowerShell on Windows.
 */
export async function pasteIntoActiveApp(
  platform: PlatformId,
  target: PasteTarget = {}
): Promise<{ ok: boolean; error?: string }> {
  await delay(SETTLE_MS)

  try {
    if (platform === 'darwin') {
      await run('osascript', ['-e', pasteScript(target.appName)])
      return { ok: true }
    }

    if (platform === 'win32') {
      await run('powershell.exe', [
        '-NoProfile',
        '-NonInteractive',
        '-Command',
        "$wshell = New-Object -ComObject WScript.Shell; $wshell.SendKeys('^v')"
      ])
      return { ok: true }
    }

    if (process.env.WAYLAND_DISPLAY && commandExists('wtype')) {
      await run('wtype', ['-M', 'ctrl', 'v', '-m', 'ctrl'])
      return { ok: true }
    }
    if (commandExists('xdotool')) {
      await run('xdotool', ['key', '--clearmodifiers', 'ctrl+v'])
      return { ok: true }
    }
    if (commandExists('ydotool')) {
      await run('ydotool', ['key', '29:1', '47:1', '47:0', '29:0'])
      return { ok: true }
    }

    const message =
      '无法触发粘贴：请安装 wtype（Wayland）或 xdotool（X11），或使用 option/alt+enter 以外的复制方式'
    log.warn(message)
    return { ok: false, error: message }
  } catch (error) {
    const message = pasteFailureMessage((error as Error).message, platform)
    log.warn('paste keystroke failed:', (error as Error).message)
    return { ok: false, error: message }
  }
}
