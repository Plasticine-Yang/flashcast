import { app, shell } from 'electron'
import { spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'
import { readdir, readFile, stat } from 'node:fs/promises'
import { homedir } from 'node:os'
import { basename, delimiter, extname, isAbsolute, join } from 'node:path'
import type { AppEntry, PlatformId } from '@shared/types'
import { log } from './logger'

interface LaunchDescriptor {
  type: 'desktop' | 'bundle' | 'shortcut'
  path: string
  exec?: string
}

interface AppRecord extends AppEntry {
  launch: LaunchDescriptor
}

const CATEGORY_LABELS: Record<string, string> = {
  Network: '网络',
  Office: '办公',
  Graphics: '图形',
  AudioVideo: '影音',
  Development: '开发',
  Game: '游戏',
  Education: '教育',
  System: '系统',
  Settings: '设置',
  Utility: '实用工具',
  WebBrowser: '浏览器',
  TextEditor: '文本编辑',
  TerminalEmulator: '终端',
  FileManager: '文件',
  Audio: '音频',
  Video: '视频',
  Player: '播放器',
  InstantMessaging: '通讯',
  Email: '邮件',
  Calendar: '日历',
  Photography: '摄影',
  Science: '科学',
  Math: '数学',
  Accessibility: '无障碍',
  Building: '建筑',
  Debugger: '调试',
  IDE: '开发',
  Java: '开发',
  Python: '开发'
}

function commandExists(command: string): boolean {
  if (isAbsolute(command)) return existsSync(command)
  return (process.env.PATH ?? '')
    .split(delimiter)
    .filter(Boolean)
    .some((dir) => existsSync(join(dir, command)))
}

function categoryLabel(categories: string[]): string {
  for (const category of categories) {
    if (CATEGORY_LABELS[category]) return CATEGORY_LABELS[category]
  }
  return '应用'
}

function stableId(identity: string): string {
  return createHash('sha1').update(identity).digest('hex').slice(0, 20)
}

function dataUrlFor(buffer: Buffer, ext: string): string {
  const mime =
    ext === '.svg'
      ? 'image/svg+xml'
      : ext === '.png'
        ? 'image/png'
        : ext === '.jpg' || ext === '.jpeg'
          ? 'image/jpeg'
          : ext === '.webp'
            ? 'image/webp'
            : ext === '.ico'
              ? 'image/x-icon'
              : 'application/octet-stream'
  return `data:${mime};base64,${buffer.toString('base64')}`
}

/* ------------------------------- .desktop ------------------------------- */

interface DesktopEntry {
  file: string
  name: string
  genericName: string
  comment: string
  keywords: string[]
  categories: string[]
  icon: string
  exec: string
  terminal: boolean
  noDisplay: boolean
  hidden: boolean
  tryExec: string
}

export function parseDesktopEntry(raw: string, file: string): DesktopEntry | null {
  const lines = raw.split(/\r?\n/)
  let inGroup = false
  const values = new Map<string, string>()

  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.startsWith('#')) continue
    if (trimmed.startsWith('[')) {
      inGroup = trimmed === '[Desktop Entry]'
      continue
    }
    if (!inGroup) continue
    const index = trimmed.indexOf('=')
    if (index < 0) continue
    values.set(trimmed.slice(0, index), trimmed.slice(index + 1))
  }

  if (!values.size) return null
  const type = values.get('Type') ?? 'Application'
  if (type !== 'Application') return null
  if (values.get('Hidden') === 'true' || values.get('NoDisplay') === 'true') return null

  const localized = (key: string): string => {
    for (const candidate of [`${key}[zh_CN]`, `${key}[zh]`, `${key}[zh_Hans]`]) {
      const value = values.get(candidate)
      if (value) return value
    }
    return values.get(key) ?? ''
  }

  const name = localized('Name').trim()
  if (!name) return null

  return {
    file,
    name,
    genericName: localized('GenericName').trim(),
    comment: localized('Comment').trim(),
    keywords: (values.get('Keywords') ?? '')
      .split(';')
      .map((keyword) => keyword.trim())
      .filter(Boolean),
    categories: (values.get('Categories') ?? '')
      .split(';')
      .map((category) => category.trim())
      .filter(Boolean),
    icon: values.get('Icon')?.trim() ?? '',
    exec: values.get('Exec')?.trim() ?? '',
    terminal: values.get('Terminal') === 'true',
    noDisplay: values.get('NoDisplay') === 'true',
    hidden: values.get('Hidden') === 'true',
    tryExec: values.get('TryExec')?.trim() ?? ''
  }
}

