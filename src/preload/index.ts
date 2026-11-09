import { contextBridge, ipcRenderer } from 'electron'
import { IPC } from '@shared/ipc'
import type {
  AppEntry,
  ActionResult,
  BootstrapData,
  ChromeProfile,
  EditorCleanupResult,
  EditorWorkspaceSnapshot,
  EventName,
  ExternalPluginItem,
  FlashcastApi,
  FlashcastEvents,
  PluginInspection,
  PluginPage,
  PluginSettingsPage,
  PluginSummary,
  PlatformId,
  ShortcutError,
  ShortcutInfo,
  ThemeInspection,
  ThemePackError,
  UpdateState,
  WorkspaceSummary
} from '@shared/types'
import type { ThemeDefinition } from '@shared/theme'

const platform = process.platform as PlatformId

async function invoke<T>(channel: string, ...args: unknown[]): Promise<T> {
  try {
    return (await ipcRenderer.invoke(channel, ...args)) as T
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error)
    throw new Error(message.replace(/^Error invoking remote method '[^']+': (Error: )?/, ''))
  }
}

const eventNames = new Set<EventName>([
  'launcher:shown',
  'launcher:close-request',
  'launcher:open-plugin',
  'clipboard:changed',
  'plugins:changed',
  'theme:changed',
  'themes:changed',
  'workspace:changed',
  'shortcuts:changed',
  'toast',
  'prefs:changed',
  'updates:changed'
])

const api: FlashcastApi = {
  platform,

  getBootstrap: () => invoke<BootstrapData>(IPC.bootstrap),
  getApps: () => invoke<AppEntry[]>(IPC.getApps),
  refreshApps: () => invoke<AppEntry[]>(IPC.refreshApps),

  getPluginPage: (pluginId) => invoke<PluginPage>(IPC.getPluginPage, pluginId),
  getPluginSettings: (pluginId, options) =>
    invoke<PluginSettingsPage>(IPC.getPluginSettings, pluginId, options),
  savePluginSettings: (pluginId, patch) =>
    invoke<{ page: PluginSettingsPage; savedAt: number }>(IPC.savePluginSettings, pluginId, patch),
  searchExternal: (pluginId, query) =>
    invoke<ExternalPluginItem[]>(IPC.searchExternal, pluginId, query),
  activateExternal: (pluginId, itemId, modifiers) =>
    invoke<ActionResult>(IPC.activateExternal, pluginId, itemId, modifiers),

  activateClipboard: (itemId, options) =>
    invoke<ActionResult>(IPC.activateClipboard, itemId, options),
  deleteClipboard: (itemId) => invoke<ActionResult>(IPC.deleteClipboard, itemId),
  clearClipboard: () => invoke<ActionResult>(IPC.clearClipboard),

  openChromeBookmark: (bookmark) => invoke<ActionResult>(IPC.openChromeBookmark, bookmark),
  openEditorWorkspace: (workspaceId, options) =>
    invoke<ActionResult>(IPC.openEditorWorkspace, workspaceId, options),
  revealEditorWorkspace: (workspaceId) =>
    invoke<ActionResult>(IPC.revealEditorWorkspace, workspaceId),
  getEditorWorkspaces: () =>
    invoke<EditorWorkspaceSnapshot[]>(IPC.editorWorkspaces),
  deleteEditorWorkspaces: (variantId, ids) =>
    invoke<EditorCleanupResult>(IPC.deleteEditorWorkspaces, variantId, ids),
  getFavicons: (domains) => invoke<Record<string, string>>(IPC.getFavicons, domains),
  openSystemSettings: (pane) => invoke<void>(IPC.openSystemSettings, pane),
  openSettings: () => invoke<void>(IPC.openSettings),
  hideLauncher: (options) => ipcRenderer.send(IPC.hideLauncher, options),

  launchApp: (appId) => invoke<ActionResult>(IPC.launchApp, appId),
  runCommand: (commandId) => invoke<ActionResult>(IPC.runCommand, commandId),

  updatePrefs: (patch) => invoke<BootstrapData>(IPC.updatePrefs, patch),
  chooseWorkspace: () =>
    invoke<{ canceled: boolean; workspace: WorkspaceSummary }>(IPC.chooseWorkspace),
  openWorkspaceFolder: () => invoke<void>(IPC.openWorkspaceFolder),
  refreshWorkspace: () => invoke<WorkspaceSummary>(IPC.refreshWorkspace),
  getGitDiff: () => invoke<{ status: string; diff: string; error: string | null }>(IPC.gitDiff),

  inspectTheme: (input) => invoke<ThemeInspection>(IPC.themesInspect, input),
  installTheme: (token) =>
    invoke<{ theme: ThemeDefinition; themes: ThemeDefinition[]; errors: ThemePackError[] }>(
      IPC.themesInstall,
      token
    ),
  updateTheme: (themeId) => invoke<ActionResult>(IPC.themesUpdate, themeId),
  uninstallTheme: (themeId) => invoke<ActionResult>(IPC.themesUninstall, themeId),
  openThemeDirectory: (themeId) => invoke<void>(IPC.themesOpenDirectory, themeId),
  reloadThemes: () =>
    invoke<{ themes: ThemeDefinition[]; errors: ThemePackError[] }>(IPC.themesReload),

  inspectPlugin: (input) => invoke<PluginInspection>(IPC.inspectPlugin, input),
  installPlugin: (token) =>
    invoke<{ plugin: PluginSummary; plugins: PluginSummary[] }>(IPC.installPlugin, token),
  updatePlugin: (pluginId) => invoke<ActionResult>(IPC.updatePlugin, pluginId),
  uninstallPlugin: (pluginId) => invoke<ActionResult>(IPC.uninstallPlugin, pluginId),
  setPluginEnabled: (pluginId, enabled) =>
    invoke<ActionResult>(IPC.setPluginEnabled, pluginId, enabled),
  openPluginDirectory: (pluginId) => invoke<void>(IPC.openPluginDirectory, pluginId),

  setShortcuts: (patch) =>
    invoke<{ shortcuts: ShortcutInfo[]; errors: ShortcutError[] }>(IPC.setShortcuts, patch),
  resetShortcuts: () =>
    invoke<{ shortcuts: ShortcutInfo[]; errors: ShortcutError[] }>(IPC.resetShortcuts),
  getUpdateState: () => invoke<UpdateState>(IPC.updateState),
  checkForUpdates: () => invoke<UpdateState>(IPC.updateCheck),
  downloadUpdate: () => invoke<UpdateState>(IPC.updateDownload),
  cancelUpdate: () => invoke<UpdateState>(IPC.updateCancel),
  installUpdate: () => invoke<ActionResult>(IPC.updateInstall),
  openReleasePage: () => invoke<void>(IPC.updateOpenRelease),
  showUpdateFile: () => invoke<void>(IPC.updateShowFile),
  quit: () => ipcRenderer.send(IPC.quit),

  on<K extends EventName>(event: K, handler: (payload: FlashcastEvents[K]) => void): () => void {
    if (!eventNames.has(event)) throw new Error(`Unknown event: ${event}`)
    const listener = (_event: Electron.IpcRendererEvent, payload: FlashcastEvents[K]): void => {
      handler(payload)
    }
    ipcRenderer.on(event, listener)
    return () => ipcRenderer.removeListener(event, listener)
  }
}

contextBridge.exposeInMainWorld('flashcast', api)

