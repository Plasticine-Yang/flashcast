import { existsSync } from 'node:fs'
import { copyFile, mkdtemp, readFile, readdir, rm, stat } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { homedir, tmpdir } from 'node:os'
import { join } from 'node:path'
import type { EditorWorkspace, PlatformId } from '@shared/types'
import type { VariantRecord } from './detect'
import {
  parseWorkspaceUri,
  workspaceDisplayPath,
  workspaceNameFor
} from './uri'

export interface IndexOptions {
  platform: PlatformId
  home: string
  includeRemote: boolean
  hideMissing: boolean
  maxItems: number
}

export interface IndexResult {
  workspaces: EditorWorkspace[]
  error: string | null
}

interface RecentEntry {
  folderUri?: string
  fileUri?: string
  workspace?: string | { configPath?: string; id?: string }
  remoteAuthority?: string
}

interface OpenWindow {
  folder?: string
  workspace?: string | { configPath?: string; id?: string }
  remoteAuthority?: string
}

interface StorageShape {
  windowsState?: {
    openedWindows?: OpenWindow[]
    lastActiveWindow?: OpenWindow
  }
  openedPathsList?: { entries?: RecentEntry[] }
  profileAssociations?: { workspaces?: Record<string, string> }
}

interface Candidate {
  key: string
  uri: string
  kind: 'folder' | 'workspace'
  open: boolean
  recentRank: number
  lastActiveAt: number
}

async function readJson<T>(path: string): Promise<T | null> {
  try {
    return JSON.parse(await readFile(path, 'utf8')) as T
  } catch {
    return null
  }
}

function uriOf(value: unknown): string | null {
  if (typeof value === 'string' && value) return value
  if (value && typeof value === 'object') {
    const record = value as { configPath?: unknown }
    if (typeof record.configPath === 'string' && record.configPath) return record.configPath
  }
  return null
}

function windowUri(window: OpenWindow): string | null {
  return uriOf(window.folder) ?? uriOf(window.workspace)
}

async function readRecentEntries(dbPath: string): Promise<{
  entries: RecentEntry[]
  error: string | null
}> {
  const tryRead = async (path: string, readOnly: boolean): Promise<RecentEntry[]> => {
    const sqlite = await import('node:sqlite')
    const database = new sqlite.DatabaseSync(path, { readOnly })
    try {
      const statement = database.prepare(
        'SELECT value FROM ItemTable WHERE key = ?'
      )
      const row = statement.get('history.recentlyOpenedPathsList') as
        | { value?: unknown }
        | undefined
      if (!row || typeof row.value !== 'string') return []
      const parsed = JSON.parse(row.value) as { entries?: RecentEntry[] }
      return Array.isArray(parsed.entries) ? parsed.entries : []
    } finally {
      database.close()
    }
  }

  if (!existsSync(dbPath)) return { entries: [], error: null }

  try {
    return { entries: await tryRead(dbPath, true), error: null }
  } catch (error) {
    // VS Code keeps a backup for exactly this case; use it before giving up.
    const backup = `${dbPath}.backup`
    if (existsSync(backup)) {
      try {
        return { entries: await tryRead(backup, true), error: null }
      } catch {
        // fall through to the temp copy
      }
    }
  }

  let directory: string | null = null
  try {
    directory = await mkdtemp(join(tmpdir(), 'flashcast-vscdb-'))
    const copy = join(directory, 'state.vscdb')
    await copyFile(dbPath, copy)
    for (const suffix of ['-wal', '-shm']) {
      if (existsSync(`${dbPath}${suffix}`)) {
        await copyFile(`${dbPath}${suffix}`, `${copy}${suffix}`)
      }
    }
    return { entries: await tryRead(copy, false), error: null }
  } catch (error) {
    return {
      entries: [],
      error: `最近记录读取失败：${(error as Error).message}`
    }
  } finally {
    if (directory) await rm(directory, { recursive: true, force: true })
  }
}

async function readWorkspaceStorage(
  directory: string
): Promise<Map<string, { uri: string; mtime: number }>> {
  const result = new Map<string, { uri: string; mtime: number }>()
  let entries
  try {
    entries = await readdir(directory, { withFileTypes: true })
  } catch {
    return result
  }

  for (const entry of entries) {
    if (!entry.isDirectory()) continue
    const folder = join(directory, entry.name)
    const data = await readJson<{ folder?: string; workspace?: string | { configPath?: string } }>(
      join(folder, 'workspace.json')
    )
    if (!data) continue
    const uri = uriOf(data.folder) ?? uriOf(data.workspace)
    if (!uri) continue
    try {
      const info = await stat(folder)
      result.set(uri, { uri, mtime: info.mtimeMs })
    } catch {
      // The directory disappeared between readdir and stat.
    }
  }

  return result
}

function stableId(variantId: string, key: string): string {
  return createHash('sha1').update(`${variantId}\u0000${key}`).digest('hex').slice(0, 20)
}