function iconRoots(): string[] {
  return [
    join(homedir(), '.local/share/icons'),
    '/usr/share/icons',
    '/usr/local/share/icons',
    join(homedir(), '.icons'),
    '/var/lib/flatpak/exports/share/icons',
    join(homedir(), '.local/share/flatpak/exports/share/icons'),
    '/usr/share/pixmaps',
    join(homedir(), '.local/share/pixmaps')
  ]
}

function iconScore(path: string): number {
  let score = 0
  if (/\/scalable\//.test(path)) score += 80
  else if (path.includes('512x512')) score += 100
  else if (path.includes('256x256')) score += 95
  else if (path.includes('128x128')) score += 90
  else if (path.includes('96x96')) score += 85
  else if (path.includes('64x64')) score += 75
  else if (path.includes('48x48')) score += 65
  else if (path.includes('32x32')) score += 40
  else if (path.includes('24x24')) score += 20
  else score += 50
  if (path.endsWith('.svg')) score += 4
  if (path.includes('/pixmaps/')) score -= 10
  if (path.includes('/apps/')) score += 5
  return score
}

let iconIndex: Promise<Map<string, string>> | null = null

async function buildIconIndex(): Promise<Map<string, string>> {
  const index = new Map<string, string>()
  const roots = iconRoots()

  const walk = async (dir: string, depth: number): Promise<void> => {
    if (depth > 6) return
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      return
    }
    await Promise.all(
      entries.map(async (entry) => {
        const full = join(dir, entry.name)
        if (entry.isDirectory()) {
          await walk(full, depth + 1)
          return
        }
        if (!entry.isFile()) return
        const ext = extname(entry.name).toLowerCase()
        if (!['.png', '.svg', '.webp', '.jpg', '.jpeg'].includes(ext)) return
        const key = entry.name.slice(0, -ext.length)
        const current = index.get(key)
        if (!current || iconScore(full) > iconScore(current)) index.set(key, full)
      })
    )
  }

  await Promise.all(
    roots.filter((root) => existsSync(root)).map((root) => walk(root, 0))
  )
  return index
}

export async function resolveIconFile(iconName: string): Promise<string | null> {
  if (!iconName) return null
  if (isAbsolute(iconName) && existsSync(iconName)) return iconName
  const name = iconName.replace(/\.(png|svg|xpm)$/i, '')
  iconIndex ??= buildIconIndex()
  const index = await iconIndex
  return index.get(name) ?? null
}

async function loadIconData(iconName: string, cache: Map<string, string | null>): Promise<string | undefined> {
  if (cache.has(iconName)) return cache.get(iconName) ?? undefined
  const file = await resolveIconFile(iconName)
  if (!file) {
    cache.set(iconName, null)
    return undefined
  }
  try {
    const info = await stat(file)
    if (info.size > 800 * 1024) {
      cache.set(iconName, null)
      return undefined
    }
    const buffer = await readFile(file)
    const ext = extname(file).toLowerCase()
    if (!['.svg', '.png', '.jpg', '.jpeg', '.webp'].includes(ext)) {
      cache.set(iconName, null)
      return undefined
    }
    const data = dataUrlFor(buffer, ext)
    cache.set(iconName, data)
    return data
  } catch {
    cache.set(iconName, null)
    return undefined
  }
}

const desktopDirs = (): string[] => [
  join(homedir(), '.local/share/applications'),
  '/var/lib/flatpak/exports/share/applications',
  join(homedir(), '.local/share/flatpak/exports/share/applications'),
  '/usr/local/share/applications',
  '/usr/share/applications',
  '/var/lib/snapd/desktop/applications'
]

