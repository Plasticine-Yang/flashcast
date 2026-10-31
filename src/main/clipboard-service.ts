import { clipboard, ClipboardItem, nativeImage } from 'electron'
import { execFile } from 'node:child_process'
import { createHash, randomUUID } from 'node:crypto'
import { EventEmitter } from 'node:events'
import { existsSync } from 'node:fs'
import { copyFile, mkdir, readFile, stat, unlink, writeFile } from 'node:fs/promises'
import { basename, delimiter, extname, join } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import { promisify } from 'node:util'
import type { ClipboardItem as ClipboardEntry, ClipboardKind, PlatformId } from '@shared/types'
import { relativeTime } from '@shared/time'
import type { ConfigStore } from './config-store'
import { CLIPBOARD_DEFAULTS } from './builtins/manifests'
import { readJson, writeJsonAtomic } from './json-store'
import { log } from './logger'
import { assetsDir, type WorkspaceLayout } from './paths'

const execFileAsync = promisify(execFile)

const VIDEO_EXTENSIONS = new Set(['.mp4', '.mov', '.webm', '.mkv', '.avi', '.m4v', '.wmv', '.flv'])
const AUDIO_EXTENSIONS = new Set(['.mp3', '.m4a', '.wav', '.flac', '.ogg', '.oga', '.opus', '.aac', '.wma', '.aiff', '.mid', '.midi'])
const IMAGE_EXTENSIONS = new Set(['.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.svg', '.avif'])
const IMAGE_MIME_EXT: Record<string, string> = {
  'image/png': '.png',
  'image/jpeg': '.jpg',
  'image/webp': '.webp',
  'image/gif': '.gif',
  'image/bmp': '.bmp',
  'image/svg+xml': '.svg',
  'image/avif': '.avif'
}

interface PersistedHistory {
  version: 1
  items: ClipboardEntry[]
}

interface TextCandidate {
  kind: 'text'
  text: string
  fingerprint: string
}

interface ImageCandidate {
  kind: 'image'
  buffer: Buffer
  mime: string
  fingerprint: string
}

interface FileCandidate {
  kind: 'file'
  filePath: string
  category: 'image' | 'video' | 'music'
  fingerprint: string
}

type Candidate = TextCandidate | ImageCandidate | FileCandidate

export class ClipboardService extends EventEmitter {
  private layout: WorkspaceLayout
  private items: ClipboardEntry[] = []
  private maxItems = CLIPBOARD_DEFAULTS.maxItems
  private lastFingerprint: string | null = null
  private pollTimer: NodeJS.Timeout | null = null
  private polling = false
  private selfWriteUntil = 0
  private started = false

  constructor(
    private readonly config: ConfigStore,
    private readonly _platform: PlatformId,
    layout: WorkspaceLayout
  ) {
    super()
    this.layout = layout
  }

  get mediaDir(): string {
    return this.layout.clipboardMediaDir
  }

  get historyFile(): string {
    return this.layout.clipboardHistoryFile
  }

  async init(): Promise<void> {
    await mkdir(this.mediaDir, { recursive: true })
    await this.loadConfig()
    await this.loadHistory()
    this.started = true
    this.startPolling()
  }

  async setWorkspace(layout: WorkspaceLayout): Promise<void> {
    this.stopPolling()
    this.layout = layout
    this.items = []
    this.lastFingerprint = null
    await mkdir(this.mediaDir, { recursive: true })
    await this.loadConfig()
    await this.loadHistory()
    this.started = true
    this.startPolling()
  }

  private async loadConfig(): Promise<void> {
    try {
      const config = await this.config.getPluginConfig('clipboard', CLIPBOARD_DEFAULTS)
      this.maxItems = clampMax(config.maxItems)
    } catch {
      this.maxItems = CLIPBOARD_DEFAULTS.maxItems
    }
  }

  private async loadHistory(): Promise<void> {
    const stored = await readJson<PersistedHistory>(this.historyFile, { version: 1, items: [] })
    this.items = Array.isArray(stored.items)
      ? stored.items.filter(isValidEntry).sort((a, b) => b.createdAt - a.createdAt)
      : []
    await this.trim(this.items.length > this.maxItems ? this.maxItems : this.items.length)
  }

