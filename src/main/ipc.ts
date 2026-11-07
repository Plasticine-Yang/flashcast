import { app, ipcMain } from 'electron'
import { IPC } from '@shared/ipc'
import type { AppPrefs, CommandEntry, GlobalHotkeys, WindowRole } from '@shared/types'
import type { FlashcastApplication } from './application'
import { log } from './logger'

let registered = false

function roleOf(application: FlashcastApplication, senderId: number): WindowRole {
  return application.getWindows().getRole(senderId) ?? 'launcher'
}

function requireString(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) throw new Error(`${name} 不能为空`)
  return value
}

function asRecord(value: unknown, name: string): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error(`${name} 格式错误`)
  return value as Record<string, unknown>
}

export function registerIpcHandlers(application: FlashcastApplication): void {
  if (registered) return
  registered = true

  ipcMain.handle(IPC.bootstrap, async (event) =>
    application.getBootstrap(roleOf(application, event.sender.id))
  )

  ipcMain.handle(IPC.getApps, async () => application.getApps())
  ipcMain.handle(IPC.refreshApps, async () => application.refreshApps())
  ipcMain.handle(IPC.launchApp, async (_event, appId: unknown) =>
    application.launchApp(requireString(appId, '应用 id'))
  )
  ipcMain.handle(IPC.runCommand, async (_event, commandId: unknown) =>
    application.runCommand(requireString(commandId, '命令') as CommandEntry['id'])
  )

  ipcMain.handle(IPC.getPluginPage, async (_event, pluginId: unknown) =>
    application.getPluginPage(requireString(pluginId, '插件 id'))
  )
  ipcMain.handle(IPC.getPluginSettings, async (_event, pluginId: unknown) =>
    application.getPluginSettings(requireString(pluginId, '插件 id'))
  )
  ipcMain.handle(
    IPC.savePluginSettings,
    async (_event, pluginId: unknown, patch: unknown) =>
      application.savePluginSettings(
        requireString(pluginId, '插件 id'),
        asRecord(patch, '插件配置')
      )
  )
  ipcMain.handle(IPC.searchExternal, async (_event, pluginId: unknown, query: unknown) =>
    application.searchExternal(requireString(pluginId, '插件 id'), String(query ?? ''))
  )
  ipcMain.handle(
    IPC.activateExternal,
    async (_event, pluginId: unknown, itemId: unknown, modifiers: unknown) =>
      application.activateExternal(
        requireString(pluginId, '插件 id'),
        requireString(itemId, '条目 id'),
        asRecord(modifiers ?? {}, '修饰键') as { alt: boolean }
      )
  )

  ipcMain.handle(IPC.activateClipboard, async (_event, itemId: unknown, options: unknown) => {
    const opts = asRecord(options ?? {}, '选项')
    return application.activateClipboard(
      requireString(itemId, '剪切板项 id'),
      { paste: opts.paste === true }
    )
  })
  ipcMain.handle(IPC.deleteClipboard, async (_event, itemId: unknown) =>
    application.deleteClipboard(requireString(itemId, '剪切板项 id'))
  )
  ipcMain.handle(IPC.clearClipboard, async () => application.clearClipboard())

  ipcMain.handle(IPC.openChromeBookmark, async (_event, bookmark: unknown) => {
    const record = asRecord(bookmark, '书签')
    return application.openChromeBookmark({
      id: requireString(record.id, '书签 id'),
      url: requireString(record.url, '书签 URL')
    })
  })
  ipcMain.handle(IPC.getFavicons, async (_event, domains: unknown) => {
    if (!Array.isArray(domains)) return {}
    return application.getFavicons(
      domains.filter((domain): domain is string => typeof domain === 'string')
    )
  })

  ipcMain.handle(IPC.openSettings, async () => {
    application.openSettings()
  })
  ipcMain.on(IPC.hideLauncher, (_event, options: unknown) => {
    const record = options && typeof options === 'object' ? (options as Record<string, unknown>) : {}
    application.hideLauncher({ paste: record.paste === true })
  })

  ipcMain.handle(IPC.updatePrefs, async (_event, patch: unknown) =>
    application.updatePrefs(
      asRecord(patch, '偏好设置') as Partial<
        Pick<AppPrefs, 'theme' | 'launchAtLogin' | 'autoCommit' | 'glassIntensity'>
      >
    )
  )

  ipcMain.handle(IPC.chooseWorkspace, async () => application.chooseWorkspace())
  ipcMain.handle(IPC.openWorkspaceFolder, async () => application.openWorkspaceFolder())
  ipcMain.handle(IPC.refreshWorkspace, async () => application.refreshWorkspace())
  ipcMain.handle(IPC.gitDiff, async () => application.getGitDiff())

  ipcMain.handle(IPC.inspectPlugin, async (_event, input: unknown) =>
    application.inspectPlugin(requireString(input, '插件来源'))
  )
  ipcMain.handle(IPC.installPlugin, async (_event, token: unknown) =>
    application.installPlugin(requireString(token, '安装令牌'))
  )
  ipcMain.handle(IPC.updatePlugin, async (_event, pluginId: unknown) =>
    application.updatePlugin(requireString(pluginId, '插件 id'))
  )
  ipcMain.handle(IPC.uninstallPlugin, async (_event, pluginId: unknown) =>
    application.uninstallPlugin(requireString(pluginId, '插件 id'))
  )
  ipcMain.handle(IPC.setPluginEnabled, async (_event, pluginId: unknown, enabled: unknown) =>
    application.setPluginEnabled(requireString(pluginId, '插件 id'), enabled === true)
  )
  ipcMain.handle(IPC.openPluginDirectory, async (_event, pluginId: unknown) =>
    application.openPluginDirectory(requireString(pluginId, '插件 id'))
  )

  ipcMain.handle(IPC.setShortcuts, async (_event, patch: unknown) =>
    application.setShortcuts(asRecord(patch, '快捷键') as Partial<GlobalHotkeys>)
  )
  ipcMain.handle(IPC.resetShortcuts, async () => application.resetShortcuts())

  ipcMain.handle(IPC.updateState, async () => application.getUpdateState())
  ipcMain.handle(IPC.updateCheck, async () => application.checkForUpdates())
  ipcMain.handle(IPC.updateDownload, async () => application.downloadUpdate())
  ipcMain.handle(IPC.updateCancel, async () => application.cancelUpdate())
  ipcMain.handle(IPC.updateInstall, async () => application.installUpdate())
  ipcMain.handle(IPC.updateOpenRelease, async () => application.openReleasePage())
  ipcMain.handle(IPC.updateShowFile, async () => application.showUpdateFile())
  ipcMain.on(IPC.quit, () => {
    log.info('quit requested from renderer')
    app.quit()
  })
}
