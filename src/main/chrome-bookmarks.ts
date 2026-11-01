import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'
import { readdir, readFile, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { delimiter, dirname, join } from 'node:path'
import type {
  ChromeBookmark,
  ChromeBookmarkSource,
  ChromeProfile,
  PlatformId
} from '@shared/types'
import { domainOf } from '@shared/fuzzy'
import { log } from './logger'

interface ChromeRoot {
  children?: ChromeNode[]
  name?: string
  type?: string
}

interface ChromeNode {
  id?: string
  name?: string
  type?: string
  url?: string
  children?: ChromeNode[]
}

interface BookmarksFile {
  roots?: {
    bookmark_bar?: ChromeRoot
    other?: ChromeRoot
    synced?: ChromeRoot
  }
}

const ROOT_LABELS: Record<string, string> = {
  bookmark_bar: '书签栏',
  other: '其他书签',
  synced: '移动设备书签'
}

export function chromeUserDataDirs(platform: PlatformId): string[] {
  const override = process.env.FLASHCAST_CHROME_USER_DATA?.trim()
  if (override) return [override]

  if (platform === 'darwin') {
    return [
      join(homedir(), 'Library/Application Support/Google/Chrome'),
      join(homedir(), 'Library/Application Support/Google/Chrome Beta'),
      join(homedir(), 'Library/Application Support/Chromium')
    ]
  }
  if (platform === 'win32') {
    const local = process.env.LOCALAPPDATA ?? join(homedir(), 'AppData/Local')
    return [
      join(local, 'Google/Chrome/User Data'),
      join(local, 'Google/Chrome Beta/User Data'),
      join(local, 'Chromium/User Data')
    ]
  }
  return [
    join(homedir(), '.config/google-chrome'),
    join(homedir(), '.config/google-chrome-beta'),
    join(homedir(), '.config/chromium')
  ]
}

interface LocalState {
  profile?: {
    info_cache?: Record<string, { name?: string; gaia_name?: string; shortcut_name?: string }>
    profiles_order?: string[]
  }
}

const PROFILE_DIR_PATTERN = /^Profile \d+$/

/**
 * Chrome keeps bookmarks in one of two profile files. `Bookmarks` holds the
 * local library; once the profile saves bookmarks in the signed-in Google
 * Account the library lives in `AccountBookmarks` and `Bookmarks` is not
 * written at all.
 */
export const CHROME_BOOKMARK_FILES: ReadonlyArray<{
  name: string
  source: ChromeBookmarkSource
}> = [
  { name: 'Bookmarks', source: 'local' },
  { name: 'AccountBookmarks', source: 'account' }
]

function looksLikeProfileDir(name: string, dir: string): boolean {
  return (
    name === 'Default' ||
    PROFILE_DIR_PATTERN.test(name) ||
    existsSync(join(dir, 'Preferences')) ||
    CHROME_BOOKMARK_FILES.some((file) => existsSync(join(dir, file.name)))
  )
}

export async function discoverChromeProfiles(platform: PlatformId): Promise<{
  profiles: ChromeProfile[]
  userDataDir: string
}> {
  for (const dir of chromeUserDataDirs(platform)) {
    if (!existsSync(dir)) continue
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      continue
    }

    let profileState: LocalState['profile'] = {}
    try {
      const state = JSON.parse(await readFile(join(dir, 'Local State'), 'utf8')) as LocalState
      profileState = state.profile ?? {}
    } catch {
      profileState = {}
    }

    const infoCache = profileState.info_cache ?? {}
    const dirs = new Map<string, string>()
    for (const entry of entries) {
      if (entry.isDirectory()) dirs.set(entry.name, join(dir, entry.name))
    }

    // `Local State` is authoritative for profile names and catches profiles
    // that have never written a Bookmarks file. Fall back to the usual
    // Default / Profile N directories when Local State is missing or stale.
    const candidates = [
      ...(profileState.profiles_order ?? []),
      'Default',
      ...Object.keys(infoCache),
      ...dirs.keys()
    ]
    const profiles: ChromeProfile[] = []
    const seen = new Set<string>()
    for (const name of candidates) {
      if (!name || seen.has(name)) continue
      seen.add(name)
      const profileDir = dirs.get(name)
      if (!profileDir) continue
      const known = Object.prototype.hasOwnProperty.call(infoCache, name)
      if (!known && !looksLikeProfileDir(name, profileDir)) continue
      const info = infoCache[name]
      const displayName = info?.name || info?.shortcut_name || name
      profiles.push({
        dir: name,
        name: displayName,
        bookmarksPath: join(profileDir, 'Bookmarks')
      })
    }

    if (profiles.length > 0) {
      profiles.sort((a, b) => {
        if (a.dir === 'Default') return -1
        if (b.dir === 'Default') return 1
        return a.dir.localeCompare(b.dir)
      })
      return { profiles, userDataDir: dir }
    }
  }
  return { profiles: [], userDataDir: chromeUserDataDirs(platform)[0] }
}