async function listDesktopFiles(dir: string): Promise<string[]> {
  try {
    const entries = await readdir(dir, { withFileTypes: true })
    return entries
      .filter((entry) => entry.isFile() && entry.name.endsWith('.desktop'))
      .map((entry) => join(dir, entry.name))
  } catch {
    return []
  }
}

const MAC_APP_DIRS = [
  '/Applications',
  '/System/Applications',
  '/System/Applications/Utilities',
  '/Applications/Utilities',
  join(homedir(), 'Applications')
]

async function listMacApps(): Promise<AppRecord[]> {
  const seen = new Set<string>()
  const records: AppRecord[] = []
  for (const dir of MAC_APP_DIRS) {
    if (!existsSync(dir)) continue
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      continue
    }
    for (const entry of entries) {
      if (!entry.name.endsWith('.app')) continue
      const path = join(dir, entry.name)
      if (seen.has(path)) continue
      seen.add(path)
      const name = basename(entry.name, '.app')
      const icon = await app.getFileIcon(path, { size: 'normal' }).catch(() => null)
      records.push({
        id: stableId(path),
        name,
        subtitle: '应用',
        keywords: [],
        icon: icon ? `data:image/png;base64,${icon.toPNG().toString('base64')}` : undefined,
        iconKey: 'app',
        launch: { type: 'bundle', path }
      })
    }
  }
  return records
}

function startMenuDirs(): string[] {
  const dirs: string[] = []
  if (process.env.ProgramData) {
    dirs.push(join(process.env.ProgramData, 'Microsoft', 'Windows', 'Start Menu', 'Programs'))
  }
  if (process.env.APPDATA) {
    dirs.push(join(process.env.APPDATA, 'Microsoft', 'Windows', 'Start Menu', 'Programs'))
  }
  return dirs
}

async function walkShortcuts(dir: string, out: string[] = []): Promise<string[]> {
  try {
    const entries = await readdir(dir, { withFileTypes: true })
    for (const entry of entries) {
      const full = join(dir, entry.name)
      if (entry.isDirectory()) await walkShortcuts(full, out)
      else if (/\.(lnk|url)$/i.test(entry.name)) out.push(full)
    }
  } catch {
    // ignored
  }
  return out
}

async function listWindowsApps(): Promise<AppRecord[]> {
  const files = (
    await Promise.all(startMenuDirs().map((dir) => walkShortcuts(dir)))
  ).flat()
  const records: AppRecord[] = []
  for (const file of files) {
    const name = basename(file).replace(/\.(lnk|url)$/i, '')
    if (!name || /uninstall|readme/i.test(name)) continue
    let icon: string | undefined
    try {
      const native = await app.getFileIcon(file, { size: 'normal' })
      icon = `data:image/png;base64,${native.toPNG().toString('base64')}`
    } catch {
      icon = undefined
    }
    records.push({
      id: stableId(file),
      name,
      subtitle: '应用',
      keywords: [],
      icon,
      iconKey: 'app',
      launch: { type: 'shortcut', path: file }
    })
  }
  return records
}

/* ------------------------------- discovery ------------------------------- */

export class AppDiscovery {
  private cache: AppRecord[] | null = null
  private records = new Map<string, AppRecord>()
  private iconCache = new Map<string, string | null>()
  private loading: Promise<AppEntry[]> | null = null

  constructor(private readonly platform: PlatformId) {}

  async list(): Promise<AppEntry[]> {
    if (this.cache) return this.cache.map(stripRecord)
    if (this.loading) return this.loading
    this.loading = this.discover()
      .then((records) => {
        this.cache = records
        this.records = new Map(records.map((record) => [record.id, record]))
        this.loading = null
        return records.map(stripRecord)
      })
      .catch((error) => {
        this.loading = null
        log.error('app discovery failed:', error)
        return []
      })
    return this.loading
  }

  async refresh(): Promise<AppEntry[]> {
    this.cache = null
    this.records.clear()
    return this.list()
  }

