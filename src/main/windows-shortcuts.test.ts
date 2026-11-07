import { describe, expect, it } from 'vitest'
import {
  expandWindowsEnv,
  parseInternetShortcut,
  parseShellLink,
  shortcutIconCandidates,
  splitIconLocation
} from './windows-shortcuts'

const HEADER_SIZE = 0x4c

function linkHeader(flags: number, iconIndex = 0): Buffer {
  const buffer = Buffer.alloc(HEADER_SIZE)
  buffer.writeUInt32LE(HEADER_SIZE, 0)
  buffer.writeUInt32LE(flags, 20)
  buffer.writeInt32LE(iconIndex, 56)
  return buffer
}

/** LinkTargetIDList：2 字节长度 + IDList（内容不参与解析，只要长度对得上）。 */
function linkTargetIdList(): Buffer {
  const idList = Buffer.from([0x14, 0x00, 0x1f, 0x50, 0x00, 0x00])
  const head = Buffer.alloc(2)
  head.writeUInt16LE(idList.length, 0)
  return Buffer.concat([head, idList])
}

/** 按 MS-SHLLINK 拼一个 LinkInfo：VolumeID + LocalBasePath + CommonPathSuffix。 */
function linkInfo(basePath: string, suffix: string, unicode: boolean): Buffer {
  const encode = (value: string): Buffer =>
    unicode ? Buffer.from(`${value}\0`, 'utf16le') : Buffer.from(`${value}\0`, 'latin1')
  const volume = Buffer.alloc(0x11)
  const base = encode(basePath)
  const tail = encode(suffix)
  const headerSize = unicode ? 0x24 : 0x1c
  const header = Buffer.alloc(headerSize)
  const baseOffset = headerSize + volume.length
  const suffixOffset = baseOffset + base.length
  header.writeUInt32LE(suffixOffset + tail.length, 0)
  header.writeUInt32LE(headerSize, 4)
  header.writeUInt32LE(0x1, 8) // VolumeIDAndLocalBasePath
  header.writeUInt32LE(headerSize, 12)
  if (unicode) {
    header.writeUInt32LE(baseOffset, 28)
    header.writeUInt32LE(suffixOffset, 32)
  } else {
    header.writeUInt32LE(baseOffset, 16)
    header.writeUInt32LE(suffixOffset, 24)
  }
  return Buffer.concat([header, volume, base, tail])
}

function stringData(value: string, unicode: boolean): Buffer {
  const body = unicode ? Buffer.from(value, 'utf16le') : Buffer.from(value, 'latin1')
  const head = Buffer.alloc(2)
  head.writeUInt16LE(value.length, 0)
  return Buffer.concat([head, body])
}

/** EnvironmentVariableDataBlock / IconEnvironmentDataBlock：8 字节头 + ANSI 260 + Unicode 520。 */
function environmentBlock(signature: number, value: string): Buffer {
  const payload = Buffer.alloc(260 + 520)
  Buffer.from(value, 'latin1').copy(payload, 0)
  Buffer.from(`${value}\0`, 'utf16le').copy(payload, 260)
  const head = Buffer.alloc(8)
  head.writeUInt32LE(head.length + payload.length, 0)
  head.writeUInt32LE(signature, 4)
  return Buffer.concat([head, payload])
}

const extraDataTerminator = Buffer.alloc(4)

interface LinkParts {
  unicode?: boolean
  idList?: boolean
  linkInfo?: Buffer
  strings?: Array<{ flag: number; value: string }>
  extra?: Buffer[]
  iconIndex?: number
  forceNoLinkInfo?: boolean
}

function buildShellLink(parts: LinkParts): Buffer {
  const unicode = parts.unicode ?? true
  let flags = unicode ? 0x80 : 0
  if (parts.idList) flags |= 0x1
  if (parts.linkInfo) flags |= 0x2
  if (parts.forceNoLinkInfo) flags |= 0x100
  for (const entry of parts.strings ?? []) flags |= entry.flag

  const chunks = [linkHeader(flags, parts.iconIndex ?? 0)]
  if (parts.idList) chunks.push(linkTargetIdList())
  if (parts.linkInfo) chunks.push(parts.linkInfo)
  for (const entry of parts.strings ?? []) chunks.push(stringData(entry.value, unicode))
  if (parts.extra) chunks.push(...parts.extra)
  return Buffer.concat(chunks)
}

