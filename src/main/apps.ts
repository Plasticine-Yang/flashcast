import { app, nativeImage, shell, type NativeImage } from 'electron'
import { execFile, spawn } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'
import { mkdir, readdir, readFile, rename, rm, stat, writeFile } from 'node:fs/promises'
import { homedir, tmpdir } from 'node:os'
import { basename, delimiter, dirname, extname, isAbsolute, join } from 'node:path'
import { promisify } from 'node:util'
import type { AppEntry, PlatformId } from '@shared/types'
import { log } from './logger'
import { aliasTargetsFromBytes } from './mac-alias'
import { resolveMacAppName } from './mac-app-name'
import { userDataDir } from './paths'
import { parseInternetShortcut, parseShellLink, shortcutIconCandidates } from './windows-shortcuts'

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

/**
 * Finder 跟着系统外壳放在 CoreServices 下，不在上面几个应用目录里。其余 CoreServices
 * 应用是系统面板和后台代理（AirDropUI、ControlCenter…），不该出现在应用列表里，
 * 所以只补 Finder，不扫整个目录。
 */
const MAC_EXTRA_APPS = ['/System/Library/CoreServices/Finder.app']

/** 应用行里的图标按 32pt 展示，64px 足够覆盖 2x 屏。 */
const MAC_ICON_MAX_PX = 64
/** 转换逻辑或取图方式变化时改这里，旧的缓存文件自然失效。 */
const MAC_ICON_CACHE_VERSION = 'v1'
/** QuickLook 缩略图偶尔会卡住（实测 CLI 版会挂起），兜底路径必须设上限。 */
const MAC_THUMBNAIL_TIMEOUT_MS = 4000

/** sips 的临时输出文件名序号，同一次运行里自增避免撞名。 */
let tempIconSeq = 0

const execFileAsync = promisify(execFile)

/** 以固定并发度执行异步任务，结果保持输入顺序。 */
async function mapLimit<T, R>(
  items: T[],
  limit: number,
  task: (item: T) => Promise<R>
): Promise<R[]> {
  const results = new Array<R>(items.length)
  let cursor = 0
  const worker = async (): Promise<void> => {
    while (cursor < items.length) {
      const index = cursor++
      results[index] = await task(items[index])
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker))
  return results
}

function withTimeout<T>(promise: Promise<T>, ms: number): Promise<T | null> {
  return Promise.race([
    promise,
    new Promise<null>((resolve) => {
      setTimeout(() => resolve(null), ms).unref()
    })
  ])
}

/**
 * Finder 里“制作替身”得到的 .app 只是几百字节的别名文件，真正的 bundle 在别处。
 * 启动不受影响（open 会自己解析替身），但取图标必须跟过去。
 */
async function resolveMacAppPath(appPath: string): Promise<string> {
  try {
    const info = await stat(appPath)
    // bundle 本身是目录；别名是普通文件，读字节就能拿到目标路径。
    if (!info.isFile() || info.size > 64 * 1024) return appPath
    const targets = aliasTargetsFromBytes(await readFile(appPath))
    return targets.find((target) => existsSync(target)) ?? appPath
  } catch {
    return appPath
  }
}

/**
 * Info.plist 里声明的 bundle 图标名。Info.plist 通常是 binary plist，
 * 交给系统自带的 plutil 转成 XML 再取字段，比在 Node 里解析 binary 格式稳。
 */
async function macBundleInfo(appPath: string): Promise<{ iconName: string; bundleName: string }> {
  try {
    const { stdout } = await execFileAsync(
      '/usr/bin/plutil',
      ['-convert', 'xml1', '-o', '-', join(appPath, 'Contents/Info.plist')],
      { timeout: 3000, maxBuffer: 8 * 1024 * 1024 }
    )
    const pick = (key: string): string => {
      const match = new RegExp(`<key>${key}</key>\\s*<string>([^<]*)</string>`).exec(stdout)
      return match?.[1].trim() ?? ''
    }
    return { iconName: pick('CFBundleIconFile'), bundleName: pick('CFBundleName') }
  } catch {
    return { iconName: '', bundleName: '' }
  }
}

