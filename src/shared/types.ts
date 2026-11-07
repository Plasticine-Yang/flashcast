/* Shared contract between the Electron main process, the preload bridge and the renderer. */

export type PlatformId = 'darwin' | 'win32' | 'linux'
export type ThemeSetting = 'system' | 'dark' | 'light'
export type ResolvedTheme = 'dark' | 'light'
export type WindowRole = 'launcher' | 'settings'

export interface GlobalHotkeys {
  launcher: string
  openClipboard: string
}

export interface AppPrefs {
  theme: ThemeSetting
  launchAtLogin: boolean
  language: 'zh-CN'
  autoCommit: boolean
  hotkeys: GlobalHotkeys
  enabledPlugins: Record<string, boolean>
}

export interface ShortcutInfo {
  id: keyof GlobalHotkeys
  label: string
  description: string
  accelerator: string
  defaultAccelerator: string
  editable: boolean
}

export interface ShortcutError {
  id: keyof GlobalHotkeys | 'system'
  accelerator: string
  message: string
}

export type GitSyncState = 'synced' | 'dirty' | 'conflict' | 'no-remote' | 'error' | 'not-a-repo'

export interface WorkspaceSummary {
  path: string
  exists: boolean
  isRepo: boolean
  branch: string | null
  lastCommit: { hash: string; message: string; date: number } | null
  dirtyCount: number
  conflict: boolean
  ahead: number
  behind: number
  hasRemote: boolean
  autoCommit: boolean
  syncState: GitSyncState
  lastError: string | null
}

export interface PluginTrigger {
  /** Single token typed before a space to enter the plugin directly. */
  keyword: string
  /** Scoped result title, e.g. `Trae CN 工作区`. */
  label: string
  /** Opaque arguments passed to the plugin page when entered from this trigger. */
  args?: Record<string, string>
}

export interface PluginManifest {
  id: string
  name: string
  description: string
  version: string
  keywords: string[]
  /** Optional scoped entry points; the editor plugin fills these from config. */
  triggers?: PluginTrigger[]
  /** Data URL for external plugins. Built-ins use iconKey. */
  icon?: string
  iconKey?: string
  searchable: boolean
  primaryAction?: string
  builtin: boolean
  hasSettings: boolean
  permissions: string[]
  layout: 'compact' | 'wide'
}

export interface PluginSummary extends PluginManifest {
  enabled: boolean
  status: 'ok' | 'error' | 'disabled'
  error?: string
  source?: 'builtin' | 'local' | 'git'
  installSource?: string
}

export interface CommandEntry {
  id: 'settings' | 'check-updates' | 'refresh-apps' | 'quit'
  name: string
  subtitle: string
  keywords: string[]
  iconKey: 'settings' | 'update' | 'refresh' | 'power'
}

export interface AppEntry {
  id: string
  name: string
  subtitle: string
  keywords: string[]
  icon?: string
  iconKey: 'app'
}

export type ClipboardKind = 'text' | 'image' | 'video' | 'music'

export interface ClipboardItem {
  id: string
  kind: ClipboardKind
  createdAt: number
  /** Text payload for text entries. */
  text?: string
  title: string
  /** Relative path below the clipboard media directory, served over flashcast-media:// */
  mediaPath?: string
  thumbPath?: string
  fileName?: string
  width?: number
  height?: number
  byteSize?: number
  /** Internal: absolute path of the original copied file. Never rendered. */
  sourcePath?: string
  /** Internal: content hash used for de-duplication. */
  fingerprint?: string
}

export interface ClipboardPluginConfig {
  maxItems: number
  [key: string]: unknown
}

export interface ChromePluginConfig {
  profile: string | null
  [key: string]: unknown
}

export interface ChromeProfile {
  dir: string
  name: string
  bookmarksPath: string
}

export interface ChromeBookmark {
  id: string
  title: string
  url: string
  folder: string[]
  /** Domain used to key favicon requests. */
  domain: string
}

