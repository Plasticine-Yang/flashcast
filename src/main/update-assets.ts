import type { PlatformId, UpdateInstallKind } from '@shared/types'

/** Subset of a GitHub release asset that the updater needs. */
export interface ReleaseAsset {
  name: string
  size: number
  url: string
}

const VERSION = /^v?(\d+)(?:\.(\d+))?(?:\.(\d+))?(?:-([0-9A-Za-z.-]+))?$/

export interface ParsedVersion {
  numbers: [number, number, number]
  /** Pre-release label (`beta.1`); a release without one sorts higher. */
  prerelease: string | null
}

export function parseVersion(input: string | null | undefined): ParsedVersion | null {
  if (typeof input !== 'string') return null
  const match = VERSION.exec(input.trim())
  if (!match) return null
  return {
    numbers: [Number(match[1] ?? 0), Number(match[2] ?? 0), Number(match[3] ?? 0)],
    prerelease: match[4] ?? null
  }
}

/** Compare two dot-separated pre-release labels the semver way. */
function comparePrerelease(a: string | null, b: string | null): number {
  if (a === b) return 0
  if (a === null) return 1
  if (b === null) return -1
  const left = a.split('.')
  const right = b.split('.')
  for (let index = 0; index < Math.max(left.length, right.length); index += 1) {
    const one = left[index]
    const two = right[index]
    if (one === undefined) return -1
    if (two === undefined) return 1
    const numeric = /^\d+$/.test(one) && /^\d+$/.test(two)
    if (numeric) {
      const diff = Number(one) - Number(two)
      if (diff !== 0) return diff < 0 ? -1 : 1
      continue
    }
    if (one === two) continue
    return one < two ? -1 : 1
  }
  return 0
}

/** True when `candidate` is a strictly newer release than `current`. */
export function isNewerVersion(candidate: string, current: string): boolean {
  const next = parseVersion(candidate)
  const base = parseVersion(current)
  if (!next || !base) return false
  for (let index = 0; index < 3; index += 1) {
    if (next.numbers[index] !== base.numbers[index]) {
      return next.numbers[index] > base.numbers[index]
    }
  }
  return comparePrerelease(next.prerelease, base.prerelease) > 0
}

/**
 * electron-builder names macOS artifacts `<Product>-<version>[-<arch>]-mac.zip`
 * (the x64 build has no arch suffix), the Windows installer
 * `<Product>.Setup.<version>.exe` and Linux `<Product>-<version>.AppImage` /
 * `<name>_<version>_<arch>.deb`.
 */
export function pickUpdateAsset(
  assets: ReleaseAsset[],
  platform: PlatformId,
  arch: string
): ReleaseAsset | null {
  const bySuffix = (test: (name: string) => boolean): ReleaseAsset | null =>
    assets.find((asset) => test(asset.name)) ?? null

  if (platform === 'darwin') {
    const arm = arch === 'arm64'
    const zip = bySuffix((name) =>
      arm ? /-arm64-mac\.zip$/i.test(name) : /(?<!arm64)-mac\.zip$/i.test(name)
    )
    if (zip) return zip
    return bySuffix((name) =>
      arm ? /-arm64\.dmg$/i.test(name) : /(?<!arm64)\.dmg$/i.test(name)
    )
  }

  if (platform === 'win32') {
    return bySuffix((name) => /setup.*\.exe$/i.test(name)) ?? bySuffix((name) => /\.exe$/i.test(name))
  }

  return (
    bySuffix((name) => /\.appimage$/i.test(name)) ??
    bySuffix((name) => /\.deb$/i.test(name)) ??
    bySuffix((name) => /\.rpm$/i.test(name))
  )
}

/** Whether the artifact can replace the running app, or needs the OS installer UI. */
export function installKindFor(assetName: string, platform: PlatformId): UpdateInstallKind {
  const lower = assetName.toLowerCase()
  if (platform === 'darwin') return lower.endsWith('.zip') ? 'relaunch' : 'manual'
  if (platform === 'win32') return lower.endsWith('.exe') ? 'relaunch' : 'manual'
  if (lower.endsWith('.appimage')) return 'relaunch'
  return 'manual'
}

/** Parse `SHA256SUMS.txt` (`<hex>  <filename>` per line). */
export function parseChecksums(text: string): Record<string, string> {
  const sums: Record<string, string> = {}
  for (const line of text.split(/\r?\n/)) {
    const match = /^([0-9a-f]{64})\s+\*?(.+?)\s*$/i.exec(line.trim())
    if (!match) continue
    sums[match[2]] = match[1].toLowerCase()
  }
  return sums
}

/**
 * GitHub rewrites spaces in uploaded asset names to dots, so a checksum line
 * (`Flashcast Setup 0.2.3.exe`, produced by `sha256sum *` on the build output)
 * and the release asset (`Flashcast.Setup.0.2.3.exe`) only differ by separators.
 */
export function normalizeArtifactName(name: string): string {
  return name
    .trim()
    .toLowerCase()
    .replace(/[\s._-]+/g, '-')
}