/** .app 的图标是 Contents/Resources 下的 .icns 文件。 */
async function macIconFile(appPath: string): Promise<string | null> {
  const resources = join(appPath, 'Contents/Resources')
  const { iconName, bundleName } = await macBundleInfo(appPath)
  const declared = [iconName, bundleName, basename(appPath, '.app')]
    .filter(Boolean)
    .map((name) => join(resources, name.endsWith('.icns') ? name : `${name}.icns`))
  const conventional = ['icon.icns', 'app.icns', 'AppIcon.icns'].map((name) => join(resources, name))
  for (const candidate of [...declared, ...conventional]) {
    if (existsSync(candidate)) return candidate
  }
  // 少数应用没在 Info.plist 里声明图标名，退化成 Resources 里的第一个 icns。
  try {
    const entries = await readdir(resources)
    const found = entries.find((entry) => entry.toLowerCase().endsWith('.icns'))
    return found ? join(resources, found) : null
  } catch {
    return null
  }
}

/** 部分 icns（如 Books 的）解出来是全透明的，这种要当作没有图标。 */
function hasVisiblePixels(image: NativeImage): boolean {
  const bitmap = image.toBitmap()
  for (let index = 3; index < bitmap.length; index += 4) {
    if (bitmap[index] > 8) return true
  }
  return false
}

/** .icns 转 64px PNG。Electron 的 nativeImage 读不了 icns，交给系统自带的 sips。 */
async function convertMacIcon(appPath: string): Promise<Buffer | null> {
  const iconFile = await macIconFile(appPath)
  if (!iconFile) return null
  const temp = join(tmpdir(), `flashcast-icon-${process.pid}-${(tempIconSeq += 1)}.png`)
  try {
    await execFileAsync(
      '/usr/bin/sips',
      [
        '-s',
        'format',
        'png',
        '--resampleHeightWidthMax',
        String(MAC_ICON_MAX_PX),
        iconFile,
        '--out',
        temp
      ],
      { timeout: 5000 }
    )
    const buffer = await readFile(temp)
    const image = nativeImage.createFromBuffer(buffer)
    return !image.isEmpty() && hasVisiblePixels(image) ? buffer : null
  } catch {
    return null
  } finally {
    await rm(temp, { force: true })
  }
}

/**
 * QuickLook 缩略图兜底：新版系统应用（图标只在 Assets.car 里，如 Books）
 * 和一些没有 icns 的 bundle 只能靠它。失败或超时都当作没有图标。
 */
async function thumbnailMacIcon(appPath: string): Promise<Buffer | null> {
  try {
    const image = await withTimeout(
      nativeImage.createThumbnailFromPath(appPath, {
        width: MAC_ICON_MAX_PX,
        height: MAC_ICON_MAX_PX
      }),
      MAC_THUMBNAIL_TIMEOUT_MS
    )
    if (!image || image.isEmpty() || !hasVisiblePixels(image)) return null
    return image.toPNG()
  } catch {
    return null
  }
}

/**
 * macOS 的 app.getFileIcon 只会返回通用的空白应用图（实测 32x32 纯白占位），
 * 所以这里自己取图：优先 bundle 自带的 icns，其次 QuickLook 缩略图。
 * 结果按应用 + bundle 修改时间缓存在 userData 下，避免每次启动重复转换。
 */
