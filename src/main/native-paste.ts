import { systemPreferences } from 'electron'
import { execFile } from 'node:child_process'
import { existsSync } from 'node:fs'
import { createRequire } from 'node:module'
import { delimiter, join } from 'node:path'
import { promisify } from 'node:util'
import type { PlatformId, SystemSettingsPane } from '@shared/types'
import { log } from './logger'

const execFileAsync = promisify(execFile)

export interface PasteTarget {
  /** macOS process name of the app that was frontmost before the launcher. */
  appName?: string | null
  /** Process id captured before the launcher stole focus; enables the native backend. */
  pid?: number | null
}

/**
 * The macOS fast path. It lives in the Electron main process so TCC attributes
 * the synthetic ⌘V to Flashcast itself (Accessibility/PostEvent), exactly like
 * a native clipboard manager; no System Events or Automation grant involved.
 */
export interface NativePasteAddon {
  frontmost(): { pid: number; name: string } | null
  activate(pid: number): boolean
  paste(pid: number): boolean
}

export interface FrontmostApplication {
  pid: number
  name: string
}

let nativeAddon: NativePasteAddon | null | undefined

function nativeCandidates(): string[] {
  const file = `paste-${process.arch}.node`
  const candidates: string[] = []
  if (process.resourcesPath) candidates.push(join(process.resourcesPath, 'native', file))
  candidates.push(join(__dirname, '../../native/dist', file))
  candidates.push(join(__dirname, '../../native/paste/build/Release/paste.node'))
  return candidates
}

/**
 * Loads the optional macOS addon. Tests must never post real keystrokes, and
 * `FLASHCAST_DISABLE_NATIVE_PASTE=1` keeps a pure-AppleScript path available
 * for debugging or for users who want to compare both backends.
 */
export function nativePaste(): NativePasteAddon | null {
  if (nativeAddon !== undefined) return nativeAddon
  nativeAddon = null
  if (
    process.platform !== 'darwin' ||
    process.env.VITEST ||
    process.env.FLASHCAST_DISABLE_NATIVE_PASTE === '1'
  ) {
    return null
  }

  let requireNative: NodeJS.Require
  try {
    requireNative = createRequire(join(process.cwd(), 'package.json'))
  } catch {
    return null
  }

  for (const file of nativeCandidates()) {
    if (!existsSync(file)) continue
    try {
      const loaded = requireNative(file) as Partial<NativePasteAddon>
      if (
        typeof loaded.frontmost === 'function' &&
        typeof loaded.activate === 'function' &&
        typeof loaded.paste === 'function'
      ) {
        nativeAddon = loaded as NativePasteAddon
        log.info('native paste backend:', file)
        return nativeAddon
      }
    } catch (error) {
      log.warn('native paste backend failed:', file, (error as Error).message)
    }
  }
  log.warn('native paste backend unavailable; using AppleScript fallback')
  return null
}

/** Frontmost app via the addon, or null when the native backend is unavailable. */
export function frontmostApplication(): FrontmostApplication | null {
  const addon = nativePaste()
  if (!addon) return null
  try {
    const result = addon.frontmost()
    return result && result.pid > 0 ? result : null
  } catch (error) {
    log.warn('native frontmost failed:', (error as Error).message)
    return null
  }
}

/**
 * Hands focus back to the app the palette was opened from. Windows/Linux let
 * the window manager restore focus when the launcher hides, so only macOS
 * needs an explicit activation; false means the caller should fall back to
 * `app.hide()`.
 */
export function activatePreviousApp(platform: PlatformId, target: PasteTarget): boolean {
  if (platform !== 'darwin') return true
  const addon = nativePaste()
  if (!addon || !target.pid) return false
  try {
    return addon.activate(target.pid)
  } catch (error) {
    log.warn('native activate failed:', (error as Error).message)
    return false
  }
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
  // Activation is asynchronous: `set frontmost` returns before the window
  // server actually moves focus, and the keystroke follows immediately. Wait
  // until the target reports itself frontmost so ⌘V cannot land on us.
  return [
    'tell application "System Events"',
    '  try',
    `    set target to first process whose name is ${appleScriptString(appName)}`,
    '    set frontmost of target to true',
    '    repeat 12 times',
    '      if frontmost of target then exit repeat',
    '      delay 0.05',
    '    end repeat',
    '  end try',
    `  ${keystroke}`,
    'end tell'
  ].join('\n')
}

