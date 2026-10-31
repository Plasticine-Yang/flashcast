import { net } from 'electron'
import { createHash } from 'node:crypto'
import { existsSync } from 'node:fs'
import { readFile, stat, writeFile, mkdir } from 'node:fs/promises'
import { join } from 'node:path'
import { log } from './logger'

const MISS_TTL = 24 * 60 * 60 * 1000

export class FaviconService {
  private memory = new Map<string, string | null>()
  private dir: string

  constructor(root: string) {
    this.dir = root
  }

  setRoot(root: string): void {
    this.dir = root
    this.memory.clear()
  }

  private fileFor(domain: string): string {
    return join(this.dir, `${createHash('sha1').update(domain).digest('hex')}.png`)
  }

  private missFileFor(domain: string): string {
    return join(this.dir, `${createHash('sha1').update(domain).digest('hex')}.miss`)
  }

  async get(domains: string[]): Promise<Record<string, string>> {
    const unique = [...new Set(domains.map(normaliseDomain).filter((domain): domain is string => !!domain))]
    const result: Record<string, string> = {}
    const pending: string[] = []

    for (const domain of unique) {
      const cached = this.memory.get(domain)
      if (cached) {
        result[domain] = cached
        continue
      }
      if (cached === null) continue

      try {
        const buffer = await readFile(this.fileFor(domain))
        const dataUrl = `data:image/png;base64,${buffer.toString('base64')}`
        this.memory.set(domain, dataUrl)
        result[domain] = dataUrl
        continue
      } catch {
        // not cached yet
      }

      try {
        const miss = await stat(this.missFileFor(domain))
        if (Date.now() - miss.mtimeMs < MISS_TTL) {
          this.memory.set(domain, null)
          continue
        }
      } catch {
        // no miss marker
      }
      pending.push(domain)
    }

    await Promise.all(
      pending.slice(0, 60).map(async (domain) => {
        const dataUrl = await this.fetchOne(domain)
        this.memory.set(domain, dataUrl)
        if (dataUrl) result[domain] = dataUrl
      })
    )

    return result
  }

  private async fetchOne(domain: string): Promise<string | null> {
    const controller = new AbortController()
    const timer = setTimeout(() => controller.abort(), 4500)
    try {
      const response = await net.fetch(
        `https://www.google.com/s2/favicons?domain=${encodeURIComponent(domain)}&sz=64`,
        {
          signal: controller.signal,
          headers: { 'User-Agent': 'Mozilla/5.0 Flashcast' }
        }
      )
      if (!response.ok) throw new Error(`HTTP ${response.status}`)
      const contentType = response.headers.get('content-type') ?? ''
      if (!contentType.startsWith('image/')) throw new Error('not an image')
      const buffer = Buffer.from(await response.arrayBuffer())
      if (buffer.byteLength === 0 || buffer.byteLength > 256 * 1024) throw new Error('bad favicon size')
      if (buffer.byteLength < 300) {
        // Google's generic globe fallback is tiny; cache it as a miss so the host
        // can render its own consistent globe icon instead.
        await this.markMiss(domain)
        return null
      }
      await mkdir(this.dir, { recursive: true })
      await writeFile(this.fileFor(domain), buffer)
      return `data:image/png;base64,${buffer.toString('base64')}`
    } catch (error) {
      log.warn(`favicon ${domain}:`, (error as Error).message)
      await this.markMiss(domain)
      return null
    } finally {
      clearTimeout(timer)
    }
  }

  private async markMiss(domain: string): Promise<void> {
    try {
      await mkdir(this.dir, { recursive: true })
      await writeFile(this.missFileFor(domain), String(Date.now()), 'utf8')
    } catch {
      // Cache failures are non-fatal.
    }
  }

  clear(): void {
    this.memory.clear()
  }
}

function normaliseDomain(value: string): string | null {
  const domain = value.trim().toLowerCase()
  if (!domain || domain.length > 253) return null
  if (!/^[a-z0-9.-]+$/.test(domain)) return null
  if (!domain.includes('.')) return null
  return domain
}

export function faviconCacheExists(root: string, domain: string): boolean {
  return existsSync(join(root, `${createHash('sha1').update(domain).digest('hex')}.png`))
}