async function macAppIcon(appPath: string): Promise<string | undefined> {
  try {
    const target = await resolveMacAppPath(appPath)
    const info = await stat(target)
    const key = createHash('sha1')
      .update(`${MAC_ICON_CACHE_VERSION}:${target}:${info.mtimeMs}`)
      .digest('hex')
      .slice(0, 20)
    const cacheFile = join(userDataDir(), 'app-icons', `${key}.png`)
    try {
      const cached = await readFile(cacheFile)
      if (cached.length) return dataUrlFor(cached, '.png')
    } catch {
      // 首次运行还没有缓存，继续转换。
    }
    const buffer = (await convertMacIcon(target)) ?? (await thumbnailMacIcon(target))
    if (!buffer) return undefined
    await mkdir(dirname(cacheFile), { recursive: true })
    const temp = `${cacheFile}.${process.pid}.tmp`
    await writeFile(temp, buffer)
    // 先写临时文件再改名：并发转换或中断都不会留下半个 PNG 被后续命中。
    await rename(temp, cacheFile)
    return dataUrlFor(buffer, '.png')
  } catch (error) {
    log.warn('mac app icon failed:', basename(appPath), (error as Error).message)
    return undefined
  }
}

async function listMacApps(): Promise<AppRecord[]> {
  const paths: string[] = []
  const seen = new Set<string>()
  const add = (path: string): void => {
    if (seen.has(path)) return
    seen.add(path)
    paths.push(path)
  }
  for (const dir of MAC_APP_DIRS) {
    if (!existsSync(dir)) continue
    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      continue
    }
    for (const entry of entries) {
      if (entry.name.endsWith('.app')) add(join(dir, entry.name))
    }
  }
  for (const path of MAC_EXTRA_APPS) {
    if (existsSync(path)) add(path)
  }

  // 名字要读本地化资源，图标要拉起 sips；两边都限并发，避免一次创建上百个进程。
  const [names, icons] = await Promise.all([
    mapLimit(paths, 6, resolveMacAppName),
    mapLimit(paths, 6, macAppIcon)
  ])
  return paths.map((path, index): AppRecord => {
    return {
      id: stableId(path),
      name: names[index].name,
      subtitle: '应用',
      keywords: names[index].keywords,
      icon: icons[index],
      iconKey: 'app',
      launch: { type: 'bundle', path }
    }
  })
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

/** 应用行里的图标按 32pt 展示：96px 覆盖 3x 屏，同时避免 100 个应用撑大 IPC 载荷。 */
const WIN_ICON_MAX_PX = 96
/** 解析/取图逻辑变化时改这里，旧的缓存文件自然失效。 */
const WIN_ICON_CACHE_VERSION = 'v1'
const WIN_ICON_TIMEOUT_MS = 4000

function decodeShortcutText(raw: Buffer): string {
  // 少数 .url 是 UTF-16LE（带 BOM），其余按 UTF-8/ANSI 读；ASCII 部分两种编码一致。
  if (raw[0] === 0xff && raw[1] === 0xfe) return raw.subarray(2).toString('utf16le')
  return raw.toString('utf8').replace(/^\uFEFF/, '')
}

/**
 * 读快捷方式拿图标来源。`.lnk` 走 Shell Link 字节解析，`.url` 读 INI，
 * 都不启动子进程，也不会触发系统对脚本/COM 的拦截。
 */
async function readWindowsShortcut(file: string) {
  try {
    const info = await stat(file)
    if (!info.isFile() || info.size > 1024 * 1024) return null
    const raw = await readFile(file)
    return /\.url$/i.test(file) ? parseInternetShortcut(decodeShortcutText(raw)) : parseShellLink(raw)
  } catch {
    return null
  }
}

/** 取 exe/dll/ico 里的图标：优先 96px 缩略图，失败再退到 shell 的 32px 图标。 */
async function extractWindowsIcon(source: string): Promise<Buffer | null> {
  if (/\.(png|jpe?g|webp)$/i.test(source)) {
    try {
      const image = nativeImage.createFromBuffer(await readFile(source))
      if (!image.isEmpty() && hasVisiblePixels(image)) return image.toPNG()
    } catch {
      // 读不出图片时继续尝试 shell 图标。
    }
  }
  try {
    const thumbnail = await withTimeout(
      nativeImage.createThumbnailFromPath(source, {
        width: WIN_ICON_MAX_PX,
        height: WIN_ICON_MAX_PX
      }),
      WIN_ICON_TIMEOUT_MS
    )
    if (thumbnail && !thumbnail.isEmpty() && hasVisiblePixels(thumbnail)) return thumbnail.toPNG()
  } catch {
    // 有些文件类型没有缩略图提供者，交给 getFileIcon。
  }
  try {
    const icon = await app.getFileIcon(source, { size: 'large' })
    if (!icon.isEmpty() && hasVisiblePixels(icon)) return icon.toPNG()
  } catch {
    // ignored
  }
  return null
}