const HAS_ICON_LOCATION = 0x40

describe('parseShellLink', () => {
  it('从 Unicode LinkInfo 解出目标，并带上 IconLocation 的序号', () => {
    const raw = buildShellLink({
      linkInfo: linkInfo('C:\\Program Files\\Example\\', 'example.exe', true),
      strings: [
        { flag: HAS_ICON_LOCATION, value: '"C:\\Program Files\\Example\\example.exe",2' }
      ]
    })
    expect(parseShellLink(raw)).toEqual({
      target: 'C:\\Program Files\\Example\\example.exe',
      iconFile: 'C:\\Program Files\\Example\\example.exe',
      iconIndex: 2
    })
  })

  it('只有 CommonPathSuffix 补全路径时也能拼出完整目标', () => {
    const raw = buildShellLink({
      linkInfo: linkInfo('C:\\Program Files\\Example', 'example.exe', true)
    })
    expect(parseShellLink(raw)?.target).toBe('C:\\Program Files\\Example\\example.exe')
  })

  it('老式 ANSI 链接（无 Unicode 偏移）也能解析', () => {
    const raw = buildShellLink({
      unicode: false,
      linkInfo: linkInfo('C:\\Tools\\', 'tool.exe', false)
    })
    expect(parseShellLink(raw)?.target).toBe('C:\\Tools\\tool.exe')
  })

  it('跳过 LinkTargetIDList 后再读 LinkInfo', () => {
    const raw = buildShellLink({
      idList: true,
      linkInfo: linkInfo('C:\\Tools\\', 'tool.exe', true)
    })
    expect(parseShellLink(raw)?.target).toBe('C:\\Tools\\tool.exe')
  })

  it('StringData 中间字段缺失时按顺序读 IconLocation', () => {
    const raw = buildShellLink({
      strings: [{ flag: HAS_ICON_LOCATION, value: 'C:\\Common\\shell.ico' }]
    })
    expect(parseShellLink(raw)).toMatchObject({ iconFile: 'C:\\Common\\shell.ico', iconIndex: 0 })
  })

  it('LinkInfo 缺失时退回 EnvironmentVariableDataBlock 里的目标', () => {
    const raw = buildShellLink({
      extra: [environmentBlock(0xa0000001, '%ProgramFiles%\\Example\\example.exe'), extraDataTerminator]
    })
    expect(parseShellLink(raw)?.target).toBe('%ProgramFiles%\\Example\\example.exe')
  })

  it('IconEnvironmentDataBlock 作为图标来源兜底', () => {
    const raw = buildShellLink({
      extra: [environmentBlock(0xa0000007, '%SystemRoot%\\system32\\imageres.dll'), extraDataTerminator]
    })
    expect(parseShellLink(raw)?.iconFile).toBe('%SystemRoot%\\system32\\imageres.dll')
  })

  it('ForceNoLinkInfo 置位时忽略 LinkInfo 的目标', () => {
    const raw = buildShellLink({
      linkInfo: linkInfo('C:\\Tools\\', 'tool.exe', true),
      forceNoLinkInfo: true
    })
    expect(parseShellLink(raw)?.target).toBeUndefined()
  })

  it('结构被截断时只返回已经读到的字段，不抛异常', () => {
    const raw = buildShellLink({ strings: [{ flag: HAS_ICON_LOCATION, value: 'C:\\App\\icon.ico' }] })
    const truncated = raw.subarray(0, HEADER_SIZE + 4)
    expect(() => parseShellLink(truncated)).not.toThrow()
    expect(parseShellLink(truncated)?.iconFile).toBeUndefined()
  })

  it('不是 Shell Link 时返回 null', () => {
    expect(parseShellLink(Buffer.alloc(0))).toBeNull()
    expect(parseShellLink(Buffer.alloc(HEADER_SIZE))).toBeNull()
    expect(parseShellLink(Buffer.from('not a shortcut', 'utf8'))).toBeNull()
  })
})

