import type {
  AppEntry,
  AppPrefs,
  ActionResult,
  BootstrapData,
  ChromeBookmarksPluginPage,
  ClipboardPluginPage,
  EventName,
  ExternalPluginItem,
  FlashcastApi,
  FlashcastEvents,
  PluginInspection,
  PluginPage,
  PluginSettingsPage,
  PluginSummary,
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

  const settingsPage = (pluginId: string): PluginSettingsPage => {
    if (pluginId === 'chrome-bookmarks') {
      return {
        ...mockChromeSettings,
        fields: mockChromeSettings.fields.map((field) =>
          field.key === 'profile' && field.type === 'select' ? { ...field, value: chromeProfile } : field
        ) as PluginSettingsPage['fields'],
        savedAt: Date.now()
      }
    }
    return {
      ...mockClipboardSettings,
      fields: mockClipboardSettings.fields.map((field) =>
        field.key === 'maxItems' && field.type === 'number' ? { ...field, value: clipboardMax } : field
      ) as PluginSettingsPage['fields'],
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
    getPluginSettings: async (pluginId) => settingsPage(pluginId),
    savePluginSettings: async (pluginId, patch) => {
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
    checkUpdates: async () => ({
      currentVersion: '0.1.0',
      latestVersion: null,
      updateAvailable: false,
      message: '当前为开发构建，未配置更新源'
    }),
    quit: () => {},

    on(event, handler) {
      if (!listeners.has(event)) listeners.set(event, new Set())
      listeners.get(event)!.add(handler as (payload: unknown) => void)
      return () => listeners.get(event)?.delete(handler as (payload: unknown) => void)
    }
  }
}
