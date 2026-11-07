import { EventEmitter } from 'node:events'
import { createHash } from 'node:crypto'
import { existsSync, readFileSync } from 'node:fs'
import { mkdtemp, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { UpdateState } from '@shared/types'

const state = vi.hoisted(() => ({
  userData: '',
  /** Everything the fake network stack can answer, keyed by URL. */
  routes: new Map<string, { status: number; body: Buffer }>()
}))

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
          const route = state.routes.get(options.url)
          if (!route) {
            emitter.emit('error', new Error(`unexpected request: ${options.url}`))
            return
          }
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
const ASSET_URL = 'https://example.test/Flashcast-0.3.0-arm64-mac.zip'
const SUMS_URL = 'https://example.test/SHA256SUMS.txt'

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
          name: `Flashcast-${version}-arm64-mac.zip`,
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
    emitted = []
  })

  afterEach(async () => {
    await rm(state.userData, { recursive: true, force: true })
  })

  const create = (): InstanceType<typeof UpdateService> =>
    new UpdateService('darwin', (next) => emitted.push(next))

  it('reports an available release with the matching asset', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson()
    })
    const service = create()
    const result = await service.check({ force: true })

    expect(result.phase).toBe('available')
    expect(result.latestVersion).toBe('0.3.0')
    expect(result.assetName).toBe('Flashcast-0.3.0-arm64-mac.zip')
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
      body: Buffer.from(`${ASSET_SHA}  Flashcast-0.3.0-arm64-mac.zip\n`)
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

  it('rejects a download whose checksum does not match', async () => {
    state.routes.set('https://api.github.com/repos/Plasticine-Yang/flashcast/releases/latest', {
      status: 200,
      body: releaseJson()
    })
    state.routes.set(ASSET_URL, { status: 200, body: ASSET_BYTES })
    state.routes.set(SUMS_URL, {
      status: 200,
      body: Buffer.from(`${'0'.repeat(64)}  Flashcast-0.3.0-arm64-mac.zip\n`)
    })

    const service = create()
    await service.check({ force: true })
    const result = await service.download()

    expect(result.phase).toBe('error')
    expect(result.error).toContain('校验')
    expect(result.downloadPath).toBeNull()
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
