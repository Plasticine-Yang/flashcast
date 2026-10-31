import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { delimiter, join } from 'node:path'
import { promisify } from 'node:util'
import type { PlatformId } from '@shared/types'
import { log } from './logger'

const execFileAsync = promisify(execFile)

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

/**
 * Synthesises a paste keystroke in the previously focused app. There is no
 * portable Electron API for this, so the host relies on small platform helpers:
 * wtype/xdotool/ydotool on Linux, AppleScript on macOS, PowerShell on Windows.
 */
export async function pasteIntoActiveApp(platform: PlatformId): Promise<{ ok: boolean; error?: string }> {
  await new Promise((resolve) => setTimeout(resolve, 120))

  try {
    if (platform === 'darwin') {
      await run('osascript', [
        '-e',
        'tell application "System Events" to keystroke "v" using command down'
      ])
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
    const message = (error as Error).message
    log.warn('paste keystroke failed:', message)
    return { ok: false, error: message }
  }
}