  async launch(id: string): Promise<{ ok: boolean; error?: string }> {
    if (!this.cache) await this.list()
    const record = this.records.get(id)
    if (!record) return { ok: false, error: '应用未找到，请重新加载应用列表' }
    try {
      if (record.launch.type === 'bundle') {
        await shell.openPath(record.launch.path)
        return { ok: true }
      }
      if (record.launch.type === 'shortcut') {
        await shell.openPath(record.launch.path)
        return { ok: true }
      }
      await this.launchDesktopFile(record.launch)
      return { ok: true }
    } catch (error) {
      log.warn('launch app failed:', record.name, error)
      return { ok: false, error: (error as Error).message }
    }
  }

  private launchDesktopFile(launch: LaunchDescriptor): Promise<void> {
    return new Promise((resolve, reject) => {
      // gio handles field codes, working directories and desktop activation.
      const child = spawn('gio', ['launch', launch.path], {
        detached: true,
        stdio: 'ignore'
      })
      child.on('error', () => this.launchExec(launch.exec ?? '').then(resolve, reject))
      child.on('exit', (code) => {
        if (code === 0) resolve()
        else this.launchExec(launch.exec ?? '').then(resolve, reject)
      })
      child.unref()
    })
  }

  private launchExec(exec: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const args = parseExec(exec)
      if (args.length === 0) {
        reject(new Error('应用启动命令为空'))
        return
      }
      const child = spawn(args[0], args.slice(1), { detached: true, stdio: 'ignore' })
      child.on('error', reject)
      child.on('spawn', () => {
        child.unref()
        resolve()
      })
    })
  }

  private async discover(): Promise<AppRecord[]> {
    if (this.platform === 'darwin') return listMacApps()
    if (this.platform === 'win32') return listWindowsApps()

    const files = (await Promise.all(desktopDirs().map((dir) => listDesktopFiles(dir)))).flat()
    const byDesktopId = new Map<string, { file: string; priority: number; entry: DesktopEntry }>()

    for (const file of files) {
      let raw: string
      try {
        raw = await readFile(file, 'utf8')
      } catch {
        continue
      }
      const entry = parseDesktopEntry(raw, file)
      if (!entry || entry.terminal) continue
      if (entry.tryExec && !commandExists(entry.tryExec)) continue
      const desktopId = basename(file, '.desktop')
      const priority = file.startsWith(homedir()) ? 3 : file.includes('flatpak') ? 2 : 1
      const existing = byDesktopId.get(desktopId)
      if (!existing || existing.priority < priority) {
        byDesktopId.set(desktopId, { file, priority, entry })
      }
    }

    const records: AppRecord[] = []
    for (const [desktopId, { file, entry }] of byDesktopId) {
      const icon = await loadIconData(entry.icon, this.iconCache)
      const subtitle = categoryLabel(entry.categories)
      records.push({
        id: stableId(desktopId),
        name: entry.name,
        subtitle,
        keywords: [entry.genericName, entry.comment, ...entry.keywords, ...entry.categories].filter(Boolean),
        icon,
        iconKey: 'app',
        launch: { type: 'desktop', path: file, exec: entry.exec }
      })
    }
    return records
  }
}

function stripRecord(record: AppRecord): AppEntry {
  const { launch: _launch, ...entry } = record
  return entry
}

/** Splits a desktop Exec line, handling quotes and removing %f/%u style field codes. */
export function parseExec(exec: string): string[] {
  const tokens: string[] = []
  let current = ''
  let quote: '"' | "'" | null = null
  let escaped = false

  for (const char of exec) {
    if (escaped) {
      current += char
      escaped = false
      continue
    }
    if (char === '\\') {
      escaped = true
      continue
    }
    if (quote) {
      if (char === quote) quote = null
      else current += char
      continue
    }
    if (char === '"' || char === "'") {
      quote = char
      continue
    }
    if (/\s/.test(char)) {
      if (current) {
        tokens.push(current)
        current = ''
      }
      continue
    }
    current += char
  }
  if (current) tokens.push(current)

  return tokens.filter((token) => !/^%[fFuUidDnNvmkc]$/.test(token))
}
