/**
 * Windows 开始菜单里装的是 `.lnk`（Shell Link）与 `.url`（Internet Shortcut）。
 *
 * Electron 的 `app.getFileIcon()` 在 Windows 上按扩展名取图标：对 `.lnk` 只会返回
 * 一张“带箭头的空白页”（所有应用长得一模一样），对 `.url` 返回通用网址图标。
 * 真正的图标记在快捷方式的目标可执行文件或 `IconLocation` 里，所以这里直接解析
 * 这两种文件格式（MS-SHLLINK 与 `.url` 的 INI），不依赖 PowerShell 等外部进程。
 *
 * 解析结果只用来找图标，不参与启动：启动仍然交给 `shell.openPath(shortcut)`，
 * 避免自己拼参数时丢掉工作目录、窗口状态和 UWP 激活信息。
 */

/** ShellLinkHeader 固定 0x4C 字节。 */
const LINK_HEADER_SIZE = 0x4c
/** LinkInfoHeaderSize 达到 0x24 时才带 LocalBasePath / CommonPathSuffix 的 Unicode 偏移。 */
const LINK_INFO_UNICODE_HEADER_SIZE = 0x24
const LINK_INFO_SIZE_FIELD = 0x1c

const LINK_FLAG = {
  hasLinkTargetIdList: 1 << 0,
  hasLinkInfo: 1 << 1,
  hasName: 1 << 2,
  hasRelativePath: 1 << 3,
  hasWorkingDir: 1 << 4,
  hasArguments: 1 << 5,
  hasIconLocation: 1 << 6,
  isUnicode: 1 << 7,
  forceNoLinkInfo: 1 << 8
} as const

/** EnvironmentVariableDataBlock：把带环境变量的目标/图标路径单独存一份。 */
const ENV_TARGET_BLOCK = 0xa0000001
const ENV_ICON_BLOCK = 0xa0000007

/** 环境变量数据块里 TargetAnsi / TargetUnicode 字段的固定长度。 */
const ENV_ANSI_PATH_LENGTH = 260
const ENV_UNICODE_PATH_LENGTH = 520

export interface WindowsShortcutInfo {
  /** 快捷方式指向的目标：`.lnk` 的 TargetPath，`.url` 的 URL。 */
  target?: string
  /** `IconLocation` / `IconFile` 指定的图标来源，通常是 `.exe`、`.dll` 或 `.ico`。 */
  iconFile?: string
  /** 图标在资源文件里的序号；负数表示资源 ID（取图时暂按默认图标处理）。 */
  iconIndex: number
  /** `.url` 里记录的网址。 */
  url?: string
}

interface ParsedString {
  value: string
  next: number
}

let gbkDecoder: TextDecoder | null | undefined

/**
 * ANSI 字符串按系统代码页编码，中文 Windows 上是 CP936。Node 的 `latin1` 会把
 * 中文路径变成乱码，所以先当 UTF-8/ASCII，再退到 GBK，最后才用 latin1 兜底。
 * Unicode 链接（Vista 之后基本都是）不会走到这里。
 */
function decodeAnsi(bytes: Buffer): string {
  if (bytes.length === 0) return ''
  if (bytes.every((byte) => byte < 0x80)) return bytes.toString('latin1')
  if (gbkDecoder === undefined) {
    try {
      gbkDecoder = new TextDecoder('gbk')
    } catch {
      // 精简版 Node/Electron 没有完整 ICU，只能放弃中文 ANSI 路径。
      gbkDecoder = null
    }
  }
  if (gbkDecoder) {
    const decoded = gbkDecoder.decode(bytes)
    if (!decoded.includes('\ufffd')) return decoded
  }
  return bytes.toString('latin1')
}

function readUtf16UntilNul(bytes: Buffer): string {
  let end = 0
  while (end + 1 < bytes.length && (bytes[end] !== 0 || bytes[end + 1] !== 0)) end += 2
  return bytes.subarray(0, end).toString('utf16le')
}

/** 读 LinkInfo 里 NUL 结尾的路径；`end` 是该结构的结束位置，越界按截断处理。 */
function readPathAt(raw: Buffer, start: number, end: number, unicode: boolean): string {
  if (start < 0 || start >= end) return ''
  if (unicode) {
    let stop = start
    while (stop + 1 < end && (raw[stop] !== 0 || raw[stop + 1] !== 0)) stop += 2
    return raw.subarray(start, stop).toString('utf16le')
  }
  let stop = start
  while (stop < end && raw[stop] !== 0) stop += 1
  return decodeAnsi(raw.subarray(start, stop))
}

/** `C:\Program Files\App\` + `app.exe` → 完整路径；两段都带分隔符时不会重复。 */
function joinWindowsPath(base: string, suffix: string): string {
  if (!base) return suffix
  if (!suffix) return base
  if (/[\\/]$/.test(base) && /^[\\/]/.test(suffix)) return base.slice(0, -1) + suffix
  if (/[\\/]$/.test(base) || /^[\\/]/.test(suffix)) return base + suffix
  return `${base}\\${suffix}`
}

