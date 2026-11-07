import { readFile } from 'node:fs/promises'
import { EventEmitter } from 'node:events'
import type { AppPrefs, GlobalHotkeys, PlatformId, ThemeSetting } from '@shared/types'
import { DEFAULT_THEME_ID } from '@shared/theme'
import { defaultHotkeys } from '@shared/shortcuts'
import { readJson, writeJsonAtomic, ensureDir } from './json-store'
import type { WorkspaceLayout } from './paths'

interface StoredConfig {
  version: 1
  theme: ThemeSetting
  themeId: string
  launchAtLogin: boolean
  language: 'zh-CN'
  autoCommit: boolean
  hotkeys: GlobalHotkeys
  enabledPlugins: Record<string, boolean>
}

export class ConfigStore extends EventEmitter {
  private prefs: AppPrefs
  private layout: WorkspaceLayout

  constructor(
    layout: WorkspaceLayout,
    private readonly platform: PlatformId
  ) {
    super()
    this.layout = layout
    this.prefs = this.defaults()
  }

  private defaults(): AppPrefs {
    return {
      theme: 'dark',
      themeId: DEFAULT_THEME_ID,
      launchAtLogin: false,
      language: 'zh-CN',
      autoCommit: false,
      hotkeys: defaultHotkeys(this.platform),
      enabledPlugins: {}
    }
  }

  get workspace(): WorkspaceLayout {
    return this.layout
  }

  get dataDir(): string {
    return this.layout.root
  }

  async init(): Promise<void> {
    await ensureDir(this.layout.root)
    await ensureDir(this.layout.pluginsDir)
    await ensureDir(this.layout.stateDir)
    await ensureDir(this.layout.extensionsDir)

    const raw = await readJson<Partial<StoredConfig> & { hotkeys?: Partial<GlobalHotkeys> }>(
      this.layout.configFile,
      {}
    )
    const defaults = this.defaults()
    this.prefs = {
      theme: isTheme(raw.theme) ? raw.theme : defaults.theme,
      themeId: normalizeThemeId(raw.themeId) ?? defaults.themeId,
      launchAtLogin: typeof raw.launchAtLogin === 'boolean' ? raw.launchAtLogin : defaults.launchAtLogin,
      language: 'zh-CN',
      autoCommit: typeof raw.autoCommit === 'boolean' ? raw.autoCommit : defaults.autoCommit,
      hotkeys: {
        launcher:
          typeof raw.hotkeys?.launcher === 'string' && raw.hotkeys.launcher
            ? raw.hotkeys.launcher
            : defaults.hotkeys.launcher,
        openClipboard:
          typeof raw.hotkeys?.openClipboard === 'string' && raw.hotkeys.openClipboard
            ? raw.hotkeys.openClipboard
            : defaults.hotkeys.openClipboard
      },
      enabledPlugins:
        raw.enabledPlugins && typeof raw.enabledPlugins === 'object' ? { ...raw.enabledPlugins } : {}
    }
    await this.persist()
  }

  async setWorkspace(layout: WorkspaceLayout): Promise<void> {
    this.layout = layout
    await this.init()
  }

  getPrefs(): AppPrefs {
    return structuredClone(this.prefs)
  }

  isPluginEnabled(id: string): boolean {
    return this.prefs.enabledPlugins[id] !== false
  }

  async updatePrefs(patch: Partial<AppPrefs>): Promise<AppPrefs> {
    this.prefs = {
      ...this.prefs,
      ...patch,
      themeId:
        patch.themeId === undefined
          ? this.prefs.themeId
          : (normalizeThemeId(patch.themeId) ?? DEFAULT_THEME_ID),
      hotkeys: patch.hotkeys ? { ...patch.hotkeys } : this.prefs.hotkeys,
      enabledPlugins: patch.enabledPlugins
        ? { ...this.prefs.enabledPlugins, ...patch.enabledPlugins }
        : this.prefs.enabledPlugins
    }
    await this.persist()
    this.emit('prefs', this.getPrefs())
    return this.getPrefs()
  }

  async setEnabled(pluginId: string, enabled: boolean): Promise<void> {
    this.prefs.enabledPlugins = { ...this.prefs.enabledPlugins, [pluginId]: enabled }
    await this.persist()
    this.emit('prefs', this.getPrefs())
  }

  async setHotkeys(hotkeys: GlobalHotkeys): Promise<void> {
    this.prefs.hotkeys = { ...hotkeys }
    await this.persist()
    this.emit('prefs', this.getPrefs())
  }

  async getPluginConfig<T extends Record<string, unknown>>(
    pluginId: string,
    defaults: T
  ): Promise<T> {
    const path = this.layout.pluginConfigFile(pluginId)
    const stored = await readJson<Record<string, unknown>>(path, {})
    const merged = { ...defaults, ...stored } as T
    await ensureDir(this.layout.pluginDir(pluginId))
    try {
      await readFile(path, 'utf8')
    } catch {
      await writeJsonAtomic(path, merged)
    }
    return merged
  }

  async savePluginConfig<T extends Record<string, unknown>>(
    pluginId: string,
    patch: Record<string, unknown>,
    defaults: T
  ): Promise<T> {
    const current = await this.getPluginConfig(pluginId, defaults)
    const next = { ...current, ...patch } as T
    await ensureDir(this.layout.pluginDir(pluginId))
    await writeJsonAtomic(this.layout.pluginConfigFile(pluginId), next)
    this.emit('plugin-config', { pluginId, config: next })
    this.emit('persist', { reason: 'plugin-config', pluginId })
    return next
  }

  async reloadPluginConfig<T extends Record<string, unknown>>(pluginId: string, defaults: T): Promise<T> {
    return this.getPluginConfig(pluginId, defaults)
  }

  private async persist(): Promise<void> {
    const stored: StoredConfig = {
      version: 1,
      theme: this.prefs.theme,
      themeId: this.prefs.themeId,
      launchAtLogin: this.prefs.launchAtLogin,
      language: this.prefs.language,
      autoCommit: this.prefs.autoCommit,
      hotkeys: this.prefs.hotkeys,
      enabledPlugins: this.prefs.enabledPlugins
    }
    await writeJsonAtomic(this.layout.configFile, stored)
    this.emit('persist', { reason: 'prefs' })
  }
}

function isTheme(value: unknown): value is ThemeSetting {
  return value === 'system' || value === 'dark' || value === 'light'
}

function normalizeThemeId(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}
