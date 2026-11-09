import { EventEmitter } from 'node:events'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { UpdateState } from '@shared/types'

const state = vi.hoisted(() => {
  type Route = { status: number; body: Buffer }
  return {
    userData: '',
    /** Everything the fake network stack can answer, keyed by URL. */
    routes: new Map<string, Route | Route[]>(),
    /** URLs in request order, so retries are observable. */
    requests: [] as string[]
  }
})

function fakeResponse(status: number, body: Buffer): EventEmitter & {
  statusCode: number
  headers: Record<string, string>
} {
  const response = new EventEmitter() as EventEmitter & {
    statusCode: number
    headers: Record<string, string>
  }
  response.statusCode = status
  response.headers = { 'content-length': String(body.length) }
  return response
}

vi.mock('electron', () => ({
  app: {
    getPath: () => state.userData,
    isPackaged: true,
    getVersion: () => '0.2.3',
    quit: vi.fn()
  },
  shell: { openPath: vi.fn(async () => ''), showItemInFolder: vi.fn(), openExternal: vi.fn() },
  net: {
    request: (options: { url: string }) => {
      const emitter = new EventEmitter() as EventEmitter & {
        setHeader: (name: string, value: string) => void
        end: () => void
        abort: () => void
      }
      emitter.setHeader = () => undefined
      emitter.abort = () => emitter.emit('abort')
      emitter.end = () => {
        setImmediate(() => {
          const entry = state.routes.get(options.url)
          // An array is a scripted sequence: consume it, but keep the last
          // response for any further attempts.
          const route = Array.isArray(entry)
            ? entry.length > 1
              ? entry.shift()
              : entry[0]
            : entry
          if (!route) {
            emitter.emit('error', new Error(`unexpected request: ${options.url}`))
            return
          }
          state.requests.push(options.url)
          const response = fakeResponse(route.status, route.body)
          emitter.emit('response', response)
          setImmediate(() => {
            // Two chunks so progress reporting is exercised.
            const half = Math.ceil(route.body.length / 2)
            response.emit('data', route.body.subarray(0, half))
            response.emit('data', route.body.subarray(half))
            response.emit('end')
          })
        })
      }
      return emitter
    }
  }
}))

vi.mock('./paths', () => ({
  appVersion: () => '0.2.3',
  isDev: () => false
}))

vi.mock('./logger', () => ({
  log: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}))

const { UpdateService } = await import('./update-service')

const ASSET_BYTES = Buffer.from('flashcast-update-payload')
const ASSET_SHA = createHash('sha256').update(ASSET_BYTES).digest('hex')
// The updater selects a macOS asset for the host architecture. Keep this test
// runnable on both Apple Silicon and the x64 GitHub Actions runner.
const ASSET_NAME =
  process.arch === 'arm64' ? 'Flashcast-0.3.0-arm64-mac.zip' : 'Flashcast-0.3.0-mac.zip'
const ASSET_URL = `https://example.test/${ASSET_NAME}`
const SUMS_URL = 'https://example.test/SHA256SUMS.txt'
const API_URL = 'https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest'

async function waitFor(predicate: () => boolean): Promise<void> {
  for (let attempt = 0; attempt < 200; attempt += 1) {
    if (predicate()) return
    await new Promise((resolve) => setTimeout(resolve, 5))
  }
  throw new Error('condition was not met in time')
}

function releaseJson(version = '0.3.0'): Buffer {
  return Buffer.from(
    JSON.stringify({
      tag_name: `v${version}`,
      name: `Flashcast ${version}`,
      body: '## 更新内容\n- 顶栏新增设置入口\n- 支持检查更新',
      html_url: `https://github.com/Plasticine-Yang/flashcast/releases/tag/v${version}`,
      published_at: '2026-11-01T18:08:47Z',
      assets: [
        {
          name: ASSET_NAME,
          size: ASSET_BYTES.length,
          browser_download_url: ASSET_URL
        },
        { name: 'SHA256SUMS.txt', size: 120, browser_download_url: SUMS_URL }
      ]
    })
  )
}