function readLinkInfo(raw: Buffer, offset: number): { target?: string; next: number } | null {
  if (offset + LINK_INFO_SIZE_FIELD > raw.length) return null
  const size = raw.readUInt32LE(offset)
  const headerSize = raw.readUInt32LE(offset + 4)
  if (size < LINK_INFO_SIZE_FIELD || offset + size > raw.length) return null
  const end = offset + size
  const flags = raw.readUInt32LE(offset + 8)
  let target: string | undefined

  // 只有 VolumeIDAndLocalBasePath 置位时才有本地路径。
  if (flags & 0x1) {
    let localOffset = raw.readUInt32LE(offset + 16)
    let suffixOffset = raw.readUInt32LE(offset + 24)
    let unicode = false

    // 新格式同时给出 ANSI 与 Unicode 偏移，优先 Unicode（ANSI 可能丢中文）。
    if (headerSize >= LINK_INFO_UNICODE_HEADER_SIZE && offset + 0x24 <= end) {
      const localUnicode = raw.readUInt32LE(offset + 28)
      const suffixUnicode = raw.readUInt32LE(offset + 32)
      if (localUnicode) {
        localOffset = localUnicode
        suffixOffset = suffixUnicode
        unicode = true
      }
    }

    if (localOffset) {
      const base = readPathAt(raw, offset + localOffset, end, unicode)
      const suffix = suffixOffset ? readPathAt(raw, offset + suffixOffset, end, unicode) : ''
      target = joinWindowsPath(base, suffix) || undefined
    }
  }

  return { target, next: end }
}

/** StringData 段按固定顺序排列，即使中间某个字段缺失也要按顺序跳过。 */
const STRING_FIELDS: ReadonlyArray<{ flag: number; key: string }> = [
  { flag: LINK_FLAG.hasName, key: 'name' },
  { flag: LINK_FLAG.hasRelativePath, key: 'relativePath' },
  { flag: LINK_FLAG.hasWorkingDir, key: 'workingDir' },
  { flag: LINK_FLAG.hasArguments, key: 'arguments' },
  { flag: LINK_FLAG.hasIconLocation, key: 'iconLocation' }
]

function readStringData(raw: Buffer, offset: number, unicode: boolean): ParsedString | null {
  if (offset + 2 > raw.length) return null
  const count = raw.readUInt16LE(offset)
  const start = offset + 2
  const end = start + count * (unicode ? 2 : 1)
  if (end > raw.length) return null
  const value = unicode
    ? raw.subarray(start, end).toString('utf16le')
    : decodeAnsi(raw.subarray(start, end))
  return { value: value.replace(/\0+$/, ''), next: end }
}

/** EnvironmentVariableDataBlock / IconEnvironmentDataBlock 的载荷：ANSI 260 字节 + Unicode 520 字节。 */
function readEnvironmentPath(payload: Buffer): string {
  if (payload.length >= ENV_ANSI_PATH_LENGTH + ENV_UNICODE_PATH_LENGTH) {
    const unicode = readUtf16UntilNul(
      payload.subarray(ENV_ANSI_PATH_LENGTH, ENV_ANSI_PATH_LENGTH + ENV_UNICODE_PATH_LENGTH)
    )
    if (unicode) return unicode
  }
  const ansi = payload.subarray(0, Math.min(ENV_ANSI_PATH_LENGTH, payload.length))
  const stop = ansi.indexOf(0)
  return decodeAnsi(stop >= 0 ? ansi.subarray(0, stop) : ansi)
}

/**
 * ExtraData 是 (BlockSize, BlockSignature, payload) 的序列，BlockSize < 8 表示结束
 * （终止块通常只有 4 字节的 BlockSize）。
 * 只取目标和图标的环境变量块：UWP/安装器写出的快捷方式常常把路径放在这里，
 * 而不是 LinkInfo 里。
 */
function readExtraBlocks(raw: Buffer, offset: number): { target?: string; icon?: string } {
  const result: { target?: string; icon?: string } = {}
  let cursor = offset
  while (cursor + 8 <= raw.length) {
    const size = raw.readUInt32LE(cursor)
    if (size < 8 || cursor + size > raw.length) break
    const signature = raw.readUInt32LE(cursor + 4)
    const payload = raw.subarray(cursor + 8, cursor + size)
    if (signature === ENV_TARGET_BLOCK) result.target = readEnvironmentPath(payload) || undefined
    else if (signature === ENV_ICON_BLOCK) result.icon = readEnvironmentPath(payload) || undefined
    cursor += size
  }
  return result
}

export function expandWindowsEnv(value: string, env: NodeJS.ProcessEnv = process.env): string {
  const lookup = new Map<string, string>()
  for (const [key, entry] of Object.entries(env)) {
    if (entry !== undefined) lookup.set(key.toLowerCase(), entry)
  }
  return value.replace(/%([^%]+)%/g, (whole, name: string) => lookup.get(name.toLowerCase()) ?? whole)
}