  getMaxItems(): number {
    return this.maxItems
  }

  getItems(): ClipboardEntry[] {
    return this.items.map(publicEntry)
  }

  async setMaxItems(value: number): Promise<ClipboardEntry[]> {
    const next = clampMax(value)
    if (next === this.maxItems) return this.getItems()
    this.maxItems = next
    await this.trim(next)
    await this.persist()
    this.emitChanged()
    return this.getItems()
  }

  async delete(id: string): Promise<ClipboardEntry[]> {
    const index = this.items.findIndex((item) => item.id === id)
    if (index < 0) return this.getItems()
    const [removed] = this.items.splice(index, 1)
    await this.removeMedia(removed)
    await this.persist()
    this.emitChanged()
    return this.getItems()
  }

  async clear(): Promise<ClipboardEntry[]> {
    for (const item of this.items) await this.removeMedia(item)
    this.items = []
    await this.persist()
    this.emitChanged()
    return []
  }

  async activate(id: string, paste: boolean): Promise<{ ok: boolean; error?: string }> {
    const item = this.items.find((entry) => entry.id === id)
    if (!item) return { ok: false, error: '剪切板项不存在' }
    try {
      await this.writeToSystem(item)
      this.selfWriteUntil = Date.now() + 1500
      this.items = [
        { ...item, createdAt: Date.now() },
        ...this.items.filter((entry) => entry.id !== id)
      ]
      await this.persist()
      this.emitChanged()
      return { ok: true }
    } catch (error) {
      log.warn('clipboard activation failed:', error)
      return { ok: false, error: (error as Error).message }
    }
  }

  /** Used by external plugins through the plugin context. */
  async copyText(text: string): Promise<void> {
    await clipboard.writeText(text)
    this.selfWriteUntil = Date.now() + 1200
  }

  async readText(): Promise<string> {
    return clipboard.readText()
  }

  private async writeToSystem(item: ClipboardEntry): Promise<void> {
    if (item.kind === 'text') {
      if (typeof item.text !== 'string') throw new Error('文本内容缺失')
      await clipboard.writeText(item.text)
      return
    }

    if (item.kind === 'image') {
      if (!item.mediaPath) throw new Error('图片文件缺失')
      const file = join(this.mediaDir, item.mediaPath)
      const buffer = await readFile(file)
      const ext = extname(file).toLowerCase()
      const mime = ext === '.jpg' || ext === '.jpeg' ? 'image/jpeg' : ext === '.webp' ? 'image/webp' : ext === '.gif' ? 'image/gif' : 'image/png'
      await clipboard.write([
        new ClipboardItem({
          [mime]: new Blob([new Uint8Array(buffer)], { type: mime })
        })
      ])
      return
    }

    const source = item.sourcePath && existsSync(item.sourcePath)
      ? item.sourcePath
      : item.fileName
        ? join(this.mediaDir, item.fileName)
        : null
    if (!source || !existsSync(source)) throw new Error('原始文件已不存在')
    const uri = pathToFileURL(source).href
    await clipboard.write([
      new ClipboardItem({
        'text/uri-list': new Blob([`${uri}\r\n`], { type: 'text/uri-list' }),
        'text/plain': new Blob([source], { type: 'text/plain' })
      })
    ])
  }

  private startPolling(): void {
    if (this.pollTimer) return
    const tick = async (): Promise<void> => {
      if (!this.started) return
      await this.poll()
      if (!this.started) return
      this.pollTimer = setTimeout(() => void tick(), 700)
      this.pollTimer.unref?.()
    }
    this.pollTimer = setTimeout(() => void tick(), 350)
    this.pollTimer.unref?.()
  }

  private stopPolling(): void {
    if (this.pollTimer) clearTimeout(this.pollTimer)
    this.pollTimer = null
    this.started = false
  }

  stop(): void {
    this.stopPolling()
  }

  private async poll(): Promise<void> {
    if (this.polling || !this.started) return
    this.polling = true
    try {
      const candidate = await this.detectCandidate()
      if (!candidate) return
      if (candidate.fingerprint === this.lastFingerprint) return
      this.lastFingerprint = candidate.fingerprint
      if (Date.now() < this.selfWriteUntil) return
      await this.ingest(candidate)
    } catch (error) {
      log.warn('clipboard poll failed:', (error as Error).message)
    } finally {
      this.polling = false
    }
  }