/** Which System Settings pane the user has to touch. */
export type PasteSettingsPane = Exclude<SystemSettingsPane, 'full-disk-access'>

export const MAC_ACCESSIBILITY_HINT =
  '需要在「系统设置 → 隐私与安全性 → 辅助功能」里允许 Flashcast。覆盖安装会让旧条目与新签名对不上（开关看着是开的也不生效）：请选中 Flashcast 用「−」移除，再按 ⌥↵ 让系统重新登记并打开开关，然后重启 Flashcast'

export const MAC_AUTOMATION_HINT =
  '需要在「系统设置 → 隐私与安全性 → 自动化」里允许 Flashcast 控制 System Events，然后重启 Flashcast'

export const MAC_FULL_DISK_ACCESS_HINT =
  'macOS 不允许 Flashcast 读取 Chrome 的数据目录。请在「系统设置 → 隐私与安全性 → 完全磁盘访问权限」里勾选 Flashcast，然后重启 Flashcast。覆盖安装后系统会按新签名重新校验，开关看着是开的也可能需要先移除再加回来'

/** System Settings deep links; clicking the failure notification opens them. */
export const MAC_SETTINGS_URL: Record<SystemSettingsPane, string> = {
  'full-disk-access':
    'x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles',
  accessibility: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility',
  automation: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Automation'
}

/**
 * Turns the raw osascript error into something the user can act on. macOS
 * reports a missing Automation grant as -1743 and a missing Accessibility
 * grant as -25211.
 */
export function pasteFailureMessage(
  error: string,
  platform: PlatformId
): { message: string; settings?: PasteSettingsPane } {
  const raw = error.trim() || '无法触发粘贴'
  if (platform !== 'darwin') return { message: raw }
  if (/-1743|not authori[sz]ed|未获得授权/i.test(raw)) {
    return { message: MAC_AUTOMATION_HINT, settings: 'automation' }
  }
  if (/-25211|assistive access|不允许|not allowed/i.test(raw)) {
    return { message: MAC_ACCESSIBILITY_HINT, settings: 'accessibility' }
  }
  return { message: raw }
}

let accessibilityPrompted = false

/**
 * Posting synthetic keystrokes needs the Accessibility grant for the running
 * build. The check is cheap and lets the UI point at the right pane instead of
 * guessing; asking once per process also registers the app in the list, which
 * is what the user needs after an update swapped the code signature.
 */
function accessibilityTrusted(): boolean {
  try {
    if (systemPreferences.isTrustedAccessibilityClient(false)) return true
    if (!accessibilityPrompted) {
      accessibilityPrompted = true
      systemPreferences.isTrustedAccessibilityClient(true)
    }
    return false
  } catch {
    // Unknown: let the keystroke attempt decide.
    return true
  }
}

/**
 * Synthesises a paste keystroke in the previously focused app. There is no
 * portable Electron API for this, so the host relies on small platform helpers:
 * wtype/xdotool/ydotool on Linux, the native CGEvent addon on macOS, and
 * AppleScript as the macOS fallback; PowerShell on Windows.
 */
export async function pasteIntoActiveApp(
  platform: PlatformId,
  target: PasteTarget = {}
): Promise<{ ok: boolean; error?: string; settings?: PasteSettingsPane }> {
  try {
    if (platform === 'darwin') {
      if (!accessibilityTrusted()) {
        log.warn('paste blocked: accessibility permission missing')
        return { ok: false, error: MAC_ACCESSIBILITY_HINT, settings: 'accessibility' }
      }
      const addon = nativePaste()
      if (addon && target.pid) {
        try {
          if (addon.paste(target.pid)) {
            log.info('pasted via native backend:', target.appName ?? target.pid)
            return { ok: true }
          }
          log.warn('native paste returned false; falling back to AppleScript')
        } catch (error) {
          log.warn('native paste failed:', (error as Error).message)
        }
      }
      log.info('pasting into:', target.appName ?? '(unspecified)')
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
    const failure = pasteFailureMessage((error as Error).message, platform)
    log.warn('paste keystroke failed:', (error as Error).message)
    return { ok: false, error: failure.message, settings: failure.settings }
  }
}
