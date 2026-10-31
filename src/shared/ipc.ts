/** All IPC channel names live here so main, preload and tests cannot drift. */
export const IPC = {
  bootstrap: 'app:bootstrap',
  getApps: 'apps:list',
  refreshApps: 'apps:refresh',
  launchApp: 'apps:launch',

  getPluginPage: 'plugins:page',
  getPluginSettings: 'plugins:settings',
  savePluginSettings: 'plugins:save-settings',
  searchExternal: 'plugins:external-search',
  activateExternal: 'plugins:external-activate',
  pluginChanged: 'plugins:changed',

  activateClipboard: 'clipboard:activate',
  deleteClipboard: 'clipboard:delete',
  clearClipboard: 'clipboard:clear',
  clipboardChanged: 'clipboard:changed',

  openChromeBookmark: 'chrome:open-bookmark',
  getFavicons: 'chrome:favicons',

  openSettings: 'launcher:open-settings',
  hideLauncher: 'launcher:hide',
  shown: 'launcher:shown',
  closeRequest: 'launcher:close-request',
  openPlugin: 'launcher:open-plugin',

  runCommand: 'commands:run',
  updatePrefs: 'prefs:update',
  prefsChanged: 'prefs:changed',
  themeChanged: 'theme:changed',
  toast: 'app:toast',

  chooseWorkspace: 'workspace:choose',
  openWorkspaceFolder: 'workspace:open-folder',
  refreshWorkspace: 'workspace:refresh',
  workspaceChanged: 'workspace:changed',
  gitDiff: 'workspace:git-diff',

  inspectPlugin: 'extensions:inspect',
  installPlugin: 'extensions:install',
  updatePlugin: 'extensions:update',
  uninstallPlugin: 'extensions:uninstall',
  setPluginEnabled: 'extensions:set-enabled',
  openPluginDirectory: 'extensions:open-directory',

  setShortcuts: 'shortcuts:set',
  resetShortcuts: 'shortcuts:reset',
  shortcutsChanged: 'shortcuts:changed',

  checkUpdates: 'app:check-updates',
  quit: 'app:quit'
} as const