describe('parseInternetShortcut', () => {
  it('解析网址与自定义图标', () => {
    const text = [
      '[InternetShortcut]',
      'URL=https://example.com/',
      'IconFile=C:\\Icons\\example.ico',
      'IconIndex=3',
      ''
    ].join('\r\n')
    expect(parseInternetShortcut(text)).toEqual({
      target: 'https://example.com/',
      iconFile: 'C:\\Icons\\example.ico',
      iconIndex: 3,
      url: 'https://example.com/'
    })
  })

  it('没有 IconFile 时只留下网址', () => {
    const info = parseInternetShortcut('[InternetShortcut]\nURL=https://example.com/\n')
    expect(info.url).toBe('https://example.com/')
    expect(info.iconFile).toBeUndefined()
    expect(info.iconIndex).toBe(0)
  })
})

describe('splitIconLocation', () => {
  it('拆出路径与资源序号', () => {
    expect(splitIconLocation('C:\\App\\app.exe,2')).toEqual({ path: 'C:\\App\\app.exe', index: 2 })
    expect(splitIconLocation('"C:\\Program Files\\App\\app.exe",0')).toEqual({
      path: 'C:\\Program Files\\App\\app.exe',
      index: 0
    })
    expect(splitIconLocation('C:\\App\\app.ico')).toEqual({ path: 'C:\\App\\app.ico', index: 0 })
  })

  it('只有序号时路径为空', () => {
    expect(splitIconLocation(',0')).toEqual({ path: '', index: 0 })
    expect(splitIconLocation('')).toEqual({ path: '', index: 0 })
  })

  it('逗号后不是数字时整串当路径', () => {
    expect(splitIconLocation('C:\\App, Inc\\app.exe')).toEqual({
      path: 'C:\\App, Inc\\app.exe',
      index: 0
    })
  })
})

describe('expandWindowsEnv', () => {
  it('大小写不敏感地展开环境变量，未知变量保持原样', () => {
    const env = { ProgramFiles: 'C:\\Program Files', SystemRoot: 'C:\\Windows' }
    expect(expandWindowsEnv('%PROGRAMFILES%\\App\\app.exe', env)).toBe('C:\\Program Files\\App\\app.exe')
    expect(expandWindowsEnv('%SystemRoot%\\system32\\imageres.dll', env)).toBe(
      'C:\\Windows\\system32\\imageres.dll'
    )
    expect(expandWindowsEnv('%MISSING%\\app.exe', env)).toBe('%MISSING%\\app.exe')
  })
})

describe('shortcutIconCandidates', () => {
  it('优先 IconLocation，其次目标可执行文件', () => {
    expect(
      shortcutIconCandidates(
        { target: 'C:\\App\\app.exe', iconFile: 'C:\\Icons\\app.ico', iconIndex: 0 },
        {}
      )
    ).toEqual(['C:\\Icons\\app.ico', 'C:\\App\\app.exe'])
  })

  it('IconLocation 指向资源 DLL 时优先目标可执行文件', () => {
    expect(
      shortcutIconCandidates(
        {
          target: 'C:\\App\\app.exe',
          iconFile: 'C:\\Windows\\system32\\imageres.dll',
          iconIndex: -1024
        },
        {}
      )
    ).toEqual(['C:\\App\\app.exe', 'C:\\Windows\\system32\\imageres.dll'])
  })

  it('展开环境变量并去掉重复项', () => {
    expect(
      shortcutIconCandidates(
        { target: '%ProgramFiles%\\App\\app.exe', iconFile: '%ProgramFiles%\\App\\app.exe', iconIndex: 0 },
        { ProgramFiles: 'C:\\Program Files' }
      )
    ).toEqual(['C:\\Program Files\\App\\app.exe'])
  })

  it('过滤网址与相对路径，只留下 Windows 绝对路径', () => {
    expect(
      shortcutIconCandidates(
        { target: 'https://example.com/', iconFile: 'assets\\icon.ico', iconIndex: 0 },
        {}
      )
    ).toEqual([])
    expect(shortcutIconCandidates({ target: '\\\\server\\share\\app.exe', iconIndex: 0 }, {})).toEqual([
      '\\\\server\\share\\app.exe'
    ])
  })
})