  private async detectCandidate(): Promise<Candidate | null> {
    const clipboardItems = await clipboard.read().catch(() => [])
    if (!clipboardItems || clipboardItems.length === 0) return null

    for (const item of clipboardItems) {
      const types = item.types ?? []
      const imageType = types.find((type) => type.startsWith('image/'))
      if (imageType) {
        const buffer = await readBlob(item, imageType)
        if (buffer && buffer.byteLength > 0) {
          return {
            kind: 'image',
            buffer,
            mime: imageType,
            fingerprint: `image:${sha256(buffer)}`
          }
        }
      }

      const fileType = types.find(
        (type) => type.includes('uri-list') || type.includes('gnome-copied-files')
      )
      if (fileType) {
        const buffer = await readBlob(item, fileType)
        if (buffer) {
          const uriText = buffer.toString('utf8')
          const filePath = firstSupportedFilePath(uriText)
          if (filePath) {
            const ext = extname(filePath).toLowerCase()
            const category = IMAGE_EXTENSIONS.has(ext)
              ? 'image'
              : VIDEO_EXTENSIONS.has(ext)
                ? 'video'
                : AUDIO_EXTENSIONS.has(ext)
                  ? 'music'
                  : null
            if (category) {
              return {
                kind: 'file',
                filePath,
                category,
                fingerprint: `file:${filePath}`
              }
            }
          }
        }
      }
    }

    for (const item of clipboardItems) {
      if (item.types?.includes('text/plain')) {
        const text = await item.getType('text/plain').then(async (payload) => {
          if (payload instanceof Blob) return payload.text()
          return ''
        }).catch(() => '')
        if (text && text.trim()) {
          const trimmed = text.slice(0, 200_000)
          return {
            kind: 'text',
            text: trimmed,
            fingerprint: `text:${sha256(Buffer.from(trimmed, 'utf8'))}`
          }
        }
      }
    }

    return null
  }

  private async ingest(candidate: Candidate): Promise<void> {
    const existing = this.items.find((item) => item.fingerprint === candidate.fingerprint)
    if (existing) {
      if (this.items[0]?.id === existing.id) {
        // Already first; only refresh the timestamp in memory.
        existing.createdAt = Date.now()
        return
      }
      existing.createdAt = Date.now()
      this.items = [existing, ...this.items.filter((item) => item.id !== existing.id)]
      await this.persist()
      this.emitChanged()
      return
    }

    const item = await this.buildEntry(candidate)
    if (!item) return
    this.items.unshift(item)
    await this.trim(this.maxItems)
    await this.persist()
    this.emitChanged()
  }

