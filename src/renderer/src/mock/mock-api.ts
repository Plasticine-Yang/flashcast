import { normalizeTrigger, validateTriggers, type TriggerDraft } from '@shared/editor-triggers'
import {
  BLUE_GLASS_THEME,
  defineTheme,
  resolveThemeState,
  supportedThemeModes,
  type ThemeDefinition
} from '@shared/theme'
import { BUILTIN_THEMES } from '@shared/themes'
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
  mockEditorWorkspaces as demoEditorWorkspaces,
  mockFavicons,
  mockPlugins,
  mockShortcuts,
  mockWorkspace
} from './demo-data'

const DEMO_THEME = defineTheme(
  {
    schemaVersion: 1,
    id: 'demo-harbor',
    name: 'Harbor',
    description: '预览用的海湾青主题包。',
    version: '1.0.0',
    author: 'Flashcast Preview',
    license: 'MIT',
    application: 'generated-tokens',
    variants: {
      dark: {
        label: 'Night',
        palette: {
          bg: '#0d1b1e',
          bgElevated: '#15282c',
          surface: '#5fc7c9',
          border: '#9ed6d8',
          text: '#eef8f7',
          textSecondary: '#bfdcdb',
          textTertiary: '#a5c5c4',
          accent: '#5fc7c9',
          onAccent: '#0d1b1e',
          success: '#7fd18b',
          warning: '#e8c07a',
          danger: '#ff8a80',
          focus: '#8ee6e9'
        }
      },
      light: {
        label: 'Day',
        palette: {
          bg: '#f2f8f7',
          bgElevated: '#e4efee',
          surface: '#0e5f63',
          border: '#20484c',
          text: '#10292b',
          textSecondary: '#31585b',
          textTertiary: '#3f6468',
          accent: '#0e6b6f',
          onAccent: '#ffffff',
          success: '#2f6b3f',
          warning: '#7a4a00',
          danger: '#a33a2a',
          focus: '#0e6b6f'
        }
      }
    }
  },
  'workspace'
)