/**
 * Which file Chrome keeps the profile's bookmarks in: `Bookmarks` for local
 * bookmarks, `AccountBookmarks` once the profile saves bookmarks in the
 * signed-in Google Account.
 */
export type ChromeBookmarkSource = 'local' | 'account'

export interface ClipboardPluginPage {
  kind: 'clipboard'
  pluginId: string
  title: string
  placeholder: string
  searchable: boolean
  items: ClipboardItem[]
  maxItems: number
}

export interface ChromeBookmarksPluginPage {
  kind: 'bookmarks'
  pluginId: string
  title: string
  placeholder: string
  searchable: boolean
  profiles: ChromeProfile[]
  selectedProfile: string | null
  userDataDir: string
  bookmarkFile: string | null
  bookmarkSource: ChromeBookmarkSource | null
  bookmarks: ChromeBookmark[]
  error: string | null
}

export interface ExternalPluginItem {
  id: string
  title: string
  subtitle?: string
  badge?: string
  icon?: string
  iconKey?: string
}

export interface ExternalPluginPage {
  kind: 'external'
  pluginId: string
  title: string
  placeholder: string
  searchable: boolean
  items: ExternalPluginItem[]
  error: string | null
  emptyTitle?: string
  emptyDescription?: string
}

/* ------------------------------------------------------------------ */
/* VS Code-like editors                                                */
/* ------------------------------------------------------------------ */

export interface EditorVariant {
  id: string
  name: string
  version: string
  dataDir: string
  installed: boolean
  error: string | null
}

export interface EditorWorkspaceRemote {
  authority: string
  host: string
  label: string
}

export interface EditorWorkspace {
  id: string
  variantId: string
  name: string
  path: string
  /** Home-abbreviated local path, or `host:path` for remote workspaces. */
  displayPath: string
  uri: string
  kind: 'folder' | 'workspace'
  remote: EditorWorkspaceRemote | null
  isOpen: boolean
  lastActiveAt: number
  exists: boolean
}

export interface VscodeLikeWorkspacePluginPage {
  kind: 'vscode-like-workspace'
  pluginId: string
  title: string
  placeholder: string
  searchable: true
  variants: EditorVariant[]
  workspaces: EditorWorkspace[]
  errors: Array<{ variantId: string; message: string }>
  scannedAt: number
}

export interface EditorVariantSettings {
  id: string
  name: string
  version: string
  dataDir: string
  trigger: string
  enabled: boolean
}

export interface VscodeLikeWorkspaceOptions {
  includeRemote: boolean
  hideMissing: boolean
  maxItems: number
}

export interface EditorWorkspaceStats {
  total: number
  missing: number
  remote: number
  running: boolean
}

export interface VscodeLikeWorkspaceSettingsPage {
  kind: 'vscode-like-workspace-settings'
  pluginId: string
  title: string
  placeholder: string
  searchable: false
  variants: EditorVariantSettings[]
  options: VscodeLikeWorkspaceOptions
  triggerErrors: Record<string, string>
  workspaceStats: Record<string, EditorWorkspaceStats>
  error: string | null
  notice: string | null
  savedAt?: number
}

/** Workspace list for one editor, used by the cleanup view. */
export interface EditorWorkspaceSnapshot {
  variantId: string
  name: string
  running: boolean
  workspaces: EditorWorkspace[]
  error: string | null
}

export interface EditorCleanupResult {
  ok: boolean
  deleted: number
  running: boolean
  backupDir: string | null
  error?: string
  toast?: ToastMessage
}

export type PluginPage =
  | ClipboardPluginPage
  | ChromeBookmarksPluginPage
  | VscodeLikeWorkspacePluginPage
  | ExternalPluginPage

