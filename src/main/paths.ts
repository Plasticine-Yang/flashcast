import { app } from 'electron'
import { existsSync, readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'

export interface LauncherState {
  activeWorkspace?: string
  hasLaunched?: boolean
}

export function userDataDir(): string {
  return app.getPath('userData')
}

export function launcherStatePath(): string {
  return join(userDataDir(), 'launcher-state.json')
}

export function readLauncherState(): LauncherState {
  try {
    return JSON.parse(readFileSync(launcherStatePath(), 'utf8')) as LauncherState
  } catch {
    return {}
  }
}

export function defaultWorkspacePath(): string {
  return join(homedir(), '.flashcast')
}

export function resolveInitialWorkspace(): string {
  const envPath = process.env.FLASHCAST_WORKSPACE?.trim()
  if (envPath) return envPath
  const state = readLauncherState()
  if (state.activeWorkspace && existsSync(state.activeWorkspace)) return state.activeWorkspace
  return defaultWorkspacePath()
}

export function devSandboxPath(...parts: string[]): string {
  return join(app.getAppPath(), '.dev-data', ...parts)
}

export function assetsDir(): string {
  if (app.isPackaged) return join(process.resourcesPath, 'build')
  // In development __dirname points at out/main; assets live at the project root.
  return join(__dirname, '../../build')
}

export function isDev(): boolean {
  return !app.isPackaged
}

export function appVersion(): string {
  try {
    const pkg = JSON.parse(readFileSync(join(__dirname, '../../package.json'), 'utf8')) as {
      version?: string
    }
    if (pkg.version) return pkg.version
  } catch {
    // fall through
  }
  return app.getVersion()
}

/** Runtime file layout inside a git-backed workspace. */
export function workspaceLayout(root: string) {
  return {
    root,
    configFile: join(root, 'config.json'),
    pluginsDir: join(root, 'plugins'),
    pluginDir: (id: string) => join(root, 'plugins', id),
    pluginConfigFile: (id: string) => join(root, 'plugins', id, 'config.json'),
    extensionsDir: join(root, 'extensions'),
    extensionDir: (id: string) => join(root, 'extensions', id),
    stateDir: join(root, 'state'),
    clipboardDir: join(root, 'state', 'clipboard'),
    clipboardMediaDir: join(root, 'state', 'clipboard', 'media'),
    clipboardHistoryFile: join(root, 'state', 'clipboard', 'history.json'),
    faviconDir: join(root, 'state', 'favicons'),
    installTempDir: join(root, 'state', 'install-temp')
  }
}

export type WorkspaceLayout = ReturnType<typeof workspaceLayout>