/**
 * Indexes one editor variant from the three files VS Code-family products use
 * for workspace history:
 *
 * - `state.vscdb` `history.recentlyOpenedPathsList` (ordered recents)
 * - `storage.json` `windowsState` (currently open windows)
 * - `workspaceStorage/<hash>/workspace.json` (every workspace ever opened)
 *
 * Missing files are normal (an editor that has never run). A locked database
 * degrades to the other sources and surfaces a per-variant error.
 */
export async function indexVariant(
  variant: VariantRecord,
  options: IndexOptions
): Promise<IndexResult> {
  if (variant.error) return { workspaces: [], error: variant.error }

  const userDir = join(variant.dataDir, 'User')
  if (!existsSync(userDir)) return { workspaces: [], error: null }

  const storagePath = join(userDir, 'globalStorage', 'storage.json')
  const dbPath = join(userDir, 'globalStorage', 'state.vscdb')
  const storage = await readJson<StorageShape>(storagePath)
  const recent = await readRecentEntries(dbPath)

  const now = Date.now()
  const openUris: string[] = []
  const recentUris: string[] = []
  const associations: string[] = []

  if (storage) {
    for (const window of storage.windowsState?.openedWindows ?? []) {
      const uri = windowUri(window)
      if (uri) openUris.push(uri)
    }
    const active = storage.windowsState?.lastActiveWindow
    const activeUri = active ? windowUri(active) : null
    if (activeUri) openUris.push(activeUri)

    for (const entry of storage.openedPathsList?.entries ?? []) {
      const uri = uriOf(entry.folderUri) ?? uriOf(entry.workspace)
      if (uri) recentUris.push(uri)
    }

    for (const uri of Object.keys(storage.profileAssociations?.workspaces ?? {})) {
      associations.push(uri)
    }
  }

  for (const entry of recent.entries) {
    const uri = uriOf(entry.folderUri) ?? uriOf(entry.workspace)
    if (uri) recentUris.push(uri)
  }

  const openKeys = new Set<string>()
  for (const uri of openUris) {
    const parsed = parseWorkspaceUri(uri, options.platform)
    if (parsed) openKeys.add(parsed.key)
  }

  const candidates = new Map<string, Candidate>()

  const add = (
    uri: string,
    source: { open?: boolean; recentRank?: number; lastActiveAt?: number }
  ): void => {
    const parsed = parseWorkspaceUri(uri, options.platform)
    if (!parsed) return
    if (!options.includeRemote && parsed.remote) return

    const baseKey = parsed.remote ? `remote:${parsed.key}` : `local:${parsed.key}`
    const existing = candidates.get(baseKey)
    const open = source.open === true || openKeys.has(parsed.key)
    const lastActiveAt = source.lastActiveAt ?? 0
    const recentRank = source.recentRank ?? Number.POSITIVE_INFINITY

    if (existing) {
      existing.open = existing.open || open
      existing.recentRank = Math.min(existing.recentRank, recentRank)
      existing.lastActiveAt = Math.max(existing.lastActiveAt, lastActiveAt)
      return
    }

    candidates.set(baseKey, {
      key: baseKey,
      uri,
      kind: parsed.kind,
      open,
      recentRank,
      lastActiveAt
    })
  }

  for (const uri of openUris) add(uri, { open: true, lastActiveAt: now })
  recentUris.forEach((uri, index) => {
    add(uri, {
      recentRank: index,
      lastActiveAt: Math.max(0, now - index * 60_000)
    })
  })

  const workspaceStorage = await readWorkspaceStorage(join(userDir, 'workspaceStorage'))
  for (const entry of workspaceStorage.values()) {
    add(entry.uri, { lastActiveAt: entry.mtime })
  }

  for (const uri of associations) add(uri, {})

  const workspaces: Array<{ workspace: EditorWorkspace; candidate: Candidate }> = []
  for (const candidate of candidates.values()) {
    const parsed = parseWorkspaceUri(candidate.uri, options.platform)
    if (!parsed) continue
    const exists = parsed.remote ? true : existsSync(parsed.path)
    if (options.hideMissing && !exists) continue

    workspaces.push({
      candidate,
      workspace: {
        id: stableId(variant.id, candidate.key),
        variantId: variant.id,
        name: workspaceNameFor(parsed.path, parsed.remote),
        path: parsed.path,
        displayPath: workspaceDisplayPath(
          parsed.path,
          parsed.remote,
          options.home,
          options.platform
        ),
        uri: candidate.uri,
        kind: candidate.kind,
        remote: parsed.remote,
        isOpen: candidate.open,
        lastActiveAt: candidate.lastActiveAt,
        exists
      }
    })
  }

  workspaces.sort((left, right) => {
    if (left.workspace.isOpen !== right.workspace.isOpen) {
      return left.workspace.isOpen ? -1 : 1
    }
    if (left.candidate.recentRank !== right.candidate.recentRank) {
      return left.candidate.recentRank - right.candidate.recentRank
    }
    if (left.workspace.lastActiveAt !== right.workspace.lastActiveAt) {
      return right.workspace.lastActiveAt - left.workspace.lastActiveAt
    }
    return left.workspace.name.localeCompare(right.workspace.name, 'zh-CN')
  })

  return {
    workspaces: workspaces
      .slice(0, Math.max(1, options.maxItems))
      .map((entry) => entry.workspace),
    error: recent.error
  }
}
