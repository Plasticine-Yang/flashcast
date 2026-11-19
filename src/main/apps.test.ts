import { mkdtemp, rm, utimes, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppRecord } from './apps'

vi.mock('electron', () => ({
  app: { getPath: () => tmpdir(), isPackaged: false, getAppPath: () => tmpdir() },
  nativeImage: { createFromBuffer: vi.fn(), createThumbnailFromPath: vi.fn() },
  shell: { openPath: vi.fn() }
}))

const { AppDiscovery } = await import('./apps')

let root: string
let files: string[]

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flashcast-apps-'))
  files = []
})

afterEach(async () => {
  await rm(root, { recursive: true, force: true })
})

function appRecord(name: string): AppRecord {
  return {
    id: name.toLowerCase(),
    name,
    subtitle: '应用',
    keywords: [],
    iconKey: 'app',
    launch: { type: 'bundle', path: join(root, `${name}.app`) }
  }
}

function makeDiscovery(counter: { scans: number }): InstanceType<typeof AppDiscovery> {
  return new AppDiscovery('darwin', {
    discover: async () => {
      counter.scans += 1
      return files.map(appRecord)
    },
    // 用临时目录里的文件模拟应用目录，测试不读本机安装的应用。
    touchPoints: async () => files.map((file) => join(root, file))
  })
}

async function install(file: string): Promise<void> {
  files.push(file)
  await writeFile(join(root, file), '', 'utf8')
}

describe('AppDiscovery.sync', () => {
  it('skips the rescan while the installed app set is unchanged', async () => {
    await install('One')
    const counter = { scans: 0 }
    const discovery = makeDiscovery(counter)

    expect((await discovery.list()).map((entry) => entry.name)).toEqual(['One'])
    expect(await discovery.sync()).toBeNull()
    expect(counter.scans).toBe(1)
  })

  it('rescans when an app is installed after the initial scan', async () => {
    await install('One')
    const counter = { scans: 0 }
    const discovery = makeDiscovery(counter)
    await discovery.list()

    await install('Two')

    const entries = await discovery.sync()
    expect(entries?.map((entry) => entry.name)).toEqual(['One', 'Two'])
    expect(counter.scans).toBe(2)
  })

  it('rescans when an installed bundle changes in place', async () => {
    await install('One')
    const counter = { scans: 0 }
    const discovery = makeDiscovery(counter)
    await discovery.list()

    const later = new Date(Date.now() + 5_000)
    await utimes(join(root, 'One'), later, later)

    const entries = await discovery.sync()
    expect(entries?.map((entry) => entry.name)).toEqual(['One'])
    expect(counter.scans).toBe(2)
  })

  it('refreshes even when the fingerprint still matches', async () => {
    await install('One')
    const counter = { scans: 0 }
    const discovery = makeDiscovery(counter)
    await discovery.list()

    expect((await discovery.refresh()).map((entry) => entry.name)).toEqual(['One'])
    expect(counter.scans).toBe(2)
  })
})
