import { normalizeTrigger, validateTriggers, type TriggerDraft } from '@shared/editor-triggers'
import type {
  AppEntry,
  AppPrefs,
  ActionResult,
  BootstrapData,
  ChromeBookmarksPluginPage,
  ClipboardPluginPage,
  EventName,
  EditorVariant,
  EditorWorkspace,
  ExternalPluginItem,
  FlashcastApi,
  FlashcastEvents,
  GenericPluginSettingsPage,
  PluginInspection,
  PluginPage,
  PluginSettingsPage,
  PluginSummary,
  UpdateState,
  VscodeLikeWorkspacePluginPage,
  VscodeLikeWorkspaceSettingsPage,
  WorkspaceSummary
} from '@shared/types'
import {
  mockApps,
  mockBookmarks,
  mockBootstrap,
  mockChromeProfiles,
  mockChromeSettings,
  mockClipboardConfig,
  mockClipboardItems,
  mockClipboardSettings,
  mockEditorSettings,
  mockEditorVariants,
  mockEditorWorkspaces,
  mockFavicons,
  mockPlugins,
  mockShortcuts,
  mockWorkspace
} from './demo-data'

export function createMockApi(): FlashcastApi {
  const params = new URLSearchParams(window.location.search)
  const role = params.get('window') === 'settings' ? 'settings' : 'launcher'
  const platform = (params.get('platform') as FlashcastApi['platform']) || 'linux'
  const listeners = new Map<string, Set<(payload: unknown) => void>>()

  const initialTheme: 'light' | 'dark' = params.get('theme') === 'light' ? 'light' : 'dark'
  let prefs: AppPrefs = {
    ...mockBootstrap.prefs,
    theme: initialTheme,
    hotkeys: { ...mockBootstrap.prefs.hotkeys }
  }
  let shortcuts = mockShortcuts.map((shortcut) => ({ ...shortcut }))
  let clipboardItems = mockClipboardItems.map((item) => ({ ...item }))
  let plugins = mockPlugins.map((plugin) => ({ ...plugin }))
  let workspace: WorkspaceSummary = { ...mockWorkspace }
  let clipboardMax = mockClipboardConfig.maxItems
  let chromeProfile = 'Default'
  let editorVariants = mockEditorSettings.variants.map((variant) => ({ ...variant }))
  let editorOptions = { ...mockEditorSettings.options }

  /* ------------------------------- updates ------------------------------- */

  const updateScenario = params.get('update') ?? ''
  const RELEASE_NOTES = [
    '- 顶栏新增设置入口，⌘, / Ctrl+, 可直接打开设置',
    '- 检查更新：发现新版本后可一键下载并重启安装',
    '- 修复 macOS 菜单栏图标比系统图标偏大的问题'
  ].join('\n')

  const updateBase = (): UpdateState => ({
    phase: 'idle',
    currentVersion: mockBootstrap.version,
    latestVersion: null,
    releaseName: null,
    releaseNotes: null,
    releaseUrl: null,
    publishedAt: null,
    assetName: null,
    assetSize: null,
    downloadedBytes: 0,
    totalBytes: 0,
    progress: 0,
    downloadPath: null,
    installKind: 'none',
    canInstall: false,
    message: '尚未检查更新',
    error: null,
    checkedAt: null
  })

  const availableState = (): UpdateState => ({
    ...updateBase(),
    phase: 'available',
    latestVersion: '0.3.0',
    releaseName: 'Flashcast 0.3.0',
    releaseNotes: RELEASE_NOTES,
    releaseUrl: 'https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.3.0',
    publishedAt: Date.now() - 1000 * 60 * 60 * 26,
    assetName:
      platform === 'win32'
        ? 'Flashcast.Setup.0.3.0.exe'
        : platform === 'linux'
          ? 'Flashcast-0.3.0.AppImage'
          : 'Flashcast-0.3.0-arm64-mac.zip',
    assetSize: 128_729_462,
    totalBytes: 128_729_462,
    installKind: 'relaunch',
    canInstall: true,
    checkedAt: Date.now() - 1000 * 120,
    message: '发现新版本 v0.3.0'
  })

  const latestState = (): UpdateState => ({
    ...updateBase(),
    phase: 'latest',
    latestVersion: mockBootstrap.version,
    checkedAt: Date.now() - 1000 * 90,
    releaseUrl: 'https://github.com/Plasticine-Yang/flashcast/releases',
    message: `已是最新版本 v${mockBootstrap.version}`
  })

  const initialUpdate = (): UpdateState => {
    if (updateScenario === 'available') return availableState()
    if (updateScenario === 'downloading') {
      const total = 128_729_462
      return {
        ...availableState(),
        phase: 'downloading',
        downloadedBytes: Math.round(total * 0.42),
        progress: 0.42,
        message: '正在下载更新…'
      }
    }
    if (updateScenario === 'ready') {
      return {
        ...availableState(),
        phase: 'ready',
        downloadedBytes: 128_729_462,
        progress: 1,
        downloadPath: '/Users/you/Library/Application Support/Flashcast/updates/Flashcast-0.3.0-arm64-mac.zip',
        message: '更新已就绪，重启后完成安装'
      }
    }
    if (updateScenario === 'latest') return latestState()
    if (updateScenario === 'error') {
      return {
        ...updateBase(),
        phase: 'error',
        checkedAt: Date.now(),
        message: '检查更新失败',
        error: '请求超时，请检查网络连接'
      }
    }
    return updateBase()
  }

  /** Result of an explicit check: without a scenario the preview is up to date. */
  const checkedUpdate = (): UpdateState => (updateScenario ? initialUpdate() : latestState())

  let updateState = initialUpdate()
  let downloadTimer: number | null = null

  const emitUpdate = (next: UpdateState): void => {
    updateState = next
    emit('updates:changed', next)
  }

  const stopDownload = (): void => {
    if (downloadTimer !== null) window.clearInterval(downloadTimer)
    downloadTimer = null
  }

  const startDownload = (): void => {
    stopDownload()
    const total = updateState.totalBytes || 128_729_462
    let downloaded = updateState.phase === 'downloading' ? updateState.downloadedBytes : 0
    emitUpdate({
      ...updateState,
      phase: 'downloading',
      downloadedBytes: downloaded,
      totalBytes: total,
      progress: downloaded / total,
      message: '正在下载更新…',
      error: null
    })
    downloadTimer = window.setInterval(() => {
      downloaded = Math.min(total, downloaded + total / 20)
      const done = downloaded >= total
      if (done) stopDownload()
      emitUpdate({
        ...updateState,
        phase: done ? 'ready' : 'downloading',
        downloadedBytes: downloaded,
        totalBytes: total,
        progress: downloaded / total,
        downloadPath: done
          ? '/Users/you/Library/Application Support/Flashcast/updates/Flashcast-0.3.0-arm64-mac.zip'
          : null,
        installKind: 'relaunch',
        canInstall: true,
        message: done ? '更新已就绪，重启后完成安装' : '正在下载更新…'
      })
    }, 220)
  }

  const bootstrap = (): BootstrapData => ({
    ...mockBootstrap,
    role,
    platform,
    prefs,
    theme: prefs.theme === 'light' ? 'light' : 'dark',
    shortcuts,
    plugins,
    workspace,
    launchAtLoginSupported: true
  })

  const clipboardPage = (): ClipboardPluginPage => ({
    kind: 'clipboard',
    pluginId: 'clipboard',
    title: '剪切板',
    placeholder: '在剪切板中搜索…',
    searchable: true,
    items: clipboardItems,
    maxItems: clipboardMax
  })

  const chromePage = (): ChromeBookmarksPluginPage => ({
    kind: 'bookmarks',
    pluginId: 'chrome-bookmarks',
    title: 'Chrome 书签',
    placeholder: '搜索书签…',
    searchable: true,
    profiles: mockChromeProfiles,
    selectedProfile: chromeProfile,
    userDataDir: '/home/you/.config/google-chrome',
    bookmarkFile: '/home/you/.config/google-chrome/Default/AccountBookmarks',
    bookmarkSource: 'account',
    bookmarks: mockBookmarks,
    error: null
  })

  const enabledEditorIds = (): Set<string> =>
    new Set(editorVariants.filter((variant) => variant.enabled).map((variant) => variant.id))

  const vscodePage = (): VscodeLikeWorkspacePluginPage => {
    const enabled = enabledEditorIds()
    const variants: EditorVariant[] = editorVariants
      .filter((variant) => variant.enabled)
      .map((variant) => ({
        id: variant.id,
        name: variant.name,
        version: variant.version,
        dataDir: variant.dataDir,
        installed: true,
        error: null
      }))
    return {
      kind: 'vscode-like-workspace',
      pluginId: 'vscode-like-workspace',
      title: 'VSCode-like Workspace',
      placeholder: '搜索工作区…',
      searchable: true,
      variants,
      workspaces: mockEditorWorkspaces.filter((workspace) => enabled.has(workspace.variantId)),
      errors: [],
      scannedAt: Date.now()
    }
  }

  const editorTriggers = (): PluginSummary['triggers'] =>
    editorVariants
      .filter((variant) => variant.enabled && variant.trigger.trim())
      .map((variant) => ({
        keyword: variant.trigger.trim().toLowerCase(),
        label: `${variant.name} 工作区`,
        args: { editor: variant.id }
      }))

  const editorSettingsPage = (): VscodeLikeWorkspaceSettingsPage => ({
    ...mockEditorSettings,
    variants: editorVariants.map((variant) => ({ ...variant })),
    options: { ...editorOptions },
    triggerErrors: {},
    savedAt: Date.now()
  })

  const settingsPage = (pluginId: string): PluginSettingsPage => {
    if (pluginId === 'vscode-like-workspace') return editorSettingsPage()
    if (pluginId === 'chrome-bookmarks') {
      return {
        ...mockChromeSettings,
        fields: mockChromeSettings.fields.map((field) =>
          field.key === 'profile' && field.type === 'select' ? { ...field, value: chromeProfile } : field
        ) as GenericPluginSettingsPage['fields'],
        savedAt: Date.now()
      }
    }
    return {
      ...mockClipboardSettings,
      fields: mockClipboardSettings.fields.map((field) =>
        field.key === 'maxItems' && field.type === 'number' ? { ...field, value: clipboardMax } : field
      ) as GenericPluginSettingsPage['fields'],
      savedAt: Date.now()
    }
  }

  const emit = <K extends EventName>(event: K, payload: FlashcastEvents[K]): void => {
    for (const handler of listeners.get(event) ?? []) handler(payload)
  }

  const noop: FlashcastApi['openSettings'] = async () => {}

  return {
    platform,
    getBootstrap: async () => bootstrap(),
    getApps: async () => mockApps,
    refreshApps: async () => mockApps,

    getPluginPage: async (pluginId): Promise<PluginPage> => {
      if (pluginId === 'clipboard') return clipboardPage()
      if (pluginId === 'chrome-bookmarks') return chromePage()
      if (pluginId === 'vscode-like-workspace') return vscodePage()
      return {
        kind: 'external',
        pluginId,
        title: pluginId,
        placeholder: '搜索…',
        searchable: true,
        items: [],
        error: '示例插件未提供页面'
      }
    },
    getPluginSettings: async (pluginId, _options) => settingsPage(pluginId),
    savePluginSettings: async (pluginId, patch) => {
      if (pluginId === 'vscode-like-workspace') {
        const rawVariants =
          patch.variants && typeof patch.variants === 'object' && !Array.isArray(patch.variants)
            ? (patch.variants as Record<string, { enabled?: boolean; trigger?: string }>)
            : {}
        const current: Record<string, TriggerDraft> = {}
        const drafts: Record<string, TriggerDraft> = {}
        for (const variant of editorVariants) {
          current[variant.id] = { enabled: variant.enabled, trigger: variant.trigger }
          const raw = rawVariants[variant.id]
          drafts[variant.id] = {
            enabled: typeof raw?.enabled === 'boolean' ? raw.enabled : variant.enabled,
            trigger: typeof raw?.trigger === 'string' ? raw.trigger : variant.trigger
          }
        }
        const others = new Set<string>()
        for (const plugin of plugins) {
          if (plugin.id === 'vscode-like-workspace') continue
          for (const trigger of plugin.triggers ?? []) others.add(trigger.keyword.toLowerCase())
        }
        const { accepted, errors } = validateTriggers(
          editorVariants.map((variant) => variant.id),
          drafts,
          current,
          others
        )
        if (typeof patch.includeRemote === 'boolean') editorOptions.includeRemote = patch.includeRemote
        if (typeof patch.hideMissing === 'boolean') editorOptions.hideMissing = patch.hideMissing
        if (typeof patch.maxItems === 'number' && Number.isFinite(patch.maxItems)) {
          editorOptions.maxItems = Math.min(5000, Math.max(100, Math.round(patch.maxItems)))
        }
        if (Object.keys(errors).length > 0) {
          return {
            page: {
              ...editorSettingsPage(),
              variants: editorVariants.map((variant) => {
                const draft = accepted[variant.id] ?? current[variant.id]
                return {
                  ...variant,
                  enabled: draft.enabled,
                  trigger: normalizeTrigger(drafts[variant.id]?.trigger ?? variant.trigger)
                }
              }),
              triggerErrors: errors
            },
            savedAt: 0
          }
        }
        editorVariants = editorVariants.map((variant) => {
          const draft = accepted[variant.id]
          return draft ? { ...variant, enabled: draft.enabled, trigger: draft.trigger } : variant
        })
        plugins = plugins.map((plugin) =>
          plugin.id === 'vscode-like-workspace'
            ? { ...plugin, triggers: editorTriggers() }
            : plugin
        )
        emit('plugins:changed', plugins)
        return { page: editorSettingsPage(), savedAt: Date.now() }
      }
      if (pluginId === 'clipboard') {
        const value = Number(patch.maxItems)
        if (Number.isFinite(value)) clipboardMax = Math.min(50, Math.max(1, Math.round(value)))
        clipboardItems = clipboardItems.slice(0, clipboardMax)
      }
      if (pluginId === 'chrome-bookmarks' && typeof patch.profile === 'string') {
        chromeProfile = patch.profile
      }
      const page = settingsPage(pluginId)
      page.savedAt = Date.now()
      return { page, savedAt: Date.now() }
    },
    searchExternal: async (): Promise<ExternalPluginItem[]> => [],
    activateExternal: async (): Promise<ActionResult> => ({ ok: true }),

    activateClipboard: async (itemId, options) => {
      const index = clipboardItems.findIndex((item) => item.id === itemId)
      if (index < 0) return { ok: false, error: '剪切板项不存在' }
      const item = clipboardItems[index]
      clipboardItems = [{ ...item, createdAt: Date.now() }, ...clipboardItems.filter((entry) => entry.id !== itemId)]
      return { ok: true, close: true, paste: options.paste }
    },
    deleteClipboard: async (itemId) => {
      clipboardItems = clipboardItems.filter((item) => item.id !== itemId)
      emit('clipboard:changed', clipboardItems)
      return { ok: true }
    },
    clearClipboard: async () => {
      clipboardItems = []
      emit('clipboard:changed', clipboardItems)
      return { ok: true, toast: { message: '已清空剪切板记录', tone: 'success' } }
    },

    openChromeBookmark: async () => ({ ok: true, close: true }),
    openEditorWorkspace: async () => ({ ok: true, close: true }),
    revealEditorWorkspace: async () => ({ ok: true }),
    getFavicons: async (domains) => mockFavicons(domains),
    openSettings: noop,
    hideLauncher: () => {
      // Preview harness: keep the panel mounted so screenshots stay stable.
    },

    launchApp: async () => ({ ok: true, close: true }),
    runCommand: async (commandId) => {
      if (commandId === 'refresh-apps') {
        return { ok: true, toast: { message: '已重新加载应用列表', tone: 'success' } }
      }
      if (commandId === 'check-updates') {
        emitUpdate({ ...updateState, phase: 'checking', message: '正在检查更新…', error: null })
        await new Promise((resolve) => window.setTimeout(resolve, 500))
        const next = checkedUpdate()
        emitUpdate(next)
        if (next.phase === 'available') {
          return {
            ok: true,
            toast: { message: `发现新版本 v${next.latestVersion}，可用右上角的更新按钮一键安装`, tone: 'warning' }
          }
        }
        if (next.phase === 'latest') {
          return { ok: true, toast: { message: `当前已是最新版本 v${next.currentVersion}`, tone: 'success' } }
        }
        return { ok: false, error: next.error ?? next.message }
      }
      return { ok: true }
    },

    updatePrefs: async (patch) => {
      prefs = { ...prefs, ...patch }
      if (typeof patch.autoCommit === 'boolean') {
        workspace = { ...workspace, autoCommit: patch.autoCommit }
      }
      emit('theme:changed', {
        theme: prefs.theme === 'light' ? 'light' : 'dark',
        setting: prefs.theme
      })
      emit('prefs:changed', prefs)
      return bootstrap()
    },
    chooseWorkspace: async () => ({ canceled: false, workspace }),
    openWorkspaceFolder: async () => {},
    refreshWorkspace: async () => workspace,
    getGitDiff: async () => ({
      status: ' M config.json\n?? plugins/clipboard/config.json',
      diff: ' config.json         | 3 +--\n plugins/.../config.json | 2 ++\n 2 files changed, 3 insertions(+), 2 deletions(-)',
      error: null
    }),

    inspectPlugin: async (input): Promise<PluginInspection> => ({
      token: 'demo-token',
      manifest: {
        id: 'demo-plugin',
        name: '示例插件',
        description: '用于展示安装确认卡片的示例插件',
        version: '1.2.0',
        keywords: ['demo', '示例'],
        iconKey: 'puzzle',
        searchable: true,
        primaryAction: '打开',
        builtin: false,
        hasSettings: true,
        permissions: ['剪贴板读取'],
        layout: 'compact'
      },
      sourceType: input.startsWith('http') ? 'git' : 'local',
      source: input,
      permissions: ['剪贴板读取'],
      entry: 'index.js',
      fileCount: 12,
      size: 48_200,
      alreadyInstalled: false,
      installedVersion: null,
      warning: '插件将在本机以当前用户权限运行，请仅安装可信来源'
    }),
    installPlugin: async () => {
      const plugin: PluginSummary = {
        id: 'demo-plugin',
        name: '示例插件',
        description: '用于展示安装确认卡片的示例插件',
        version: '1.2.0',
        keywords: ['demo', '示例'],
        iconKey: 'puzzle',
        searchable: true,
        primaryAction: '打开',
        builtin: false,
        hasSettings: true,
        permissions: ['剪贴板读取'],
        layout: 'compact',
        enabled: true,
        status: 'ok',
        source: 'git'
      }
      plugins = [...plugins, plugin]
      return { plugin, plugins }
    },
    updatePlugin: async () => ({ ok: true, toast: { message: '已更新插件', tone: 'success' } }),
    uninstallPlugin: async (pluginId) => {
      plugins = plugins.filter((plugin) => plugin.id !== pluginId)
      emit('plugins:changed', plugins)
      return { ok: true }
    },
    setPluginEnabled: async (pluginId, enabled) => {
      plugins = plugins.map((plugin) => (plugin.id === pluginId ? { ...plugin, enabled } : plugin))
      emit('plugins:changed', plugins)
      return { ok: true }
    },
    openPluginDirectory: async () => {},

    setShortcuts: async (patch) => {
      shortcuts = shortcuts.map((shortcut) =>
        shortcut.id in patch ? { ...shortcut, accelerator: String(patch[shortcut.id]) } : shortcut
      )
      emit('shortcuts:changed', { shortcuts, errors: [] })
      return { shortcuts, errors: [] }
    },
    resetShortcuts: async () => {
      shortcuts = shortcuts.map((shortcut) => ({
        ...shortcut,
        accelerator: shortcut.defaultAccelerator
      }))
      emit('shortcuts:changed', { shortcuts, errors: [] })
      return { shortcuts, errors: [] }
    },
    getUpdateState: async () => updateState,
    checkForUpdates: async () => {
      emitUpdate({ ...updateState, phase: 'checking', message: '正在检查更新…', error: null })
      await new Promise((resolve) => window.setTimeout(resolve, 620))
      const next = checkedUpdate()
      emitUpdate(next)
      return next
    },
    downloadUpdate: async () => {
      startDownload()
      return updateState
    },
    cancelUpdate: async () => {
      stopDownload()
      emitUpdate({
        ...updateState,
        phase: 'available',
        downloadedBytes: 0,
        progress: 0,
        downloadPath: null,
        message: '已取消下载'
      })
      return updateState
    },
    installUpdate: async () => ({
      ok: true,
      toast: { message: '正在重启并安装更新（预览模式不会真的重启）', tone: 'success' }
    }),
    openReleasePage: async () => {},
    showUpdateFile: async () => {},
    quit: () => {},

    on(event, handler) {
      if (!listeners.has(event)) listeners.set(event, new Set())
      listeners.get(event)!.add(handler as (payload: unknown) => void)
      return () => listeners.get(event)?.delete(handler as (payload: unknown) => void)
    }
  }
}
