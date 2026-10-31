import { contextBridge, ipcRenderer } from 'electron'
import { IPC } from '@shared/ipc'
import type {
  AppEntry,
  ActionResult,
  BootstrapData,
  ChromeProfile,
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
  WorkspaceSummary
} from '@shared/types'

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
  'workspace:changed',
  'shortcuts:changed',
  'toast',
  'prefs:changed'
])

const api: FlashcastApi = {
  platform,

  getBootstrap: () => invoke<BootstrapData>(IPC.bootstrap),
  getApps: () => invoke<AppEntry[]>(IPC.getApps),
  refreshApps: () => invoke<AppEntry[]>(IPC.refreshApps),

  getPluginPage: (pluginId) => invoke<PluginPage>(IPC.getPluginPage, pluginId),
  getPluginSettings: (pluginId) => invoke<PluginSettingsPage>(IPC.getPluginSettings, pluginId),
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
  getFavicons: (domains) => invoke<Record<string, string>>(IPC.getFavicons, domains),
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
  checkUpdates: () => invoke<{
    currentVersion: string
    latestVersion: string | null
    updateAvailable: boolean
    message: string
  }>(IPC.checkUpdates),
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

