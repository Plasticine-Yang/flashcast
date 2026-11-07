import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('electron', () => ({
  app: { getPath: () => tmpdir(), isPackaged: false, getAppPath: () => tmpdir() },
  clipboard: { writeText: vi.fn() },
  shell: { showItemInFolder: vi.fn() }
}))

const { ConfigStore } = await import('../config-store')
const { workspaceLayout } = await import('../paths')
const { VscodeLikeWorkspaceService } = await import('./service')

let root: string

async function write(path: string, content: string): Promise<void> {
  await mkdir(join(path, '..'), { recursive: true })
  await writeFile(path, content, 'utf8')
}

async function makeEditor(
  name: string,
  applicationName: string,
  version: string
): Promise<{ appRoot: string; dataDir: string }> {
  const appRoot = join(root, 'Applications', `${name}.app`, 'Contents', 'Resources', 'app')
  await write(
    join(appRoot, 'product.json'),
    JSON.stringify({
      applicationName,
      nameShort: name,
      dataFolderName: `.${applicationName}`,
      urlProtocol: applicationName,
      version
    })
  )
  await write(join(appRoot, 'package.json'), JSON.stringify({ name, version }))
  await write(join(appRoot, 'out', 'main.js'), '')
  const dataDir = join(root, 'Library', 'Application Support', name)
  await mkdir(join(dataDir, 'User'), { recursive: true })
  return { appRoot, dataDir }
}

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flashcast-service-'))
})

afterEach(async () => {
  await rm(root, { recursive: true, force: true })
})

describe('VscodeLikeWorkspaceService', () => {
  it('lists detected editors, their workspaces and configured triggers', async () => {
    const code = await makeEditor('Code', 'code', '1.100.0')
    const trae = await makeEditor('Trae CN', 'trae-cn', '1.107.1')
    await mkdir(join(root, 'projects', 'alpha'), { recursive: true })

    for (const editor of [code, trae]) {
      await write(
        join(editor.dataDir, 'User', 'globalStorage', 'storage.json'),
        JSON.stringify({
          windowsState: { openedWindows: [{ folder: `file://${join(root, 'projects', 'alpha')}` }] }
        })
      )
    }

    const store = new ConfigStore(workspaceLayout(join(root, 'workspace')), 'darwin')
    await store.init()
    const service = new VscodeLikeWorkspaceService(store, 'darwin', {
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Applications')]
    })
    await service.init()

    const page = await service.getPage()
    expect(page.variants.map((variant) => variant.id).sort()).toEqual(['code', 'trae-cn'])
    expect(page.workspaces).toHaveLength(2)
    expect(page.workspaces.every((workspace) => workspace.isOpen)).toBe(true)
    expect(service.getTriggersSync()).toEqual([])

    const settings = await service.getSettings(false)
    expect(settings.kind).toBe('vscode-like-workspace-settings')
    expect(settings.variants.map((variant) => variant.trigger)).toEqual(['', ''])
  })

  it('registers triggers only after a valid save and rejects conflicts', async () => {
    await makeEditor('Code', 'code', '1.100.0')
    await makeEditor('Trae CN', 'trae-cn', '1.107.1')

    const store = new ConfigStore(workspaceLayout(join(root, 'workspace')), 'darwin')
    await store.init()
    const service = new VscodeLikeWorkspaceService(store, 'darwin', {
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Applications')]
    })
    await service.init()

    const rejected = await service.saveSettings(
      { variants: { code: { trigger: 'trae' }, 'trae-cn': { trigger: 'trae' } } },
      new Set()
    )
    expect(rejected.savedAt).toBe(0)
    expect(rejected.page.triggerErrors['trae-cn']).toContain('重复')
    expect(service.getTriggersSync()).toEqual([])

    const saved = await service.saveSettings(
      { variants: { code: { trigger: 'VSC' }, 'trae-cn': { enabled: true, trigger: 'trae' } } },
      new Set()
    )
    expect(saved.savedAt).toBeGreaterThan(0)
    expect(saved.page.triggerErrors).toEqual({})
    expect(service.getTriggersSync()).toEqual([
      { keyword: 'vsc', label: 'Code 工作区', args: { editor: 'code' } },
      { keyword: 'trae', label: 'Trae CN 工作区', args: { editor: 'trae-cn' } }
    ])
  })

  it('skips triggers of disabled editors and rejects ones owned by other plugins', async () => {
    await makeEditor('Code', 'code', '1.100.0')
    await makeEditor('Trae CN', 'trae-cn', '1.107.1')

    const store = new ConfigStore(workspaceLayout(join(root, 'workspace')), 'darwin')
    await store.init()
    const service = new VscodeLikeWorkspaceService(store, 'darwin', {
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Applications')]
    })
    await service.init()

    const foreign = await service.saveSettings(
      { variants: { code: { trigger: 'bookmarks' } } },
      new Set(['bookmarks'])
    )
    expect(foreign.page.triggerErrors.code).toContain('其他插件')
    expect(service.getTriggersSync()).toEqual([])

    await service.saveSettings(
      { variants: { code: { trigger: 'vsc' }, 'trae-cn': { enabled: true, trigger: 'trae' } } },
      new Set()
    )
    const disabled = await service.saveSettings(
      { variants: { code: { enabled: false, trigger: 'vsc' }, 'trae-cn': { enabled: true, trigger: 'trae' } } },
      new Set()
    )
    expect(disabled.savedAt).toBeGreaterThan(0)
    expect(service.getTriggersSync().map((trigger) => trigger.keyword)).toEqual(['trae'])
  })
})