  private async buildEntry(candidate: Candidate): Promise<ClipboardEntry | null> {
    if (candidate.kind === 'text') {
      return {
        id: randomUUID(),
        kind: 'text',
        createdAt: Date.now(),
        text: candidate.text,
        title: candidate.text,
        byteSize: Buffer.byteLength(candidate.text, 'utf8'),
        fingerprint: candidate.fingerprint
      }
    }

    if (candidate.kind === 'image') {
      const id = randomUUID()
      const ext = IMAGE_MIME_EXT[candidate.mime] ?? '.png'
      const mediaName = `${id}${ext}`
      await writeFile(join(this.mediaDir, mediaName), candidate.buffer)
      const thumbName = await createThumbnail(join(this.mediaDir, mediaName), id, this.mediaDir, candidate.buffer)
      const image = nativeImage.createFromBuffer(candidate.buffer)
      const size = image.isEmpty() ? { width: 0, height: 0 } : image.getSize()
      return {
        id,
        kind: 'image',
        createdAt: Date.now(),
        title: '图片',
        mediaPath: mediaName,
        thumbPath: thumbName ?? mediaName,
        width: size.width || undefined,
        height: size.height || undefined,
        byteSize: candidate.buffer.byteLength,
        fingerprint: candidate.fingerprint
      }
    }

    const info = await stat(candidate.filePath)
    const ext = extname(candidate.filePath).toLowerCase()
    const id = randomUUID()
    const fileName = basename(candidate.filePath)

    if (candidate.category === 'image') {
      const mediaName = `${id}${ext}`
      await copyFile(candidate.filePath, join(this.mediaDir, mediaName))
      let thumbName: string | null = null
      try {
        const buffer = await readFile(candidate.filePath)
        thumbName = await createThumbnail(join(this.mediaDir, mediaName), id, this.mediaDir, buffer)
      } catch {
        thumbName = null
      }
      const image = nativeImage.createFromPath(candidate.filePath)
      const size = image.isEmpty() ? { width: 0, height: 0 } : image.getSize()
      return {
        id,
        kind: 'image',
        createdAt: Date.now(),
        title: '图片',
        mediaPath: mediaName,
        thumbPath: thumbName ?? mediaName,
        fileName,
        width: size.width || undefined,
        height: size.height || undefined,
        byteSize: info.size,
        sourcePath: candidate.filePath,
        fingerprint: candidate.fingerprint
      }
    }

    if (candidate.category === 'video') {
      const coverName = `${id}-cover.png`
      const extracted = await extractVideoFrame(candidate.filePath, join(this.mediaDir, coverName))
      if (extracted) {
        return {
          id,
          kind: 'video',
          createdAt: Date.now(),
          title: '视频',
          mediaPath: coverName,
          thumbPath: coverName,
          fileName,
          byteSize: info.size,
          sourcePath: candidate.filePath,
          fingerprint: candidate.fingerprint
        }
      }
      try {
        await copyFile(join(assetsDir(), 'video-cover.png'), join(this.mediaDir, coverName))
      } catch {
        // Fall back to a generated SVG if the packaged cover asset is missing.
        const svgName = `${id}.svg`
        await writeFile(join(this.mediaDir, svgName), videoCoverSvg(fileName), 'utf8')
        return {
          id,
          kind: 'video',
          createdAt: Date.now(),
          title: '视频',
          mediaPath: svgName,
          thumbPath: svgName,
          fileName,
          byteSize: info.size,
          sourcePath: candidate.filePath,
          fingerprint: candidate.fingerprint
        }
      }
      return {
        id,
        kind: 'video',
        createdAt: Date.now(),
        title: '视频',
        mediaPath: coverName,
        thumbPath: coverName,
        fileName,
        byteSize: info.size,
        sourcePath: candidate.filePath,
        fingerprint: candidate.fingerprint
      }
    }

    return {
      id,
      kind: 'music',
      createdAt: Date.now(),
      title: '音乐',
      fileName,
      byteSize: info.size,
      sourcePath: candidate.filePath,
      fingerprint: candidate.fingerprint
    }
  }

  private async trim(limit: number): Promise<void> {
    const target = clampMax(limit)
    if (this.items.length <= target) return
    const removed = this.items.splice(target)
    for (const item of removed) await this.removeMedia(item)
  }

  private async removeMedia(item: ClipboardEntry): Promise<void> {
    const paths = new Set([item.mediaPath, item.thumbPath].filter(Boolean) as string[])
    for (const relative of paths) {
      try {
        await unlink(join(this.mediaDir, relative))
      } catch {
        // ignore missing files
      }
    }
  }

  private async persist(): Promise<void> {
    await mkdir(this.mediaDir, { recursive: true })
    await writeJsonAtomic(this.historyFile, {
      version: 1,
      items: this.items
    } satisfies PersistedHistory)
  }

  private emitChanged(): void {
    this.emit('changed', this.getItems())
  }
}

async function readBlob(item: ClipboardItem, type: string): Promise<Buffer | null> {
  try {
    const payload = await item.getType(type)
    if (payload instanceof Blob) {
      return Buffer.from(await payload.arrayBuffer())
    }
    return null
  } catch {
    return null
  }
}

function sha256(buffer: Buffer): string {
  return createHash('sha256').update(buffer).digest('hex')
}

function clampMax(value: unknown): number {
  const number = typeof value === 'number' && Number.isFinite(value) ? Math.floor(value) : CLIPBOARD_DEFAULTS.maxItems
  return Math.min(100, Math.max(1, number))
}