export function createMockApi(): FlashcastApi {
  const params = new URLSearchParams(window.location.search)
  const role = params.get('window') === 'settings' ? 'settings' : 'launcher'
  const platform = (params.get('platform') as FlashcastApi['platform']) || 'linux'
  const listeners = new Map<string, Set<(payload: unknown) => void>>()

  const initialTheme: 'light' | 'dark' = params.get('theme') === 'light' ? 'light' : 'dark'
  const initialThemeId = params.get('themeId')?.trim() || 'blue-glass'
  let themes: ThemeDefinition[] = [...BUILTIN_THEMES]
  let prefs: AppPrefs = {
    ...mockBootstrap.prefs,
    theme: initialTheme,
    themeId: initialThemeId,
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
  let mockEditorWorkspaces = demoEditorWorkspaces.map((workspace) => ({ ...workspace }))

  /* ------------------------------- updates ------------------------------- */

  const updateScenario = params.get('update') ?? ''
  const nextMockVersion = (() => {
    const [major = 0, minor = 0, patch = 0] = __FLASHCAST_VERSION__.split('.').map(
      (part) => Number.parseInt(part, 10) || 0
    )
    return `${major}.${minor}.${patch + 1}`
  })()
  const RELEASE_NOTES = [
    '- 新增工作区主题包与外观设置（六套内置主题、深浅模式）',
    '- VSCode-like Workspace 支持按编辑器维护触发词与清理历史',
    '- 改进启动器列表动画与减少动效适配'
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
    latestVersion: nextMockVersion,
    releaseName: `Flashcast ${nextMockVersion}`,
    releaseNotes: RELEASE_NOTES,
    releaseUrl: `https://github.com/Plasticine-Yang/flashcast/releases/tag/v${nextMockVersion}`,
    publishedAt: Date.now() - 1000 * 60 * 60 * 26,
    assetName:
      platform === 'win32'
        ? `Flashcast.Setup.${nextMockVersion}.exe`
        : platform === 'linux'
          ? `Flashcast-${nextMockVersion}.AppImage`
          : `Flashcast-${nextMockVersion}-arm64-mac.zip`,
    assetSize: 128_729_462,
    totalBytes: 128_729_462,
    installKind: 'relaunch',
    canInstall: true,
    checkedAt: Date.now() - 1000 * 120,
    message: `发现新版本 v${nextMockVersion}`
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
        downloadPath: `/Users/you/Library/Application Support/Flashcast/updates/Flashcast-${nextMockVersion}-arm64-mac.zip`,
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
          ? `/Users/you/Library/Application Support/Flashcast/updates/Flashcast-${nextMockVersion}-arm64-mac.zip`
          : null,
        installKind: 'relaunch',
        canInstall: true,
        message: done ? '更新已就绪，重启后完成安装' : '正在下载更新…'
      })
    }, 220)
  }

  const themeState = (): {
    state: ReturnType<typeof resolveThemeState>
    definition: ThemeDefinition
    fallbackReason: string | null
  } => {
    const requested = themes.find(
      (theme) => theme.id === prefs.themeId && supportedThemeModes(theme).length > 0
    )
    const state = resolveThemeState(requested, prefs.theme, 'dark', BLUE_GLASS_THEME)
    const definition = themes.find((theme) => theme.id === state.themeId) ?? BLUE_GLASS_THEME
    const fallbackReason =
      state.themeId === prefs.themeId
        ? null
        : `主题「${prefs.themeId}」不可用，已回退 Blue Glass`
    return { state, definition, fallbackReason }
  }

  const emitThemeState = (): void => {
    const { state, definition, fallbackReason } = themeState()
    emit('theme:changed', {
      theme: state.mode,
      setting: prefs.theme,
      themeId: state.themeId,
      locked: state.locked,
      supportedModes: state.supportedModes,
      definition,
      requestedThemeId: prefs.themeId,
      fallbackReason
    })
    emit('themes:changed', { themes, errors: [] })
  }

  const bootstrap = (): BootstrapData => {
    const { state, fallbackReason } = themeState()
    return {
      ...mockBootstrap,
      role,
      platform,
      prefs,
      theme: state.mode,
      themeId: state.themeId,
      themeLocked: state.locked,
      themeSupportedModes: state.supportedModes,
      themeFallback:
        fallbackReason === null
          ? null
          : { requestedId: prefs.themeId, reason: fallbackReason },
      themes,
      themeErrors: [],
      shortcuts,
      plugins,
      workspace,
      launchAtLoginSupported: true
    }
  }

  const clipboardPage = (): ClipboardPluginPage => ({
    kind: 'clipboard',
    pluginId: 'clipboard',
    title: '剪切板',
    placeholder: '在剪切板中搜索…',
    searchable: true,
    items: clipboardItems,
    maxItems: clipboardMax
  })

  // `?chrome=permission|none` previews the two failure cards without a Mac.
  const chromeScenario = params.get('chrome') ?? ''

  const chromePage = (): ChromeBookmarksPluginPage => {
    if (chromeScenario === 'permission' || chromeScenario === 'none') {
      const permissionDenied = chromeScenario === 'permission'
      return {
        kind: 'bookmarks',
        pluginId: 'chrome-bookmarks',
        title: 'Chrome 书签',
        placeholder: '搜索书签…',
        searchable: true,
        profiles: [],
        selectedProfile: null,
        userDataDir: '/Users/you/Library/Application Support/Google/Chrome',
        bookmarkFile: null,
        bookmarkSource: null,
        bookmarks: [],
        error: permissionDenied ? '无法读取 Chrome 数据目录' : '未找到 Chrome Profile',
        errorCode: permissionDenied ? 'permission-denied' : 'no-profile'
      }
    }
    return {
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
      error: null,
      errorCode: null
    }
  }

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

  const editorWorkspaces = (variantId: string): EditorWorkspace[] =>
    mockEditorWorkspaces.filter((workspace) => workspace.variantId === variantId)

  const editorStats = (): VscodeLikeWorkspaceSettingsPage['workspaceStats'] =>
    Object.fromEntries(
      editorVariants.map((variant) => {
        const list = editorWorkspaces(variant.id)
        return [
          variant.id,
          {
            total: list.length,
            missing: list.filter((workspace) => !workspace.exists).length,
            remote: list.filter((workspace) => workspace.remote !== null).length,
            running: variant.id === 'trae-cn'
          }
        ]
      })
    )

  const editorSettingsPage = (): VscodeLikeWorkspaceSettingsPage => ({
    ...mockEditorSettings,
    variants: editorVariants.map((variant) => ({ ...variant })),
    options: { ...editorOptions },
    triggerErrors: {},
    workspaceStats: editorStats(),
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
  const noopWithPane: FlashcastApi['openSystemSettings'] = async () => {}

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
    getEditorWorkspaces: async () =>
      editorVariants.map((variant) => ({
        variantId: variant.id,
        name: variant.name,
        running: variant.id === 'trae-cn',
        workspaces: editorWorkspaces(variant.id).map((workspace) => ({ ...workspace })),
        error: null
      })),
    deleteEditorWorkspaces: async (variantId, ids) => {
      const wanted = new Set(ids)
      const removed = mockEditorWorkspaces.filter(
        (workspace) => workspace.variantId === variantId && wanted.has(workspace.id)
      )
      if (removed.length === 0) {
        return { ok: false, deleted: 0, running: false, backupDir: null, error: '没有可删除的工作区' }
      }
      if (variantId === 'trae-cn') {
        return {
          ok: false,
          deleted: 0,
          running: true,
          backupDir: null,
          error: 'Trae CN 正在运行，请先退出再删除'
        }
      }
      mockEditorWorkspaces = mockEditorWorkspaces.filter(
        (workspace) => !(workspace.variantId === variantId && wanted.has(workspace.id))
      )
      return {
        ok: true,
        deleted: removed.length,
        running: false,
        backupDir: '/Users/you/.flashcast/state/editor-backups/code-1700000000000',
        toast: {
          message: `已从 ${variantId} 历史中删除 ${removed.length} 个工作区`,
          tone: 'success'
        }
      }
    },
    getFavicons: async (domains) => mockFavicons(domains),
    openSystemSettings: noopWithPane,
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
      if (patch.themeId !== undefined) {
        const themeId = typeof patch.themeId === 'string' ? patch.themeId.trim() : ''
        if (!themeId || !themes.some((theme) => theme.id === themeId)) {
          throw new Error('主题不存在或不可用')
        }
      }
      prefs = { ...prefs, ...patch }
      if (typeof patch.autoCommit === 'boolean') {
        workspace = { ...workspace, autoCommit: patch.autoCommit }
      }
      emitThemeState()
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

    inspectTheme: async () => ({
      token: 'demo-theme-token',
      definition: DEMO_THEME,
      sourceType: 'local',
      source: '/demo/flashcast-theme-harbor',
      warnings: [],
      alreadyInstalled: themes.some((theme) => theme.id === DEMO_THEME.id),
      installedVersion: themes.find((theme) => theme.id === DEMO_THEME.id)?.version ?? null
    }),
    installTheme: async () => {
      themes = themes.some((theme) => theme.id === DEMO_THEME.id)
        ? themes
        : [...themes, DEMO_THEME]
      emit('themes:changed', { themes, errors: [] })
      return { theme: DEMO_THEME, themes, errors: [] }
    },
    updateTheme: async (themeId) => {
      if (themeId !== DEMO_THEME.id) {
        return { ok: false, error: '预览模式只有 Harbor 可更新' }
      }
      emit('themes:changed', { themes, errors: [] })
      return { ok: true, toast: { message: 'Harbor 已是最新版本', tone: 'default' } }
    },
    uninstallTheme: async (themeId) => {
      if (themeId !== DEMO_THEME.id) {
        return { ok: false, error: '预览模式只有 Harbor 可卸载' }
      }
      themes = themes.filter((theme) => theme.id !== themeId)
      if (prefs.themeId === themeId) prefs = { ...prefs, themeId: 'blue-glass' }
      emitThemeState()
      emit('prefs:changed', prefs)
      return { ok: true, toast: { message: '已卸载 Harbor', tone: 'success' } }
    },
    openThemeDirectory: async () => {},
    reloadThemes: async () => {
      emit('themes:changed', { themes, errors: [] })
      return { themes, errors: [] }
    },

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
