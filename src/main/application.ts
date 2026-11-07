import { app, Menu, nativeImage, nativeTheme, Notification, protocol, shell, Tray } from 'electron'
import { existsSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { mkdir, readFile, writeFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { extname, join } from 'node:path'
import { IPC } from '@shared/ipc'
import {
  BLUE_GLASS_THEME,
  resolveThemeState,
  supportedThemeModes,
  type ResolvedThemeState,
  type ThemeDefinition,
  type ThemeMode
} from '@shared/theme'
import type {
  ActionResult,
  AppPrefs,
  BootstrapData,
  CommandEntry,
  EditorCleanupResult,
  EditorWorkspaceSnapshot,
  ExternalPluginItem,
  FlashcastEvents,
  GlobalHotkeys,
  PluginInspection,
  PluginPage,
  PluginSettingsPage,
  PluginSummary,
  ShortcutError,
  ShortcutInfo,
  ThemeInspection,
  ThemePackError,
  UpdateState,
  WindowRole,
  WorkspaceSummary
} from '@shared/types'
import { CLIPBOARD_MEDIA_SCHEME, DESKTOP_WALLPAPER_SCHEME } from '@shared/types'
import { AppDiscovery } from './apps'
import { discoverChromeProfiles, openInChrome } from './chrome-bookmarks'
import { ClipboardService } from './clipboard-service'
import { ConfigStore } from './config-store'
import { ExternalPluginManager } from './external-plugins'
import { FaviconService } from './favicons'
import { desktopEntryDirs, findDesktopEntry, usesWayland } from './linux-portal'
import { log } from './logger'
import { pasteIntoActiveApp } from './native-paste'
import {
  appVersion,
  assetsDir,
  desktopName,
  launcherStatePath,
  readLauncherState,
  resolveInitialWorkspace,
  isDev,
  workspaceLayout,
  type WorkspaceLayout
} from './paths'
import { COMMANDS, PluginService } from './plugin-service'
import {
  CHROME_DEFAULTS,
  CLIPBOARD_DEFAULTS,
  VSCODE_LIKE_DEFAULTS,
  VSCODE_LIKE_WORKSPACE_MANIFEST
} from './builtins/manifests'
import { VscodeLikeWorkspaceService } from './vscode-like-workspace/service'
import { ShortcutManager } from './shortcut-manager'
import { ThemeManager } from './theme-manager'
import { UpdateService } from './update-service'
import { readDesktopWallpaper } from './wallpaper'
import { wallpaperRequestTheme } from './wallpaper-path'
import { WindowManager } from './window-manager'
import { WorkspaceService } from './workspace-service'
import { registerIpcHandlers } from './ipc'
import type { PlatformId } from '@shared/types'

const MEDIA_CONTENT_TYPES: Record<string, string> = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.gif': 'image/gif',
  '.bmp': 'image/bmp',
  '.svg': 'image/svg+xml',
  '.avif': 'image/avif'
}

export class FlashcastApplication {
  readonly platform: PlatformId
  private config!: ConfigStore
  private workspace!: WorkspaceService
  private clipboard!: ClipboardService
  private external!: ExternalPluginManager
  private plugins!: PluginService
  private editors!: VscodeLikeWorkspaceService
  private favicons!: FaviconService
  private apps!: AppDiscovery
  private shortcuts!: ShortcutManager
  private windows!: WindowManager
  private updates!: UpdateService
  private themes!: ThemeManager
  private tray: Tray | null = null
  private ready = false

  constructor(platform: PlatformId) {
    this.platform = platform
  }

  async init(): Promise<void> {
    const initialPath = resolveInitialWorkspace()
    this.config = new ConfigStore(workspaceLayout(initialPath), this.platform)
    await this.config.init()

    this.workspace = new WorkspaceService(this.config, initialPath)
    await this.workspace.init()

    const layout = this.workspace.workspaceLayout
    this.themes = new ThemeManager(
      layout,
      (snapshot) => this.broadcast(IPC.themesChanged, snapshot),
      (message, tone = 'default') => this.toast(message, tone)
    )
    await this.themes.init()

    this.clipboard = new ClipboardService(this.config, this.platform, layout)
    await this.clipboard.init()

    this.favicons = new FaviconService(layout.faviconDir)
    this.editors = new VscodeLikeWorkspaceService(this.config, this.platform)
    await this.editors.init()
    this.external = new ExternalPluginManager(
      this.config,
      layout,
      (plugins) => {
        this.broadcast(IPC.pluginChanged, this.plugins ? this.plugins.getSummaries() : plugins)
      },
      (message, tone = 'default') => this.toast(message, tone)
    )
    await this.external.init()
    // Materialise the default configs that belong to the versioned workspace.
    await this.config.getPluginConfig('clipboard', CLIPBOARD_DEFAULTS)
    await this.config.getPluginConfig('chrome-bookmarks', CHROME_DEFAULTS)
    await this.config.getPluginConfig(VSCODE_LIKE_WORKSPACE_MANIFEST.id, VSCODE_LIKE_DEFAULTS)
    await this.workspace.finalizeInitialCommit()
    this.plugins = new PluginService(
      this.config,
      this.clipboard,
      this.external,
      this.editors,
      this.platform
    )
    this.apps = new AppDiscovery(this.platform)

    this.setupTheme()
    await this.applyLaunchAtLogin(this.config.getPrefs().launchAtLogin)

    this.windows = new WindowManager(this.platform, {
      onPasteAfterHide: () => pasteIntoActiveApp(this.platform),
      theme: () => this.themeForWindow()
    })
    this.shortcuts = new ShortcutManager(this.platform, {
      launcher: () => this.windows.toggleLauncher(),
      openClipboard: () => this.windows.openClipboardPlugin()
    })

    this.wireEvents()
    this.registerMediaProtocol()
    this.registerWallpaperProtocol()
    this.setupMenu()

    registerIpcHandlers(this)

    this.shortcuts.apply(this.config.getPrefs().hotkeys, { transactional: false })
    this.workspace.startStatusPolling()

    // Create the updater before the first window so an early `updates:*` IPC
    // call always finds a live service.
    this.updates = new UpdateService(this.platform, (state) => {
      this.broadcast(IPC.updateChanged, state)
    })
    this.updates.start()
    this.windows.createLauncher()
    this.setupTray()

    const state = readLauncherState()
    const shouldShow =
      process.env.FLASHCAST_SHOW_ON_START === '1' ||
      !state.hasLaunched
    this.ready = true
    if (shouldShow) {
      this.windows.showLauncher()
    }
    this.markLaunched()

    const shortcutErrors = this.shortcutErrors()
    if (shortcutErrors.length > 0) {
      log.warn('global shortcut issues:', shortcutErrors)
      const error = shortcutErrors[0]
      try {
        new Notification({
          title: 'Flashcast 全局快捷键注册失败',
          body: `${error.accelerator}：${error.message}${
            error.id === 'system' ? '' : '。可从托盘打开设置并更换快捷键'
          }`
        }).show()
      } catch {
        // Notifications are best-effort.
      }
    }
  }

  private wireEvents(): void {
    this.clipboard.on('changed', (items) => {
      this.broadcast(IPC.clipboardChanged, items)
    })
    this.workspace.on('changed', (summary: WorkspaceSummary) => {
      this.broadcast(IPC.workspaceChanged, summary)
    })
    nativeTheme.on('updated', () => {
      if (!this.themes) return
      const requested = this.themes.getTheme(this.config.getPrefs().themeId)
      // A single-variant theme ignores the OS setting, so reacting would only
      // re-apply the same state and can loop with nativeTheme updates.
      if (supportedThemeModes(requested).length === 1) return
      this.applyThemeState(true)
    })
  }

  private setupTheme(): void {
    this.applyThemeState(false)
  }

  private computeThemeState(): {
    state: ResolvedThemeState
    definition: ThemeDefinition
    fallbackReason: string | null
  } {
    const prefs = this.config.getPrefs()
    const requested = this.themes.getTheme(prefs.themeId)
    const supported = supportedThemeModes(requested)
    const desiredThemeSource = supported.length === 1 ? supported[0] : prefs.theme
    if (nativeTheme.themeSource !== desiredThemeSource) {
      nativeTheme.themeSource = desiredThemeSource
    }
    const systemMode: ThemeMode = nativeTheme.shouldUseDarkColors ? 'dark' : 'light'
    const state = resolveThemeState(requested, prefs.theme, systemMode, BLUE_GLASS_THEME)
    const definition = this.themes.getTheme(state.themeId) ?? BLUE_GLASS_THEME
    const fallbackReason =
      state.themeId === prefs.themeId
        ? null
        : requested
          ? '主题包没有可用的深色或浅色变体'
          : '未找到主题包，或主题包校验失败'
    return { state, definition, fallbackReason }
  }

  private applyThemeState(broadcast = true): {
    state: ResolvedThemeState
    definition: ThemeDefinition
    fallbackReason: string | null
  } {
    const computed = this.computeThemeState()
    const variant = computed.definition.variants[computed.state.mode]
    const background =
      variant?.tokens.windowSolid ?? BLUE_GLASS_THEME.variants.dark!.tokens.windowSolid
    this.windows?.setTheme(background)
    if (broadcast) {
      const prefs = this.config.getPrefs()
      this.broadcast(IPC.themeChanged, {
        theme: computed.state.mode,
        setting: prefs.theme,
        themeId: computed.state.themeId,
        locked: computed.state.locked,
        supportedModes: computed.state.supportedModes,
        definition: computed.definition,
        requestedThemeId: prefs.themeId,
        fallbackReason: computed.fallbackReason
      })
      this.broadcast(IPC.themesChanged, this.themes.snapshot())
    }
    return computed
  }

  private themeForWindow(): { themeId: string; mode: ThemeMode; background: string } {
    const { state, definition } = this.computeThemeState()
    return {
      themeId: definition.id,
      mode: state.mode,
      background:
        definition.variants[state.mode]?.tokens.windowSolid ??
        BLUE_GLASS_THEME.variants.dark!.tokens.windowSolid
    }
  }

  async getBootstrap(role: WindowRole): Promise<BootstrapData> {
    const chromeProfiles =
      role === 'settings' ? (await discoverChromeProfiles(this.platform)).profiles : []
    const { state, fallbackReason } = this.computeThemeState()
    const prefs = this.config.getPrefs()
    return {
      role,
      platform: this.platform,
      version: appVersion(),
      prefs,
      theme: state.mode,
      themeId: state.themeId,
      themeLocked: state.locked,
      themeSupportedModes: state.supportedModes,
      themeFallback:
        fallbackReason === null
          ? null
          : { requestedId: prefs.themeId, reason: fallbackReason },
      themes: this.themes.getThemes(),
      themeErrors: this.themes.getErrors(),
      shortcuts: this.shortcuts.getInfo(),
      shortcutErrors: this.shortcutErrors(),
      plugins: this.plugins.getSummaries(),
      commands: COMMANDS,
      workspace: this.workspace.getSummary(),
      chromeProfiles,
      launchAtLoginSupported: true,
      dataDir: this.config.dataDir,
      isDev: isDev()
    }
  }

  /**
   * Registration errors plus environment problems that make every global
   * shortcut silently inoperative. On Wayland the portal refuses to bind
   * shortcuts for an app identity it cannot resolve to an installed
   * `.desktop` file, and `globalShortcut.register()` still reports success.
   */
  private shortcutErrors(): ShortcutError[] {
    return [...this.shortcuts.getErrors(), ...this.portalIdentityErrors()]
  }

  private portalIdentityErrors(): ShortcutError[] {
    if (this.platform !== 'linux' || !usesWayland()) return []
    const identity = desktopName()
    if (findDesktopEntry(identity, desktopEntryDirs())) return []
    return [
      {
        id: 'system',
        accelerator: identity,
        message:
          'Wayland 下全局快捷键经由 xdg-desktop-portal 注册，系统找不到同名的 .desktop 文件时会静默拒绝绑定。' +
          '开发时重新运行 pnpm dev 会自动写入，重新打包安装后也会随包提供。'
      }
    ]
  }

  async getApps() {
    return this.apps.list()
  }

  async refreshApps() {
    return this.apps.refresh()
  }

  async launchApp(appId: string): Promise<ActionResult> {
    const result = await this.apps.launch(appId)
    return result.ok
      ? { ok: true, close: true }
      : { ok: false, error: result.error ?? '启动应用失败' }
  }

  async runCommand(commandId: CommandEntry['id']): Promise<ActionResult> {
    if (commandId === 'settings') {
      this.windows.openSettings()
      return { ok: true }
    }
    if (commandId === 'check-updates') {
      const state = await this.checkForUpdates()
      if (state.phase === 'available') {
        return {
          ok: true,
          toast: {
            message: `发现新版本 v${state.latestVersion}，可用右上角的更新按钮一键安装`,
            tone: 'warning'
          }
        }
      }
      if (state.phase === 'latest') {
        return {
          ok: true,
          toast: { message: `当前已是最新版本 v${state.currentVersion}`, tone: 'success' }
        }
      }
      return { ok: false, error: state.error ?? state.message }
    }
    if (commandId === 'refresh-apps') {
      await this.apps.refresh()
      return { ok: true, toast: { message: '已重新加载应用列表', tone: 'success' } }
    }
    if (commandId === 'quit') {
      setTimeout(() => app.quit(), 60)
      return { ok: true }
    }
    return { ok: false, error: '未知命令' }
  }

  async getPluginPage(pluginId: string): Promise<PluginPage> {
    return this.plugins.getPage(pluginId)
  }

  async getPluginSettings(
    pluginId: string,
    refresh = false
  ): Promise<PluginSettingsPage> {
    return this.plugins.getSettings(pluginId, { refresh })
  }

  async savePluginSettings(
    pluginId: string,
    patch: Record<string, unknown>
  ): Promise<{ page: PluginSettingsPage; savedAt: number }> {
    const result = await this.plugins.saveSettings(pluginId, patch)
    this.broadcast(IPC.pluginChanged, this.plugins.getSummaries())
    return result
  }

  async searchExternal(pluginId: string, query: string): Promise<ExternalPluginItem[]> {
    return this.plugins.searchExternal(pluginId, query)
  }

  async activateExternal(
    pluginId: string,
    itemId: string,
    modifiers: { alt: boolean }
  ): Promise<ActionResult> {
    const result = await this.plugins.activateExternal(pluginId, itemId, modifiers?.alt === true)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async activateClipboard(itemId: string, options: { paste: boolean }): Promise<ActionResult> {
    return this.plugins.activateClipboard(itemId, options?.paste === true)
  }

  async deleteClipboard(itemId: string): Promise<ActionResult> {
    return this.plugins.deleteClipboard(itemId)
  }

  async clearClipboard(): Promise<ActionResult> {
    const result = await this.plugins.clearClipboard()
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async openEditorWorkspace(
    workspaceId: string,
    options: { newWindow: boolean }
  ): Promise<ActionResult> {
    const result = await this.plugins.openEditorWorkspace(workspaceId, options)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async revealEditorWorkspace(workspaceId: string): Promise<ActionResult> {
    const result = await this.plugins.revealEditorWorkspace(workspaceId)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async getEditorWorkspaces(): Promise<EditorWorkspaceSnapshot[]> {
    return this.plugins.getEditorWorkspaceSnapshots()
  }

  async deleteEditorWorkspaces(
    variantId: string,
    ids: string[]
  ): Promise<EditorCleanupResult> {
    const result = await this.plugins.deleteEditorWorkspaces(variantId, ids)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async openChromeBookmark(bookmark: { id: string; url: string }): Promise<ActionResult> {
    try {
      await openInChrome(bookmark.url, this.platform)
      return { ok: true, close: true }
    } catch (error) {
      return { ok: false, error: (error as Error).message }
    }
  }

  async getFavicons(domains: string[]): Promise<Record<string, string>> {
    return this.favicons.get(Array.isArray(domains) ? domains.slice(0, 100) : [])
  }

  async updatePrefs(
    patch: Partial<Pick<AppPrefs, 'theme' | 'themeId' | 'launchAtLogin' | 'autoCommit'>>
  ): Promise<BootstrapData> {
    const configPatch: Partial<AppPrefs> = {}
    if (patch.theme && ['system', 'dark', 'light'].includes(patch.theme)) configPatch.theme = patch.theme
    if (patch.themeId !== undefined) {
      const themeId = typeof patch.themeId === 'string' ? patch.themeId.trim() : ''
      if (!themeId || !this.themes.hasTheme(themeId)) {
        throw new Error('主题不存在或不可用')
      }
      configPatch.themeId = themeId
    }
    if (typeof patch.launchAtLogin === 'boolean') configPatch.launchAtLogin = patch.launchAtLogin
    if (Object.keys(configPatch).length > 0) {
      await this.config.updatePrefs(configPatch)
    }
    if (typeof patch.launchAtLogin === 'boolean') {
      await this.applyLaunchAtLogin(patch.launchAtLogin)
    }
    if (typeof patch.autoCommit === 'boolean') {
      await this.workspace.setAutoCommit(patch.autoCommit)
    }

    const prefs = this.config.getPrefs()
    this.broadcast(IPC.prefsChanged, prefs)
    this.applyThemeState(true)
    this.broadcast(IPC.workspaceChanged, this.workspace.getSummary())
    return this.getBootstrap('settings')
  }

  async chooseWorkspace(): Promise<{ canceled: boolean; workspace: WorkspaceSummary }> {
    const parent = this.windows.settingsWindow ?? this.windows.launcherWindow
    const result = await this.workspace.chooseWorkspace(parent)
    if (result.canceled) return result

    await this.applyWorkspaceChange()
    return { canceled: false, workspace: this.workspace.getSummary() }
  }

  private async applyWorkspaceChange(): Promise<void> {
    const layout = this.workspace.workspaceLayout
    // The new workspace's config is already loaded; resolve its theme before
    // touching windows so the new workspace's themes/ directory wins.
    await this.themes.setLayout(layout)
    this.applyThemeState(true)
    this.broadcast(IPC.prefsChanged, this.config.getPrefs())

    await this.clipboard.setWorkspace(layout)
    await this.external.setLayout(layout)
    await this.external.reload()
    this.editors.invalidate()
    await this.editors.init()
    this.favicons.setRoot(layout.faviconDir)
    this.shortcuts.apply(this.config.getPrefs().hotkeys, { transactional: false })
    await this.applyLaunchAtLogin(this.config.getPrefs().launchAtLogin)

    this.broadcast(IPC.pluginChanged, this.plugins.getSummaries())
    this.broadcast(IPC.shortcutsChanged, {
      shortcuts: this.shortcuts.getInfo(),
      errors: this.shortcutErrors()
    })
    this.broadcast(IPC.workspaceChanged, this.workspace.getSummary())
    this.broadcast(IPC.clipboardChanged, this.clipboard.getItems())
    this.toast('已切换工作区', 'success')
  }

  async openWorkspaceFolder(): Promise<void> {
    await this.workspace.openFolder()
  }

  async refreshWorkspace(): Promise<WorkspaceSummary> {
    return this.workspace.refreshStatus()
  }

  async getGitDiff(): Promise<{ status: string; diff: string; error: string | null }> {
    return this.workspace.diff()
  }

  /* --------------------------------- themes ------------------------------- */

  async inspectTheme(input: string): Promise<ThemeInspection> {
    return this.themes.inspect(input)
  }

  async installTheme(token: string): Promise<{
    theme: ThemeDefinition
    themes: ThemeDefinition[]
    errors: ThemePackError[]
  }> {
    const theme = await this.themes.install(token)
    await this.requestWorkspaceCommit(`theme: install ${theme.id}@${theme.version}`)
    this.applyThemeState(true)
    if (!this.config.getPrefs().autoCommit) {
      this.toast('已写入工作区；提交并推送后可在其他电脑恢复', 'success')
    }
    return {
      theme,
      themes: this.themes.getThemes(),
      errors: this.themes.getErrors()
    }
  }

  async updateTheme(themeId: string): Promise<ActionResult> {
    const result = await this.themes.update(themeId)
    if (!result.ok) return result
    await this.requestWorkspaceCommit(`theme: update ${themeId}`)
    this.applyThemeState(true)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    if (!this.config.getPrefs().autoCommit) {
      this.toast('已写入工作区；提交并推送后可在其他电脑恢复', 'success')
    }
    return result
  }

  async uninstallTheme(themeId: string): Promise<ActionResult> {
    const previous = this.config.getPrefs().themeId
    if (previous === themeId) {
      await this.config.updatePrefs({ themeId: BLUE_GLASS_THEME.id })
    }
    const result = await this.themes.uninstall(themeId)
    if (!result.ok) {
      if (previous === themeId && this.themes.hasTheme(previous)) {
        await this.config.updatePrefs({ themeId: previous })
      }
      return result
    }
    await this.requestWorkspaceCommit(`theme: remove ${themeId}`)
    this.applyThemeState(true)
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    if (!this.config.getPrefs().autoCommit) {
      this.toast('已写入工作区；提交并推送后可在其他电脑恢复', 'success')
    }
    return result
  }

  async openThemeDirectory(themeId: string): Promise<void> {
    await this.themes.openDirectory(themeId)
  }

  async reloadThemes(): Promise<{ themes: ThemeDefinition[]; errors: ThemePackError[] }> {
    const snapshot = await this.themes.reload()
    this.applyThemeState(true)
    return snapshot
  }

  private async requestWorkspaceCommit(message: string): Promise<void> {
    try {
      const result = await this.workspace.requestCommit(message)
      if (result.error) {
        this.toast(`自动提交失败：${result.error}`, 'danger')
      }
    } catch (error) {
      const reason = (error as Error).message
      log.warn('workspace commit failed:', reason)
      this.toast(`自动提交失败：${reason}`, 'danger')
    }
  }

  async inspectPlugin(input: string): Promise<PluginInspection> {
    return this.external.inspect(input)
  }

  async installPlugin(token: string): Promise<{ plugin: PluginSummary; plugins: PluginSummary[] }> {
    const plugin = await this.external.install(token)
    await this.requestWorkspaceCommit(`plugin: install ${plugin.id}@${plugin.version}`)
    if (!this.config.getPrefs().autoCommit) {
      this.toast('插件代码已写入工作区；提交并推送后可在其他电脑恢复', 'success')
    }
    return { plugin, plugins: this.plugins.getSummaries() }
  }

  async updatePlugin(pluginId: string): Promise<ActionResult> {
    const result = await this.external.update(pluginId)
    if (result.ok) {
      await this.requestWorkspaceCommit(`plugin: update ${pluginId}`)
    }
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    if (result.ok && !this.config.getPrefs().autoCommit) {
      this.toast('插件代码已写入工作区；提交并推送后可在其他电脑恢复', 'success')
    }
    return result
  }

  async uninstallPlugin(pluginId: string): Promise<ActionResult> {
    const result = await this.external.uninstall(pluginId)
    if (result.ok) {
      await this.requestWorkspaceCommit(`plugin: remove ${pluginId}`)
      if (!this.config.getPrefs().autoCommit) {
        this.toast('插件代码已从工作区移除；提交并推送后可在其他电脑同步', 'success')
      }
    }
    return result
  }

  async setPluginEnabled(pluginId: string, enabled: boolean): Promise<ActionResult> {
    return this.external.setEnabled(pluginId, enabled)
  }

  async openPluginDirectory(pluginId: string): Promise<void> {
    await this.external.openDirectory(pluginId)
  }

  async setShortcuts(patch: Partial<GlobalHotkeys>): Promise<{
    shortcuts: ShortcutInfo[]
    errors: ShortcutError[]
  }> {
    const current = this.config.getPrefs().hotkeys
    const next = { ...current, ...patch }
    const result = this.shortcuts.apply(next, { transactional: true })
    if (result.ok) {
      await this.config.setHotkeys(result.hotkeys)
    }
    this.broadcast(IPC.shortcutsChanged, {
      shortcuts: this.shortcuts.getInfo(),
      errors: this.shortcutErrors()
    })
    return { shortcuts: this.shortcuts.getInfo(), errors: this.shortcutErrors() }
  }

  async resetShortcuts(): Promise<{ shortcuts: ShortcutInfo[]; errors: ShortcutError[] }> {
    const result = this.shortcuts.apply(this.shortcuts.getDefaults(), { transactional: true })
    if (result.ok) {
      await this.config.setHotkeys(result.hotkeys)
    }
    this.broadcast(IPC.shortcutsChanged, {
      shortcuts: this.shortcuts.getInfo(),
      errors: this.shortcutErrors()
    })
    return { shortcuts: this.shortcuts.getInfo(), errors: this.shortcutErrors() }
  }

  /* --------------------------------- updates ------------------------------ */

  getUpdateState(): UpdateState {
    return this.updates.getState()
  }

  async checkForUpdates(): Promise<UpdateState> {
    return this.updates.check({ force: true })
  }

  async downloadUpdate(): Promise<UpdateState> {
    return this.updates.download()
  }

  async cancelUpdate(): Promise<UpdateState> {
    return this.updates.cancel()
  }

  async installUpdate(): Promise<ActionResult> {
    const result = await this.updates.install()
    if (result.toast) this.broadcast(IPC.toast, result.toast)
    return result
  }

  async openReleasePage(): Promise<void> {
    const url = this.updates.getState().releaseUrl
    if (url) await shell.openExternal(url)
  }

  async showUpdateFile(): Promise<void> {
    const path = this.updates.getState().downloadPath
    if (path) shell.showItemInFolder(path)
  }

  async applyLaunchAtLogin(enabled: boolean): Promise<void> {
    try {
      if (this.platform === 'linux') {
        const dev = isDev()
        const autostartDir = process.env.FLASHCAST_AUTOSTART_DIR ?? join(homedir(), '.config', 'autostart')
        // 开发版写独立文件，开关自启动时不会覆盖打包版的条目。
        const file = join(autostartDir, dev ? 'flashcast-dev.desktop' : 'flashcast.desktop')
        if (enabled) {
          await mkdir(autostartDir, { recursive: true })
          const exec =
            process.env.APPIMAGE ??
            `${quoteExec(process.execPath)}${dev ? ` --no-sandbox ${quoteExec(app.getAppPath())}` : ''}`
          await writeFile(
            file,
            `[Desktop Entry]\nType=Application\nName=${dev ? 'Flashcast (Dev)' : 'Flashcast'}\nComment=Keyboard-first launcher\nExec=${exec}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n`,
            'utf8'
          )
        } else if (existsSync(file)) {
          rmSync(file, { force: true })
        }
      } else {
        app.setLoginItemSettings({ openAtLogin: enabled })
      }
    } catch (error) {
      log.warn('launch at login could not be applied:', (error as Error).message)
    }
  }

  openSettings(): void {
    this.windows.openSettings()
  }

  hideLauncher(options?: { paste?: boolean }): void {
    this.windows.hideImmediately(options?.paste === true)
  }

  toast(message: string, tone: 'default' | 'success' | 'warning' | 'danger' = 'default'): void {
    this.broadcast(IPC.toast, { message, tone })
  }

  broadcast<K extends keyof FlashcastEvents>(channel: string, payload: FlashcastEvents[K]): void {
    this.windows?.broadcast(channel, payload)
  }

  private markLaunched(): void {
    try {
      const state = readLauncherState()
      writeFileSync(
        launcherStatePath(),
        `${JSON.stringify({ ...state, hasLaunched: true }, null, 2)}\n`,
        'utf8'
      )
    } catch {
      // The user data directory may be read-only in tests.
    }
  }

  private setupTray(): void {
    try {
      const named = this.platform === 'darwin' ? 'trayTemplate.png' : 'tray.png'
      const iconPath = join(assetsDir(), named)
      if (!existsSync(iconPath)) {
        log.warn('tray icon missing:', iconPath)
        return
      }
      this.tray = new Tray(this.trayImage(iconPath))
      this.tray.setToolTip(isDev() ? 'Flashcast (Dev)' : 'Flashcast')
      const menu = Menu.buildFromTemplate([
        { label: '打开 Flashcast', click: () => this.windows.toggleLauncher() },
        { label: '设置…', click: () => this.windows.openSettings() },
        { type: 'separator' },
        { label: '退出 Flashcast', click: () => app.quit() }
      ])
      this.tray.setContextMenu(menu)
      if (this.platform !== 'darwin') {
        this.tray.on('click', () => this.windows.toggleLauncher())
      }
    } catch (error) {
      log.warn('tray setup failed:', (error as Error).message)
    }
  }

  /**
   * macOS 菜单栏图标必须按 point 尺寸提供：18pt 的模板图（@2x 36px）才会和系统
   * 图标等高。以前直接把 32pt 的 PNG 交给 Tray，状态栏里的闪电比旁边图标大一圈。
   * 这里显式组装 1x/2x 表示并标记为模板图，避免依赖文件名约定。
   */
  private trayImage(basePath: string): Electron.NativeImage | string {
    if (this.platform !== 'darwin') return basePath
    const retinaPath = basePath.replace(/\.png$/, '@2x.png')
    if (!existsSync(retinaPath)) return basePath
    const image = nativeImage.createEmpty()
    image.addRepresentation({ scaleFactor: 1, buffer: readFileSync(basePath) })
    image.addRepresentation({ scaleFactor: 2, buffer: readFileSync(retinaPath) })
    image.setTemplateImage(true)
    return image
  }

  private setupMenu(): void {
    if (this.platform !== 'darwin') {
      Menu.setApplicationMenu(null)
      return
    }
    app.dock?.hide()
    const menu = Menu.buildFromTemplate([
      {
        label: 'Flashcast',
        submenu: [
          { label: '打开 Flashcast', click: () => this.windows.showLauncher() },
          { label: '设置…', click: () => this.windows.openSettings() },
          { type: 'separator' },
          { role: 'quit', label: '退出 Flashcast' }
        ]
      },
      { role: 'editMenu' }
    ])
    Menu.setApplicationMenu(menu)
  }

  /**
   * Serves the current desktop wallpaper as a downscaled JPEG. A transparent
   * Electron window cannot sample the desktop behind it, so the launcher uses
   * this image (heavily blurred in CSS) as the frosted backdrop instead.
   */
  private registerWallpaperProtocol(): void {
    protocol.handle(DESKTOP_WALLPAPER_SCHEME, async (request) => {
      try {
        const theme = wallpaperRequestTheme(request.url)
        if (!theme) return new Response('Not found', { status: 404 })
        const wallpaper = await readDesktopWallpaper(theme)
        if (!wallpaper) return new Response('Not found', { status: 404 })
        return new Response(new Uint8Array(wallpaper.data), {
          headers: {
            'Content-Type': wallpaper.contentType,
            'Cache-Control': 'no-cache'
          }
        })
      } catch {
        return new Response('Not found', { status: 404 })
      }
    })
  }

  private registerMediaProtocol(): void {
    protocol.handle(CLIPBOARD_MEDIA_SCHEME, async (request) => {
      try {
        const url = new URL(request.url)
        if (url.hostname !== 'media') return new Response('Not found', { status: 404 })
        const name = decodeURIComponent(url.pathname.replace(/^\//, ''))
        if (!/^[A-Za-z0-9][A-Za-z0-9._-]*$/.test(name) || name.includes('..')) {
          return new Response('Invalid media path', { status: 400 })
        }
        const file = join(this.clipboard.mediaDir, name)
        const data = await readFile(file)
        const contentType = MEDIA_CONTENT_TYPES[extname(name).toLowerCase()] ?? 'application/octet-stream'
        return new Response(new Uint8Array(data), {
          headers: {
            'Content-Type': contentType,
            'Cache-Control': 'no-store',
            'Access-Control-Allow-Origin': '*'
          }
        })
      } catch {
        return new Response('Not found', { status: 404 })
      }
    })
  }

  getClips() {
    return this.clipboard
  }

  getWorkspaceLayout(): WorkspaceLayout {
    return this.workspace.workspaceLayout
  }

  getConfig(): ConfigStore {
    return this.config
  }

  getWindows(): WindowManager {
    return this.windows
  }

  getShortcutManager(): ShortcutManager {
    return this.shortcuts
  }

  getPluginService(): PluginService {
    return this.plugins
  }

  getExternalPlugins(): ExternalPluginManager {
    return this.external
  }

  getAppsDiscovery(): AppDiscovery {
    return this.apps
  }

  showLauncher(pluginId?: string): void {
    this.windows.showLauncher(pluginId)
  }

  isReady(): boolean {
    return this.ready
  }

  dispose(): void {
    this.updates?.dispose()
    this.workspace?.stop()
    this.clipboard?.stop()
    this.shortcuts?.unregisterAll()
    this.tray?.destroy()
    this.tray = null
  }
}

function quoteExec(value: string): string {
  return /[\s"']/.test(value) ? `"${value.replace(/(["\\$`])/g, '\\$1')}"` : value
}

/* Keeps the generic signature ergonomic for the IPC layer without polluting the domain. */
export type { FlashcastEvents }

// Circular import is avoided by injecting the registry function below.
// eslint-disable-next-line @typescript-eslint/no-var-requires