export function flattenBookmarks(data: BookmarksFile): ChromeBookmark[] {
  const bookmarks: ChromeBookmark[] = []
  const roots = data.roots ?? {}
  const rootOrder = ['bookmark_bar', 'other', 'synced'] as const

  const walk = (node: ChromeNode, folder: string[]): void => {
    if (node.type === 'url' && node.url) {
      const protocol = safeProtocol(node.url)
      if (protocol !== 'http:' && protocol !== 'https:' && protocol !== 'file:') return
      const title = (node.name ?? '').trim() || node.url
      const id =
        node.id ??
        createHash('sha1').update(`${folder.join('/')}|${title}|${node.url}`).digest('hex').slice(0, 20)
      bookmarks.push({
        id,
        title,
        url: node.url,
        folder,
        domain: domainOf(node.url)
      })
      return
    }

    if (node.type === 'folder') {
      const nextFolder = node.name ? [...folder, node.name] : folder
      for (const child of node.children ?? []) walk(child, nextFolder)
    }
  }

  for (const key of rootOrder) {
    const root = roots[key]
    if (!root) continue
    const rootLabel = root.name || ROOT_LABELS[key]
    for (const child of root.children ?? []) {
      if (child.type === 'url') {
        walk(child, [rootLabel])
      } else {
        walk(child, [])
      }
    }
  }
  return bookmarks
}

function safeProtocol(url: string): string | null {
  try {
    return new URL(url).protocol
  } catch {
    return null
  }
}

export interface ChromeBookmarksResult {
  profiles: ChromeProfile[]
  selectedProfile: string | null
  userDataDir: string
  bookmarkFile: string | null
  bookmarkSource: ChromeBookmarkSource | null
  bookmarks: ChromeBookmark[]
  error: string | null
}

interface BookmarkCandidate {
  path: string
  source: ChromeBookmarkSource
  mtimeMs: number
}

async function bookmarkCandidates(profileDir: string): Promise<BookmarkCandidate[]> {
  const candidates: BookmarkCandidate[] = []
  for (const file of CHROME_BOOKMARK_FILES) {
    const path = join(profileDir, file.name)
    try {
      const info = await stat(path)
      if (!info.isFile() || info.size === 0) continue
      candidates.push({ path, source: file.source, mtimeMs: info.mtimeMs })
    } catch {
      // Missing file: the profile simply uses the other storage.
    }
  }
  // Chrome only writes the file backing the active storage, so the most
  // recently written one wins. Ties go to the account file because a profile
  // migrated to account storage stops updating the local one.
  candidates.sort(
    (a, b) => b.mtimeMs - a.mtimeMs || (a.source === 'account' ? -1 : b.source === 'account' ? 1 : 0)
  )
  return candidates
}

/** Resolves which bookmark file the profile is actually using, if any. */
export async function findBookmarksFile(
  profileDir: string
): Promise<{ path: string; source: ChromeBookmarkSource } | null> {
  const candidates = await bookmarkCandidates(profileDir)
  const first = candidates[0]
  return first ? { path: first.path, source: first.source } : null
}

