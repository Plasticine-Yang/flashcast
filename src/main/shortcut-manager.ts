import { globalShortcut } from 'electron'
import type { GlobalHotkeys, PlatformId, ShortcutError, ShortcutInfo } from '@shared/types'
import { canonicalizeAccelerator, defaultHotkeys, parseShortcut, toElectronAccelerator } from '@shared/shortcuts'
import { log } from './logger'

export interface ShortcutHandlers {
  launcher: () => void
  openClipboard: () => void
}

export class ShortcutManager {
  private hotkeys: GlobalHotkeys
  private errors: ShortcutError[] = []
  private registered = new Set<string>()

  constructor(
    private readonly platform: PlatformId,
    private readonly handlers: ShortcutHandlers
  ) {
    this.hotkeys = defaultHotkeys(platform)
  }

  getInfo(): ShortcutInfo[] {
    const defaults = defaultHotkeys(this.platform)
    return [
      {
        id: 'launcher',
        label: '打开 / 关闭 Flashcast',
        description: '在任意应用中唤起启动器',
        accelerator: this.hotkeys.launcher,
        defaultAccelerator: defaults.launcher,
        editable: true
      },
      {
        id: 'openClipboard',
        label: '打开剪切板',
        description: '直接进入剪切板插件',
        accelerator: this.hotkeys.openClipboard,
        defaultAccelerator: defaults.openClipboard,
        editable: true
      }
    ]
  }

  getErrors(): ShortcutError[] {
    return [...this.errors]
  }

  getHotkeys(): GlobalHotkeys {
    return { ...this.hotkeys }
  }

  /**
   * Applies a new hotkey set. When `transactional` is true any registration
   * failure rolls the session back to the previous working hotkeys.
   */
  apply(
    next: GlobalHotkeys,
    options: { transactional: boolean } = { transactional: true }
  ): { ok: boolean; errors: ShortcutError[]; hotkeys: GlobalHotkeys } {
    const previous = this.hotkeys
    const errors = validateHotkeys(next, this.platform)
    if (errors.length > 0) {
      this.errors = errors
      return { ok: false, errors, hotkeys: this.hotkeys }
    }

    this.unregisterAll()
    const registrationErrors = this.registerAll(next)
    if (registrationErrors.length === 0) {
      this.hotkeys = next
      this.errors = []
      return { ok: true, errors: [], hotkeys: this.getHotkeys() }
    }

    // Attempt to restore the previous set so the user never loses their launcher.
    this.unregisterAll()
    this.registerAll(previous)
    if (options.transactional) {
      this.hotkeys = previous
      this.errors = registrationErrors
      return { ok: false, errors: registrationErrors, hotkeys: this.getHotkeys() }
    }

    // Initial boot is best-effort: keep partial registrations and surface errors.
    this.hotkeys = next
    this.errors = registrationErrors
    return { ok: false, errors: registrationErrors, hotkeys: this.getHotkeys() }
  }

  private registerAll(hotkeys: GlobalHotkeys): ShortcutError[] {
    const errors: ShortcutError[] = []
    const entries: Array<[keyof GlobalHotkeys, () => void]> = [
      ['launcher', this.handlers.launcher],
      ['openClipboard', this.handlers.openClipboard]
    ]

    for (const [action, handler] of entries) {
      const accelerator = hotkeys[action]
      const electronAccelerator = toElectronAccelerator(accelerator, this.platform)
      if (!electronAccelerator) {
        errors.push({
          id: action,
          accelerator,
          message: '快捷键格式无效'
        })
        continue
      }
      try {
        const ok = globalShortcut.register(electronAccelerator, handler)
        if (ok) {
          this.registered.add(action)
          log.info(`registered global shortcut ${accelerator} (${action})`)
        } else {
          errors.push({
            id: action,
            accelerator,
            message: '快捷键已被系统或其他应用占用'
          })
        }
      } catch (error) {
        errors.push({
          id: action,
          accelerator,
          message: (error as Error).message
        })
      }
    }
    if (errors.length > 0) {
      log.warn('global shortcut registration issues:', errors)
    }
    return errors
  }

  unregisterAll(): void {
    globalShortcut.unregisterAll()
    this.registered.clear()
  }
}

function validateHotkeys(hotkeys: GlobalHotkeys, platform: PlatformId): ShortcutError[] {
  const errors: ShortcutError[] = []
  for (const action of ['launcher', 'openClipboard'] as const) {
    const accelerator = hotkeys[action]
    const parsed = parseShortcut(accelerator)
    const hasModifier = parsed.modifiers.some((modifier) => ['Control', 'Alt', 'Meta'].includes(modifier))
    if (!parsed.key || !hasModifier) {
      errors.push({
        id: action,
        accelerator,
        message: '快捷键需要至少一个 Ctrl / Alt / ⌘ 修饰键'
      })
    }
  }

  if (
    hotkeys.launcher &&
    hotkeys.openClipboard &&
    canonicalizeAccelerator(parseShortcut(hotkeys.launcher).modifiers.concat(parseShortcut(hotkeys.launcher).key)) ===
      canonicalizeAccelerator(
        parseShortcut(hotkeys.openClipboard).modifiers.concat(parseShortcut(hotkeys.openClipboard).key)
      )
  ) {
    errors.push({
      id: 'openClipboard',
      accelerator: hotkeys.openClipboard,
      message: '与「打开 / 关闭 Flashcast」冲突'
    })
  }
  return errors
}