export type SettingsField =
  | {
      key: string
      type: 'select'
      label: string
      value: string | null
      options: Array<{ value: string; label: string }>
      description?: string
      disabled?: boolean
    }
  | {
      key: string
      type: 'number'
      label: string
      value: number
      min: number
      max: number
      step: number
      unit?: string
      description?: string
      disabled?: boolean
    }
  | {
      key: string
      type: 'switch'
      label: string
      value: boolean
      description?: string
      disabled?: boolean
    }
  | {
      key: string
      type: 'text'
      label: string
      value: string
      description?: string
      placeholder?: string
      mono?: boolean
      readonly?: boolean
      disabled?: boolean
    }

export interface GenericPluginSettingsPage {
  kind: 'settings'
  pluginId: string
  title: string
  placeholder: string
  searchable: boolean
  fields: SettingsField[]
  error: string | null
  notice: string | null
  footerHints?: Hint[]
  savedAt?: number
}

export type PluginSettingsPage = GenericPluginSettingsPage | VscodeLikeWorkspaceSettingsPage

export interface Hint {
  keys: string[]
  label: string
  tone?: 'default' | 'danger'
}

export interface ToastMessage {
  message: string
  tone: 'default' | 'success' | 'warning' | 'danger'
  duration?: number
}

export interface ActionResult {
  ok: boolean
  error?: string
  toast?: ToastMessage
  close?: boolean
  paste?: boolean
}

export interface BootstrapData {
  role: WindowRole
  platform: PlatformId
  version: string
  prefs: AppPrefs
  theme: ResolvedTheme
  shortcuts: ShortcutInfo[]
  shortcutErrors: ShortcutError[]
  plugins: PluginSummary[]
  commands: CommandEntry[]
  workspace: WorkspaceSummary
  chromeProfiles: ChromeProfile[]
  launchAtLoginSupported: boolean
  dataDir: string
  isDev: boolean
}

export interface PluginInspection {
  token: string
  manifest: PluginManifest
  sourceType: 'git' | 'local'
  source: string
  permissions: string[]
  entry: string
  fileCount: number
  size: number
  alreadyInstalled: boolean
  installedVersion: string | null
  warning: string | null
}

/**
 * Update lifecycle. `unsupported` means the newest release has no artifact for
 * this platform, so there is nothing the app can download.
 */
export type UpdatePhase =
  | 'unsupported'
  | 'idle'
  | 'checking'
  | 'latest'
  | 'available'
  | 'downloading'
  | 'ready'
  | 'installing'
  | 'error'

/** How the downloaded artifact gets applied. `manual` = hand off to the OS. */
export type UpdateInstallKind = 'relaunch' | 'manual' | 'none'

export interface UpdateState {
  phase: UpdatePhase
  currentVersion: string
  latestVersion: string | null
  releaseName: string | null
  releaseNotes: string | null
  releaseUrl: string | null
  publishedAt: number | null
  assetName: string | null
  assetSize: number | null
  downloadedBytes: number
  totalBytes: number
  /** 0…1, mirrored so the renderer never recomputes it from bytes. */
  progress: number
  downloadPath: string | null
  installKind: UpdateInstallKind
  canInstall: boolean
  message: string
  error: string | null
  checkedAt: number | null
}

export interface GitDiffResult {
  status: string
  diff: string
  error: string | null
}

export interface FlashcastEvents {
  'launcher:shown': undefined
  'launcher:close-request': undefined
  'launcher:open-plugin': { pluginId: string }
  'clipboard:changed': ClipboardItem[]
  'plugins:changed': PluginSummary[]
  'theme:changed': { theme: ResolvedTheme; setting: ThemeSetting }
  'workspace:changed': WorkspaceSummary
  'shortcuts:changed': { shortcuts: ShortcutInfo[]; errors: ShortcutError[] }
  'toast': ToastMessage
  'prefs:changed': AppPrefs
  'updates:changed': UpdateState
}

export type EventName = keyof FlashcastEvents

export interface FlashcastApi {
  readonly platform: PlatformId
  getBootstrap(): Promise<BootstrapData>
  getApps(): Promise<AppEntry[]>
  refreshApps(): Promise<AppEntry[]>