export async function readChromeBookmarks(
  platform: PlatformId,
  selectedProfile: string | null
): Promise<ChromeBookmarksResult> {
  const { profiles, userDataDir } = await discoverChromeProfiles(platform)
  if (profiles.length === 0) {
    return {
      profiles,
      selectedProfile: null,
      userDataDir,
      bookmarkFile: null,
      bookmarkSource: null,
      bookmarks: [],
      error: '未找到 Chrome Profile'
    }
  }

  const profile =
    profiles.find((item) => item.dir === selectedProfile) ??
    profiles.find((item) => item.dir === 'Default') ??
    profiles[0]

  const candidates = await bookmarkCandidates(dirname(profile.bookmarksPath))
  if (candidates.length === 0) {
    // Chrome only materialises a bookmark file after the first bookmark
    // exists. A freshly used profile is still selectable and simply has none.
    return {
      profiles,
      selectedProfile: profile.dir,
      userDataDir,
      bookmarkFile: profile.bookmarksPath,
      bookmarkSource: null,
      bookmarks: [],
      error: null
    }
  }

  let oversized = false
  let parseFailed = false
  for (const candidate of candidates) {
    try {
      const info = await stat(candidate.path)
      if (info.size > 40 * 1024 * 1024) {
        oversized = true
        continue
      }
      const data = JSON.parse(await readFile(candidate.path, 'utf8')) as BookmarksFile
      return {
        profiles,
        selectedProfile: profile.dir,
        userDataDir,
        bookmarkFile: candidate.path,
        bookmarkSource: candidate.source,
        bookmarks: flattenBookmarks(data),
        error: null
      }
    } catch (error) {
      parseFailed = true
      log.warn(`reading Chrome bookmarks failed (${candidate.path}):`, error)
    }
  }

  return {
    profiles,
    selectedProfile: profile.dir,
    userDataDir,
    bookmarkFile: candidates[0].path,
    bookmarkSource: candidates[0].source,
    bookmarks: [],
    error: oversized && !parseFailed ? '书签文件过大，无法读取' : '书签读取失败'
  }
}

function commandExists(command: string): boolean {
  if (command.includes('/') || command.includes('\\')) return existsSync(command)
  return (process.env.PATH ?? '')
    .split(delimiter)
    .filter(Boolean)
    .some((dir) => existsSync(join(dir, command)))
}

export function findChromeExecutable(platform: PlatformId): string | null {
  if (platform === 'darwin') {
    const candidates = [
      '/Applications/Google Chrome.app',
      join(homedir(), 'Applications/Google Chrome.app'),
      '/Applications/Chromium.app'
    ]
    return candidates.find((path) => existsSync(path)) ?? null
  }

  if (platform === 'win32') {
    const roots = [
      process.env.PROGRAMFILES,
      process.env['PROGRAMFILES(X86)'],
      process.env.LOCALAPPDATA
    ].filter(Boolean) as string[]
    const candidates = roots.flatMap((root) => [
      join(root, 'Google/Chrome/Application/chrome.exe'),
      join(root, 'Chromium/Application/chrome.exe')
    ])
    return candidates.find((path) => existsSync(path)) ?? null
  }

  const candidates = [
    'google-chrome',
    'google-chrome-stable',
    'google-chrome-beta',
    'chromium',
    'chromium-browser',
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/opt/google/chrome/chrome'
  ]
  return candidates.find((command) => commandExists(command)) ?? null
}

export async function openInChrome(url: string, platform: PlatformId): Promise<void> {
  const protocol = safeProtocol(url)
  if (!protocol || !['http:', 'https:', 'file:'].includes(protocol)) {
    throw new Error('不支持的书签链接')
  }
  const chrome = findChromeExecutable(platform)
  if (!chrome) throw new Error('未找到 Chrome 安装')

  await new Promise<void>((resolve, reject) => {
    let child
    if (platform === 'darwin') {
      child = spawn('open', ['-a', 'Google Chrome', url], { detached: true, stdio: 'ignore' })
    } else {
      child = spawn(chrome, ['--new-tab', url], { detached: true, stdio: 'ignore' })
    }
    child.on('error', reject)
    child.on('spawn', () => {
      child.unref()
      resolve()
    })
  })
}