function firstSupportedFilePath(raw: string): string | null {
  const lines = raw.split(/\r?\n/)
  for (const line of lines) {
    const trimmed = line.trim()
    if (!trimmed || trimmed.toLowerCase() === 'copy' || trimmed.toLowerCase() === 'cut') continue
    if (trimmed.startsWith('file://')) {
      try {
        const path = fileURLToPath(trimmed)
        if (existsSync(path)) return path
      } catch {
        // malformed URI
      }
    }
  }
  return null
}

function isValidEntry(value: unknown): value is ClipboardEntry {
  if (!value || typeof value !== 'object') return false
  const entry = value as ClipboardEntry
  return (
    typeof entry.id === 'string' &&
    ['text', 'image', 'video', 'music'].includes(entry.kind) &&
    typeof entry.createdAt === 'number' &&
    typeof entry.title === 'string'
  )
}

function publicEntry(entry: ClipboardEntry): ClipboardEntry {
  const { sourcePath: _sourcePath, fingerprint: _fingerprint, ...rest } = entry
  return rest
}

async function createThumbnail(
  sourcePath: string,
  id: string,
  mediaDir: string,
  buffer?: Buffer
): Promise<string | null> {
  try {
    const image = buffer ? nativeImage.createFromBuffer(buffer) : nativeImage.createFromPath(sourcePath)
    if (image.isEmpty()) return null
    const size = image.getSize()
    const width = Math.min(320, Math.max(1, size.width))
    const resized = image.resize({ width, quality: 'good' })
    const name = `${id}-thumb.png`
    await writeFile(join(mediaDir, name), resized.toPNG())
    return name
  } catch {
    return null
  }
}

function commandExists(command: string): boolean {
  return (process.env.PATH ?? '')
    .split(delimiter)
    .filter(Boolean)
    .some((dir) => existsSync(join(dir, command)))
}

/**
 * Best-effort video cover extraction. Desktop environments that ship ffmpeg get
 * a real frame; everywhere else we fall back to a designed cover card.
 */
async function extractVideoFrame(input: string, output: string): Promise<boolean> {
  if (!commandExists('ffmpeg')) return false
  try {
    await execFileAsync(
      'ffmpeg',
      ['-y', '-loglevel', 'error', '-ss', '0.1', '-i', input, '-frames:v', '1', '-vf', 'scale=1280:-2', output],
      { timeout: 8000, maxBuffer: 4 * 1024 * 1024 }
    )
    const info = await stat(output)
    return info.size > 0
  } catch {
    try {
      await unlink(output)
    } catch {
      // ignore
    }
    return false
  }
}

function escapeXml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

function videoCoverSvg(fileName: string): string {
  const label = escapeXml(fileName.length > 42 ? `${fileName.slice(0, 41)}…` : fileName)
  return `<svg xmlns="http://www.w3.org/2000/svg" width="1280" height="720" viewBox="0 0 1280 720">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#202026"/>
      <stop offset="0.55" stop-color="#17171b"/>
      <stop offset="1" stop-color="#251d10"/>
    </linearGradient>
    <radialGradient id="glow" cx="0.5" cy="0.42" r="0.55">
      <stop offset="0" stop-color="#ffb224" stop-opacity="0.18"/>
      <stop offset="1" stop-color="#ffb224" stop-opacity="0"/>
    </radialGradient>
  </defs>
  <rect width="1280" height="720" fill="url(#bg)"/>
  <rect width="1280" height="720" fill="url(#glow)"/>
  <circle cx="640" cy="332" r="58" fill="rgba(255,178,36,0.14)" stroke="rgba(255,178,36,0.45)" stroke-width="1.5"/>
  <path d="M628 306 L666 332 L628 358 Z" fill="#FFB224"/>
  <text x="640" y="438" text-anchor="middle" fill="#F5F5F7" font-family="Inter, system-ui, sans-serif" font-size="28" font-weight="500">${label}</text>
  <text x="640" y="474" text-anchor="middle" fill="#8E8E93" font-family="Inter, system-ui, sans-serif" font-size="18" letter-spacing="2">视频封面</text>
</svg>`
}

/** Exposes the relative-time helper for the clipboard preview copy. */
export function clipboardTimeLabel(createdAt: number): string {
  return relativeTime(createdAt)
}