describe('UpdateService', () => {
  let emitted: UpdateState[]

  beforeEach(async () => {
    state.userData = await mkdtemp(join(tmpdir(), 'flashcast-update-'))
    state.routes.clear()
    state.requests = []
    emitted = []
  })

  afterEach(async () => {
    await rm(state.userData, { recursive: true, force: true })
  })

  const create = (
    options?: { downloadRetryDelayMs?: number }
  ): InstanceType<typeof UpdateService> =>
    new UpdateService('darwin', (next) => emitted.push(next), options)

  it('reports an available release with the matching asset', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson()
    })
    const service = create()
    const result = await service.check({ force: true })

    expect(result.phase).toBe('available')
    expect(result.latestVersion).toBe('0.3.0')
    expect(result.assetName).toBe(ASSET_NAME)
    expect(result.assetSize).toBe(ASSET_BYTES.length)
    expect(result.installKind).toBe('relaunch')
    expect(result.canInstall).toBe(true)
    expect(result.releaseNotes).toContain('顶栏新增设置入口')
    expect(emitted.some((entry) => entry.phase === 'checking')).toBe(true)
  })

  it('stays on latest when the running version is not older', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson('0.2.3')
    })
    const service = create()
    const result = await service.check({ force: true })
    expect(result.phase).toBe('latest')
    expect(result.message).toContain('0.2.3')
  })

  it('downloads, verifies and marks the update ready', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson()
    })
    state.routes.set(ASSET_URL, { status: 200, body: ASSET_BYTES })
    state.routes.set(SUMS_URL, {
      status: 200,
      body: Buffer.from(`${ASSET_SHA}  ${ASSET_NAME}\n`)
    })

    const service = create()
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('ready')
    expect(result.progress).toBe(1)
    expect(result.downloadPath && existsSync(result.downloadPath)).toBe(true)
    expect(readFileSync(result.downloadPath!, 'utf8')).toBe(ASSET_BYTES.toString('utf8'))
    expect(emitted.some((entry) => entry.phase === 'downloading')).toBe(true)
  })

  it('retries a transient 504 and then finishes the download', async () => {
    state.routes.set(API_URL, { status: 200, body: releaseJson() })
    state.routes.set(ASSET_URL, [
      { status: 504, body: Buffer.from('gateway timeout') },
      { status: 200, body: ASSET_BYTES }
    ])
    state.routes.set(SUMS_URL, {
      status: 200,
      body: Buffer.from(`${ASSET_SHA}  ${ASSET_NAME}\n`)
    })

    const service = create({ downloadRetryDelayMs: 1 })
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('ready')
    expect(state.requests.filter((url) => url === ASSET_URL)).toHaveLength(2)
    expect(emitted.some((entry) => entry.message.includes('正在重试'))).toBe(true)
    expect(readFileSync(result.downloadPath!, 'utf8')).toBe(ASSET_BYTES.toString('utf8'))
  })

  it('does not retry a permanent 404', async () => {
    state.routes.set(API_URL, { status: 200, body: releaseJson() })
    state.routes.set(ASSET_URL, { status: 404, body: Buffer.from('not found') })

    const service = create({ downloadRetryDelayMs: 1 })
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('error')
    expect(result.error).toContain('HTTP 404')
    expect(state.requests.filter((url) => url === ASSET_URL)).toHaveLength(1)
  })

  it('stops after three failed attempts', async () => {
    state.routes.set(API_URL, { status: 200, body: releaseJson() })
    state.routes.set(ASSET_URL, { status: 504, body: Buffer.from('gateway timeout') })

    const service = create({ downloadRetryDelayMs: 1 })
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('error')
    expect(result.error).toContain('HTTP 504')
    expect(state.requests.filter((url) => url === ASSET_URL)).toHaveLength(3)
  })

  it('cancels while waiting to retry', async () => {
    state.routes.set(API_URL, { status: 200, body: releaseJson() })
    state.routes.set(ASSET_URL, { status: 504, body: Buffer.from('gateway timeout') })

    const service = create({ downloadRetryDelayMs: 200 })
    await service.check({ force: true })
    const pending = service.download()
    await waitFor(() => emitted.some((entry) => entry.message.includes('正在重试')))

    await service.cancel()
    const result = await pending

    expect(result.phase).toBe('available')
    expect(result.message).toBe('已取消下载')
    expect(state.requests.filter((url) => url === ASSET_URL)).toHaveLength(1)
  })

  it('rejects a download whose checksum does not match', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson()
    })
    state.routes.set(ASSET_URL, { status: 200, body: ASSET_BYTES })
    state.routes.set(SUMS_URL, {
      status: 200,
      body: Buffer.from(`${'0'.repeat(64)}  ${ASSET_NAME}\n`)
    })

    const service = create()
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('error')
    expect(result.error).toContain('校验')
    expect(result.downloadPath).toBeNull()
  })

  it('falls back to the release pages when the API is rate limited', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 403,
      body: Buffer.from('{"message":"API rate limit exceeded"}')
    })
    state.routes.set('https://github.com/Plasticine-Yang/flashcast/releases.atom', {
      status: 200,
      body: Buffer.from(`<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom">
  <entry>
    <id>tag:github.com,2008:Repository/123456/v0.3.0</id>
    <updated>2026-11-01T18:08:47Z</updated>
    <link rel="alternate" type="text/html" href="https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.3.0"/>
    <title>Flashcast 0.3.0</title>
    <content type="html">&lt;p&gt;顶栏新增设置入口&lt;/p&gt;</content>
  </entry>
</feed>`)
    })
    state.routes.set(
      'https://github.com/Plasticine-Yang/flashcast/releases/expanded_assets/v0.3.0',
      {
        status: 200,
        body: Buffer.from(
          [
            `<a href="/Plasticine-Yang/flashcast/releases/download/v0.3.0/${ASSET_NAME}" rel="nofollow">`,
            `  <span class="text-bold">${ASSET_NAME}</span>`,
            '</a>',
            '<a href="/Plasticine-Yang/flashcast/releases/download/v0.3.0/Flashcast.Setup.0.3.0.exe" rel="nofollow">',
            '  <span class="text-bold">Flashcast.Setup.0.3.0.exe</span>',
            '</a>',
            '<a href="/Plasticine-Yang/flashcast/releases/download/v0.3.0/SHA256SUMS.txt" rel="nofollow">',
            '  <span class="text-bold">SHA256SUMS.txt</span>',
            '</a>'
          ].join('\n')
        )
      }
    )

    const service = create()
    const result = await service.check({ force: true })

    expect(result.phase).toBe('available')
    expect(result.latestVersion).toBe('0.3.0')
    expect(result.assetName).toBe(ASSET_NAME)
    expect(result.assetSize).toBe(0)
    expect(result.releaseNotes).toContain('顶栏新增设置入口')
    expect(result.releaseUrl).toBe(
      'https://github.com/Plasticine-Yang/flashcast/releases/tag/v0.3.0'
    )
  })

  it('surfaces feed failures instead of throwing', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 500,
      body: Buffer.from('boom')
    })
    const service = create()
    const result = await service.check({ force: true })
    expect(result.phase).toBe('error')
    expect(result.error).toContain('HTTP 500')
  })

  it('blocks installing before anything is downloaded', async () => {
    const service = create()
    const result = await service.install()
    expect(result.ok).toBe(false)
  })
})