/** `"C:\App\app.exe",3` → 路径与序号；`C:\App\app.exe` 或 `,0` 也都要能解。 */
export function splitIconLocation(value: string): { path: string; index: number } {
  const text = value.trim()
  if (!text) return { path: '', index: 0 }
  const comma = text.lastIndexOf(',')
  if (comma >= 0) {
    const tail = text.slice(comma + 1).trim()
    if (/^-?\d+$/.test(tail)) {
      const path = text
        .slice(0, comma)
        .trim()
        .replace(/^"(.*)"$/, '$1')
        .trim()
      return { path, index: Number.parseInt(tail, 10) }
    }
  }
  return { path: text.replace(/^"(.*)"$/, '$1').trim(), index: 0 }
}

/** 解析 Shell Link（`.lnk`）字节；结构损坏时返回 null，不抛异常。 */
export function parseShellLink(raw: Buffer): WindowsShortcutInfo | null {
  if (raw.length < LINK_HEADER_SIZE || raw.readUInt32LE(0) !== LINK_HEADER_SIZE) return null

  const flags = raw.readUInt32LE(20)
  const unicode = (flags & LINK_FLAG.isUnicode) !== 0
  let offset = LINK_HEADER_SIZE

  if (flags & LINK_FLAG.hasLinkTargetIdList) {
    if (offset + 2 > raw.length) return null
    offset += 2 + raw.readUInt16LE(offset)
    if (offset > raw.length) return null
  }

  let linkInfoTarget: string | undefined
  if (flags & LINK_FLAG.hasLinkInfo) {
    // 即使 ForceNoLinkInfo 置位也要按结构长度跳过：字符串段紧跟在它后面。
    const linkInfo = readLinkInfo(raw, offset)
    if (!linkInfo) return null
    linkInfoTarget = flags & LINK_FLAG.forceNoLinkInfo ? undefined : linkInfo.target
    offset = linkInfo.next
  }

  const strings = new Map<string, string>()
  let truncated = false
  for (const { flag, key } of STRING_FIELDS) {
    if (!(flags & flag)) continue
    const parsed = readStringData(raw, offset, unicode)
    if (!parsed) {
      truncated = true
      break
    }
    strings.set(key, parsed.value)
    offset = parsed.next
  }

  // 字符串段读坏了就不再猜测 ExtraData 的起点，避免把随机字节当路径。
  const extra: { target?: string; icon?: string } = truncated ? {} : readExtraBlocks(raw, offset)
  const iconLocation = splitIconLocation(strings.get('iconLocation') ?? '')
  const target = linkInfoTarget || extra.target

  return {
    target,
    iconFile: iconLocation.path || extra.icon || undefined,
    iconIndex: iconLocation.index
  }
}

/** 解析 Internet Shortcut（`.url`）文本；缺失字段留空。 */
export function parseInternetShortcut(text: string): WindowsShortcutInfo {
  const values = new Map<string, string>()
  for (const line of text.split(/\r?\n/)) {
    const match = /^\s*([A-Za-z][A-Za-z0-9]*)\s*=\s*(.*)$/.exec(line)
    if (match) values.set(match[1].toLowerCase(), match[2].trim())
  }
  const url = values.get('url') || undefined
  const iconFile = values.get('iconfile') || undefined
  const iconIndex = Number.parseInt(values.get('iconindex') ?? '', 10)
  return {
    target: url,
    iconFile,
    iconIndex: Number.isFinite(iconIndex) ? iconIndex : 0,
    url
  }
}

function isAbsoluteWindowsPath(value: string): boolean {
  return /^[a-z]:[\\/]/i.test(value) || value.startsWith('\\\\')
}

/**
 * 按优先级给出可用的图标来源：快捷方式显式指定的 exe/ico → 目标可执行文件 →
 * 显式指定的 `.dll`。
 *
 * `.dll` 的图标要靠资源序号定位（`imageres.dll,-1024` 这种还带资源 ID），
 * 取默认图标基本是错的，所以放在最后；大多数应用的目标 exe 本身就带正确的图标。
 * `.url` 的目标是网址，不是文件路径，会被过滤掉。
 */
export function shortcutIconCandidates(
  info: WindowsShortcutInfo,
  env: NodeJS.ProcessEnv = process.env
): string[] {
  const candidates: string[] = []
  const add = (value: string | undefined): void => {
    if (!value) return
    const expanded = expandWindowsEnv(value.trim(), env)
    if (!isAbsoluteWindowsPath(expanded) || candidates.includes(expanded)) return
    candidates.push(expanded)
  }

  const icon = info.iconFile
  if (icon && !/\.dll$/i.test(icon.trim())) add(icon)
  add(info.target)
  add(icon)
  return candidates
}