  getPluginPage(pluginId: string): Promise<PluginPage>
  getPluginSettings(
    pluginId: string,
    options?: { refresh?: boolean }
  ): Promise<PluginSettingsPage>
  savePluginSettings(
    pluginId: string,
    patch: Record<string, unknown>
  ): Promise<{ page: PluginSettingsPage; savedAt: number }>
  searchExternal(pluginId: string, query: string): Promise<ExternalPluginItem[]>
  activateExternal(
    pluginId: string,
    itemId: string,
    modifiers: { alt: boolean }
  ): Promise<ActionResult>

  activateClipboard(
    itemId: string,
    options: { paste: boolean }
  ): Promise<ActionResult>
  deleteClipboard(itemId: string): Promise<ActionResult>
  clearClipboard(): Promise<ActionResult>

  openChromeBookmark(bookmark: { id: string; url: string; title: string }): Promise<ActionResult>
  openEditorWorkspace(
    workspaceId: string,
    options: { newWindow: boolean }
  ): Promise<ActionResult>
  revealEditorWorkspace(workspaceId: string): Promise<ActionResult>
  getEditorWorkspaces(): Promise<EditorWorkspaceSnapshot[]>
  deleteEditorWorkspaces(variantId: string, ids: string[]): Promise<EditorCleanupResult>
  getFavicons(domains: string[]): Promise<Record<string, string>>
  openSettings(): Promise<void>
  hideLauncher(options?: { paste?: boolean }): void

  launchApp(appId: string): Promise<ActionResult>
  runCommand(commandId: CommandEntry['id']): Promise<ActionResult>

  updatePrefs(patch: Partial<Pick<AppPrefs, 'theme' | 'launchAtLogin' | 'autoCommit'>>): Promise<BootstrapData>
  chooseWorkspace(): Promise<{ canceled: boolean; workspace: WorkspaceSummary }>
  openWorkspaceFolder(): Promise<void>
  refreshWorkspace(): Promise<WorkspaceSummary>
  getGitDiff(): Promise<GitDiffResult>

  inspectPlugin(input: string): Promise<PluginInspection>
  installPlugin(token: string): Promise<{ plugin: PluginSummary; plugins: PluginSummary[] }>
  updatePlugin(pluginId: string): Promise<ActionResult>
  uninstallPlugin(pluginId: string): Promise<ActionResult>
  setPluginEnabled(pluginId: string, enabled: boolean): Promise<ActionResult>
  openPluginDirectory(pluginId: string): Promise<void>

  setShortcuts(patch: Partial<GlobalHotkeys>): Promise<{ shortcuts: ShortcutInfo[]; errors: ShortcutError[] }>
  resetShortcuts(): Promise<{ shortcuts: ShortcutInfo[]; errors: ShortcutError[] }>
  getUpdateState(): Promise<UpdateState>
  checkForUpdates(): Promise<UpdateState>
  downloadUpdate(): Promise<UpdateState>
  cancelUpdate(): Promise<UpdateState>
  installUpdate(): Promise<ActionResult>
  openReleasePage(): Promise<void>
  showUpdateFile(): Promise<void>
  quit(): void

  on<K extends EventName>(event: K, handler: (payload: FlashcastEvents[K]) => void): () => void
}

/* ------------------------------------------------------------------ */
/* Constants shared by more than one process                           */
/* ------------------------------------------------------------------ */

export const CLIPBOARD_MEDIA_SCHEME = 'flashcast-media'
export const DESKTOP_WALLPAPER_SCHEME = 'flashcast-wallpaper'
export const CLIPBOARD_MEDIA_PREFIX = 'media/'
export const BUILTIN_PLUGIN_IDS = [
  'clipboard',
  'chrome-bookmarks',
  'vscode-like-workspace'
] as const
export type BuiltinPluginId = (typeof BUILTIN_PLUGIN_IDS)[number]

export const SERVICE_NAME = 'Flashcast'