/** Look up a checksum by exact name, then by separator-insensitive name. */
export function checksumFor(text: string, name: string): string | null {
  const sums = parseChecksums(text)
  if (sums[name]) return sums[name]
  const wanted = normalizeArtifactName(name)
  for (const [key, value] of Object.entries(sums)) {
    if (normalizeArtifactName(key) === wanted) return value
  }
  return null
}

/** Release notes are Markdown; the renderer only ever displays them as text. */
export function normalizeReleaseNotes(body: string | null | undefined, limit = 1200): string | null {
  if (typeof body !== 'string') return null
  const text = body.replace(/\r\n/g, '\n').trim()
  if (!text) return null
  return text.length > limit ? `${text.slice(0, limit).trimEnd()}…` : text
}

export interface ReleaseFeedEntry {
  /** Release tag, including the `v` prefix when GitHub has one. */
  tag: string
  title: string
  notes: string | null
  publishedAt: number | null
}

const HTML_ENTITIES: Record<string, string> = {
  amp: '&',
  lt: '<',
  gt: '>',
  quot: '"',
  apos: "'",
  nbsp: ' '
}

function decodeEntities(input: string): string {
  return input.replace(/&(#x?[0-9a-f]+|[a-z]+);/gi, (match, entity: string) => {
    if (entity.startsWith('#x') || entity.startsWith('#X')) {
      const code = Number.parseInt(entity.slice(2), 16)
      return Number.isFinite(code) ? String.fromCodePoint(code) : match
    }
    if (entity.startsWith('#')) {
      const code = Number.parseInt(entity.slice(1), 10)
      return Number.isFinite(code) ? String.fromCodePoint(code) : match
    }
    return HTML_ENTITIES[entity.toLowerCase()] ?? match
  })
}

/** Release-note HTML from the Atom feed reduced to the plain text we display. */
export function htmlToPlainText(input: string): string {
  // Atom escapes the note HTML once; decode before stripping the real tags,
  // then decode again so `&amp;amp;`-style leftovers do not leak through.
  return decodeEntities(
    decodeEntities(input)
      .replace(/<\/(p|li|h[1-6]|div|tr)>/gi, '\n')
      .replace(/<br\s*\/?>/gi, '\n')
      .replace(/<[^>]+>/g, '')
  )
    .replace(/[ \t]+/g, ' ')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

/**
 * Reads the newest release from `https://github.com/<owner>/<repo>/releases.atom`.
 * The REST API is rate limited per IP (60/h for anonymous callers), which a
 * shared office egress IP exhausts easily; the Atom feed has no such quota.
 */
export function parseLatestReleaseEntry(feed: string): ReleaseFeedEntry | null {
  const entry = /<entry\b[^>]*>([\s\S]*?)<\/entry>/i.exec(feed)?.[1]
  if (!entry) return null
  const link = /<link\b[^>]*rel="alternate"[^>]*href="([^"]+)"[^>]*>/i.exec(entry)?.[1]
  const id = /<id>\s*tag:github\.com[^/]*\/[^/]*\/([^<\s]+)\s*<\/id>/i.exec(entry)?.[1]
  const tag = (link ? decodeURIComponent(link.split('/').pop() ?? '') : id) ?? ''
  if (!tag) return null
  const title = /<title\b[^>]*>([\s\S]*?)<\/title>/i.exec(entry)?.[1]
  const updated = /<updated>\s*([^<]+?)\s*<\/updated>/i.exec(entry)?.[1]
  const content = /<content\b[^>]*>([\s\S]*?)<\/content>/i.exec(entry)?.[1]
  const published = updated ? Date.parse(updated) : Number.NaN
  return {
    tag,
    title: title ? decodeEntities(title).trim() : tag,
    notes: content ? normalizeReleaseNotes(htmlToPlainText(content)) : null,
    publishedAt: Number.isFinite(published) ? published : null
  }
}

/**
 * Asset list from `https://github.com/<owner>/<repo>/releases/expanded_assets/<tag>`.
 * The REST API is the only place that returns sizes, but its names are the
 * canonical ones — unlike `SHA256SUMS.txt`, which keeps the `sha256sum` output
 * (`Flashcast Setup 0.4.1.exe`) instead of the asset name
 * (`Flashcast.Setup.0.4.1.exe`).
 */
export function parseReleaseAssets(html: string, slug: string, tag: string): ReleaseAsset[] {
  const escape = (value: string): string => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
  const pattern = new RegExp(
    `href="/${escape(slug)}/releases/download/${escape(tag)}/([^"?#]+)"`,
    'g'
  )
  const seen = new Set<string>()
  const assets: ReleaseAsset[] = []
  for (const match of html.matchAll(pattern)) {
    const name = decodeEntities(match[1])
    if (seen.has(name)) continue
    seen.add(name)
    assets.push({
      name,
      size: 0,
      url: `https://github.com/${slug}/releases/download/${tag}/${encodeURIComponent(name)}`
    })
  }
  return assets
}