/**
 * Windows 开始菜单的 `.lnk` 本身没有应用图标，`app.getFileIcon(shortcut)` 只会返回
 * “带箭头的空白页”。这里解析出快捷方式的目标 exe / IconLocation，从那里取图并按
 * 快捷方式与图标来源的修改时间缓存，避免每次启动重复调用 shell。
 */
async function windowsAppIcon(shortcutPath: string): Promise<string | undefined> {
  try {
    const shortcutInfo = await stat(shortcutPath)
    const shortcut = await readWindowsShortcut(shortcutPath)
    let source: { path: string; mtimeMs: number } | undefined
    for (const candidate of shortcut ? shortcutIconCandidates(shortcut) : []) {
      try {
        const info = await stat(candidate)
        if (info.isFile()) {
          source = { path: candidate, mtimeMs: info.mtimeMs }
          break
        }
      } catch {
        // 目标已卸载或移动，换下一个候选。
      }
    }

    const key = createHash('sha1')
      .update(
        `${WIN_ICON_CACHE_VERSION}:${shortcutPath}:${shortcutInfo.mtimeMs}:${source?.path ?? ''}:${source?.mtimeMs ?? 0}:${shortcut?.iconIndex ?? 0}`
      )
      .digest('hex')
      .slice(0, 20)
    const cacheFile = join(userDataDir(), 'app-icons', `win-${key}.png`)
    try {
      const cached = await readFile(cacheFile)
      if (cached.length) return dataUrlFor(cached, '.png')
    } catch {
      // 首次运行还没有缓存。
    }

    let buffer = source ? await extractWindowsIcon(source.path) : null
    // 解析不到目标时，.url 还能用系统给的网址图标；.lnk 的空白页图标还不如
    // 让渲染层回退到统一的应用图标。
    if (!buffer && /\.url$/i.test(shortcutPath)) {
      try {
        const icon = await app.getFileIcon(shortcutPath, { size: 'normal' })
        if (!icon.isEmpty() && hasVisiblePixels(icon)) buffer = icon.toPNG()
      } catch {
        // ignored
      }
    }
    if (!buffer) return undefined

    await mkdir(dirname(cacheFile), { recursive: true })
    const temp = `${cacheFile}.${process.pid}.tmp`
    await writeFile(temp, buffer)
    await rename(temp, cacheFile)
    return dataUrlFor(buffer, '.png')
  } catch (error) {
    log.warn('windows app icon failed:', basename(shortcutPath), (error as Error).message)
    return undefined
  }
}

async function listWindowsApps(): Promise<AppRecord[]> {
  const files = (
    await Promise.all(startMenuDirs().map((dir) => walkShortcuts(dir)))
  ).flat()
  const visible = files.filter((file) => {
    const name = basename(file).replace(/\.(lnk|url)$/i, '')
    return Boolean(name) && !/uninstall|readme/i.test(name)
  })
  // 每个图标都要读快捷方式并可能调用 shell，限制并发避免拖慢首次发现。
  const icons = await mapLimit(visible, 6, windowsAppIcon)
  return visible.map((file, index): AppRecord => ({
    id: stableId(file),
    name: basename(file).replace(/\.(lnk|url)$/i, ''),
    subtitle: '应用',
    keywords: [],
    icon: icons[index],
    iconKey: 'app',
    launch: { type: 'shortcut', path: file }
  }))
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
