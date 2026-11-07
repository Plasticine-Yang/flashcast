import { execFile } from 'node:child_process'
import { cp, mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { basename, join } from 'node:path'
import { promisify } from 'node:util'
import type { EditorWorkspace, PlatformId } from '@shared/types'
import type { VariantRecord } from './detect'
import { parseWorkspaceUri } from './uri'

const execFileAsync = promisify(execFile)
const RECENTS_KEY = 'history.recentlyOpenedPathsList'

export interface PruneResult {
  removed: number
  failures: Array<{ id: string; message: string }>
  backupDir: string | null
  running: boolean
}

interface RecentEntry {
  folderUri?: string
  fileUri?: string
  workspace?: string | { configPath?: string; id?: string }
}

interface WindowEntry {
  folder?: string
  workspace?: string | { configPath?: string; id?: string }
}

interface StorageShape {
  windowsState?: {
    openedWindows?: WindowEntry[]
    lastActiveWindow?: WindowEntry | null
  }
  openedPathsList?: { entries?: RecentEntry[] }
  profileAssociations?: { workspaces?: Record<string, string> }
  backupWorkspaces?: {
    workspaces?: Array<{ folderUri?: string; workspace?: { configPath?: string } }>
    folders?: Array<{ folderUri?: string }>
  }
}

/* ------------------------------ processes ------------------------------ */

/**
 * Lists running commands once and matches editor variants against them.
 * History files are written by the editor while it runs, so pruning while it
 * is open would be silently overwritten (or clash with its WAL).
 */
export async function runningVariants(
  variants: Array<Pick<VariantRecord, 'id' | 'appPath' | 'executable'>>
): Promise<Set<string>> {
  const running = new Set<string>()
  if (variants.length === 0) return running

  let commands: string[] = []
  const windows = variants.some((variant) => variant.executable?.toLowerCase().endsWith('.exe'))
  try {
    if (windows) {
      // "Code.exe","1234","Console","1","123,456 K"
      const { stdout } = await execFileAsync('tasklist', ['/fo', 'csv', '/nh'], {
        maxBuffer: 8 * 1024 * 1024
      })
      commands = stdout
        .split('\n')
        .map((line) => line.split('","')[0]?.replace(/^"/, '').trim().toLowerCase() ?? '')
    } else {
      const { stdout } = await execFileAsync('ps', ['-A', '-o', 'command='], {
        maxBuffer: 8 * 1024 * 1024
      })
      commands = stdout.split('\n')
    }
  } catch {
    return running
  }

  for (const variant of variants) {
    const needles = [variant.appPath, variant.executable]
      .filter((value): value is string => Boolean(value))
      .map((value) => (windows ? basename(value).toLowerCase() : value))
    if (needles.length === 0) continue
    const hit = commands.some((command) =>
      needles.some((needle) => command.includes(needle))
    )
    if (hit) running.add(variant.id)
  }
  return running
}

/* ------------------------------- matching ------------------------------- */

function uriKey(uri: string, platform: PlatformId): string | null {
  const parsed = parseWorkspaceUri(uri, platform)
  if (!parsed) return null
  return parsed.remote ? `remote:${parsed.key}` : `local:${parsed.key}`
}

function entryUri(entry: { folderUri?: string; workspace?: unknown; folder?: string }): string | null {
  if (typeof entry.folderUri === 'string' && entry.folderUri) return entry.folderUri
  if (typeof entry.folder === 'string' && entry.folder) return entry.folder
  const workspace = entry.workspace
  if (typeof workspace === 'string' && workspace) return workspace
  if (workspace && typeof workspace === 'object') {
    const configPath = (workspace as { configPath?: unknown }).configPath
    if (typeof configPath === 'string' && configPath) return configPath
  }
  return null
}

/* -------------------------------- backup -------------------------------- */

async function backupFile(path: string, target: string): Promise<void> {
  if (!existsSync(path)) return
  await mkdir(join(target, '..'), { recursive: true })
  await cp(path, target, { force: true })
}

/* -------------------------------- pruning -------------------------------- */

async function pruneRecents(
  dbPath: string,
  keys: Set<string>,
  platform: PlatformId
): Promise<number> {
  if (!existsSync(dbPath)) return 0

  const sqlite = await import('node:sqlite')
  const database = new sqlite.DatabaseSync(dbPath)
  try {
    const row = database
      .prepare('SELECT value FROM ItemTable WHERE key = ?')
      .get(RECENTS_KEY) as { value?: unknown } | undefined
    if (!row || typeof row.value !== 'string') return 0

    const parsed = JSON.parse(row.value) as { entries?: RecentEntry[] }
    const entries = Array.isArray(parsed.entries) ? parsed.entries : []
    const kept = entries.filter((entry) => {
      const uri = entryUri(entry)
      const key = uri ? uriKey(uri, platform) : null
      return !key || !keys.has(key)
    })
    if (kept.length === entries.length) return 0

    database
      .prepare('UPDATE ItemTable SET value = ? WHERE key = ?')
      .run(JSON.stringify({ ...parsed, entries: kept }), RECENTS_KEY)
    return entries.length - kept.length
  } finally {
    database.close()
  }
}

function pruneStorage(
  storage: StorageShape,
  keys: Set<string>,
  platform: PlatformId
): { changed: boolean; removed: number } {
  let removed = 0
  const keep = (uri: string | null): boolean => {
    if (!uri) return true
    const key = uriKey(uri, platform)
    return !key || !keys.has(key)
  }

  const associations = storage.profileAssociations?.workspaces
  if (associations) {
    for (const uri of Object.keys(associations)) {
      if (!keep(uri)) {
        delete associations[uri]
        removed += 1
      }
    }
  }

  const recentEntries = storage.openedPathsList?.entries
  if (Array.isArray(recentEntries)) {
    const kept = recentEntries.filter((entry) => keep(entryUri(entry)))
    removed += recentEntries.length - kept.length
    storage.openedPathsList = { ...storage.openedPathsList, entries: kept }
  }

  const backupFolders = storage.backupWorkspaces?.folders
  if (Array.isArray(backupFolders)) {
    const kept = backupFolders.filter((entry) => keep(entry.folderUri ?? null))
    removed += backupFolders.length - kept.length
    storage.backupWorkspaces = { ...storage.backupWorkspaces, folders: kept }
  }

  const backupWorkspaces = storage.backupWorkspaces?.workspaces
  if (Array.isArray(backupWorkspaces)) {
    const kept = backupWorkspaces.filter((entry) =>
      keep(entry.folderUri ?? entry.workspace?.configPath ?? null)
    )
    removed += backupWorkspaces.length - kept.length
    storage.backupWorkspaces = { ...storage.backupWorkspaces, workspaces: kept }
  }

  if (Array.isArray(storage.windowsState?.openedWindows)) {
    const kept = storage.windowsState.openedWindows.filter((entry) => keep(entryUri(entry)))
    if (kept.length !== storage.windowsState.openedWindows.length) {
      removed += storage.windowsState.openedWindows.length - kept.length
      storage.windowsState.openedWindows = kept
    }
  }

  const lastActive = storage.windowsState?.lastActiveWindow
  if (lastActive && !keep(entryUri(lastActive))) {
    storage.windowsState = { ...storage.windowsState, lastActiveWindow: null }
    removed += 1
  }

  return { changed: removed > 0, removed }
}

/**
 * Removes workspaces from one editor's history.
 *
 * Only history is touched: the project folders on disk are never deleted.
 * `workspaceStorage/<hash>` directories are *moved* into the backup folder so
 * the editor forgets the workspace's UI state and the action stays reversible.
 */
export async function pruneWorkspaces(
  variant: VariantRecord,
  targets: EditorWorkspace[],
  backupRoot: string,
  platform: PlatformId
): Promise<PruneResult> {
  const result: PruneResult = { removed: 0, failures: [], backupDir: null, running: false }
  if (targets.length === 0) return result

  const running = await runningVariants([variant])
  if (running.has(variant.id)) {
    return { ...result, running: true }
  }

  const userDir = join(variant.dataDir, 'User')
  const globalStorage = join(userDir, 'globalStorage')
  const backupDir = join(backupRoot, `${variant.id}-${Date.now()}`)

  const keys = new Set<string>()
  for (const target of targets) {
    const key = uriKey(target.uri, platform)
    if (key) keys.add(key)
  }
  if (keys.size === 0) return result

  // 1. Recents in the SQLite state database (back up before writing).
  const dbPath = join(globalStorage, 'state.vscdb')
  try {
    if (existsSync(dbPath)) {
      await backupFile(dbPath, join(backupDir, 'state.vscdb'))
      for (const suffix of ['-wal', '-shm']) {
        await backupFile(`${dbPath}${suffix}`, join(backupDir, `state.vscdb${suffix}`))
      }
      result.backupDir ??= backupDir
      result.removed += await pruneRecents(dbPath, keys, platform)
    }
  } catch (error) {
    result.failures.push({ id: variant.id, message: `最近列表写入失败：${(error as Error).message}` })
  }

  // 2. Open windows, profile associations and backups in storage.json.
  const storagePath = join(globalStorage, 'storage.json')
  try {
    if (existsSync(storagePath)) {
      const storage = JSON.parse(await readFile(storagePath, 'utf8')) as StorageShape
      const { changed, removed } = pruneStorage(storage, keys, platform)
      if (changed) {
        await backupFile(storagePath, join(backupDir, 'storage.json'))
        result.backupDir ??= backupDir
        await writeFile(storagePath, `${JSON.stringify(storage, null, 2)}\n`, 'utf8')
        result.removed += removed
      }
    }
  } catch (error) {
    result.failures.push({ id: variant.id, message: `storage.json 写入失败：${(error as Error).message}` })
  }

  // 3. Per-workspace state directories: move instead of delete.
  const workspaceStorage = join(userDir, 'workspaceStorage')
  let entries: Array<{ name: string }> = []
  try {
    entries = (await readdir(workspaceStorage, { withFileTypes: true }))
      .filter((entry) => entry.isDirectory())
      .map((entry) => ({ name: entry.name }))
  } catch {
    entries = []
  }

  for (const entry of entries) {
    const directory = join(workspaceStorage, entry.name)
    try {
      const data = JSON.parse(await readFile(join(directory, 'workspace.json'), 'utf8')) as {
        folder?: string
        workspace?: string | { configPath?: string }
      }
      const uri =
        data.folder ??
        (typeof data.workspace === 'string' ? data.workspace : data.workspace?.configPath) ??
        null
      const key = uri ? uriKey(uri, platform) : null
      if (!key || !keys.has(key)) continue

      const destination = join(backupDir, 'workspaceStorage', basename(directory))
      await mkdir(join(destination, '..'), { recursive: true })
      try {
        await rename(directory, destination)
      } catch {
        await cp(directory, destination, { recursive: true, force: true })
        await rm(directory, { recursive: true, force: true })
      }
      result.backupDir ??= backupDir
      result.removed += 1
    } catch {
      // A workspace without a readable workspace.json carries no identity.
    }
  }

  return result
}
