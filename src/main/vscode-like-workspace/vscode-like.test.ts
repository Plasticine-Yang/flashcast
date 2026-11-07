import { DatabaseSync } from 'node:sqlite'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { detectVariants, type VariantRecord } from './detect'
import { indexVariant } from './indexer'
import { parseWorkspaceUri } from './uri'

let root: string

async function write(path: string, content: string): Promise<void> {
  await mkdir(join(path, '..'), { recursive: true })
  await writeFile(path, content, 'utf8')
}

function variant(dataDir: string): VariantRecord {
  return {
    id: 'foo',
    name: 'Foo Editor',
    version: '1.0.0',
    dataDir,
    installed: true,
    error: null,
    appPath: '/Applications/Foo.app',
    appDir: '/Applications/Foo.app/Contents/Resources/app',
    cliPath: null,
    cliScript: null,
    executable: null,
    launchKind: 'open',
    hasUserData: true
  }
}

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flashcast-editor-'))
})

afterEach(async () => {
  await rm(root, { recursive: true, force: true })
})

describe('VS Code-like detection', () => {
  it('detects an app from product.json and resolves its data directory', async () => {
    const app = join(root, 'Applications', 'Foo.app', 'Contents', 'Resources', 'app')
    await write(
      join(app, 'product.json'),
      JSON.stringify({
        applicationName: 'foo',
        nameShort: 'Foo Editor',
        nameLong: 'Foo Editor',
        dataFolderName: '.foo',
        urlProtocol: 'foo',
        version: '1.2.3'
      })
    )
    await write(join(app, 'package.json'), JSON.stringify({ name: 'Foo Editor', version: '1.2.3' }))
    await write(join(app, 'out', 'main.js'), '')
    await write(join(app, 'bin', 'foo'), '#!/bin/sh\n')
    await mkdir(join(root, 'Library', 'Application Support', 'Foo Editor', 'User'), {
      recursive: true
    })

    const variants = await detectVariants({
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Applications')]
    })

    expect(variants).toHaveLength(1)
    expect(variants[0]).toMatchObject({
      id: 'foo',
      name: 'Foo Editor',
      version: '1.2.3',
      dataDir: join(root, 'Library', 'Application Support', 'Foo Editor')
    })
    expect(variants[0].cliPath).toBe(join(app, 'bin', 'foo'))
    expect(variants[0].hasUserData).toBe(true)
  })

  it('ignores Electron apps without a VS Code product identity', async () => {
    const app = join(root, 'Applications', 'Other.app', 'Contents', 'Resources', 'app')
    await write(join(app, 'product.json'), JSON.stringify({ nameShort: 'Other' }))
    await write(join(app, 'out', 'main.js'), '')

    const variants = await detectVariants({
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Applications')]
    })
    expect(variants).toEqual([])
  })

  it('prefers the copy that has user data when two installs share an application name', async () => {
    for (const name of ['Foo', 'Bar']) {
      const app = join(root, name, 'Foo.app', 'Contents', 'Resources', 'app')
      await write(
        join(app, 'product.json'),
        JSON.stringify({
          applicationName: 'foo',
          nameShort: name === 'Foo' ? 'Foo Editor' : 'Bar Editor',
          dataFolderName: '.foo',
          urlProtocol: 'foo',
          version: name === 'Foo' ? '1.0.0' : '2.0.0'
        })
      )
      await write(join(app, 'out', 'main.js'), '')
    }
    await mkdir(join(root, 'Library', 'Application Support', 'Foo Editor', 'User'), {
      recursive: true
    })

    const variants = await detectVariants({
      platform: 'darwin',
      home: root,
      env: {},
      roots: [join(root, 'Foo'), join(root, 'Bar')]
    })
    expect(variants).toHaveLength(1)
    expect(variants[0].version).toBe('1.0.0')
    expect(variants[0].hasUserData).toBe(true)
  })
})

describe('VS Code-like workspace URI parsing', () => {
  it('parses local folders and .code-workspace files', () => {
    const folder = parseWorkspaceUri('file:///Users/me/my%20project', 'darwin')
    expect(folder).toMatchObject({
      path: '/Users/me/my project',
      kind: 'folder'
    })
    const workspace = parseWorkspaceUri('file:///Users/me/app.code-workspace', 'darwin')
    expect(workspace).toMatchObject({ kind: 'workspace', path: '/Users/me/app.code-workspace' })
  })

  it('decodes remote authorities and keeps the authority', () => {
    const parsed = parseWorkspaceUri(
      'vscode-remote://ssh-remote%2Bdevbox/cloudide/workspace',
      'darwin'
    )
    expect(parsed?.remote).toMatchObject({
      authority: 'ssh-remote+devbox',
      host: 'devbox'
    })
    expect(parsed?.path).toBe('/cloudide/workspace')
  })

  it('normalises Windows drive letters and UNC paths', () => {
    expect(parseWorkspaceUri('file:///c%3A/Users/me/app', 'win32')?.path).toBe('c:\\Users\\me\\app')
    expect(parseWorkspaceUri('file://server/share/app', 'win32')?.path).toBe(
      '\\\\server\\share\\app'
    )
  })
})

describe('VS Code-like workspace index', () => {
  it('merges recents, open windows and workspace storage without duplicates', async () => {
    const dataDir = join(root, 'User Data')
    const openFolder = join(root, 'projects', 'open')
    const recentFolder = join(root, 'projects', 'recent')
    const assocFolder = join(root, 'projects', 'associated')
    for (const folder of [openFolder, recentFolder, assocFolder]) {
      await mkdir(folder, { recursive: true })
    }
    const workspaceFile = join(root, 'projects', 'app.code-workspace')
    await write(workspaceFile, '{}')

    const globalStorage = join(dataDir, 'User', 'globalStorage')
    await write(
      join(globalStorage, 'storage.json'),
      JSON.stringify({
        windowsState: {
          openedWindows: [{ folder: `file://${openFolder}` }],
          lastActiveWindow: { folder: `file://${openFolder}` }
        },
        profileAssociations: { workspaces: { [`file://${assocFolder}`]: '__default__profile__' } }
      })
    )

    await write(
      join(dataDir, 'User', 'workspaceStorage', 'abc', 'workspace.json'),
      JSON.stringify({ folder: `file://${recentFolder}` })
    )
    await write(
      join(dataDir, 'User', 'workspaceStorage', 'def', 'workspace.json'),
      JSON.stringify({ workspace: `file://${workspaceFile}` })
    )

    await mkdir(globalStorage, { recursive: true })
    const db = new DatabaseSync(join(globalStorage, 'state.vscdb'))
    db.exec('CREATE TABLE ItemTable (key TEXT, value TEXT)')
    db.prepare('INSERT INTO ItemTable (key, value) VALUES (?, ?)').run(
      'history.recentlyOpenedPathsList',
      JSON.stringify({
        entries: [
          { folderUri: `file://${openFolder}` },
          { folderUri: `file://${recentFolder}` },
          { workspace: { configPath: `file://${workspaceFile}` } },
          { fileUri: `file://${join(root, 'projects', 'note.md')}` }
        ]
      })
    )
    db.close()

    const result = await indexVariant(variant(dataDir), {
      platform: 'darwin',
      home: root,
      includeRemote: true,
      hideMissing: false,
      maxItems: 100
    })

    expect(result.error).toBeNull()
    const byName = new Map(result.workspaces.map((workspace) => [workspace.name, workspace]))
    expect(result.workspaces.filter((workspace) => workspace.path === openFolder)).toHaveLength(1)
    expect(byName.get('open')?.isOpen).toBe(true)
    expect(byName.get('recent')?.kind).toBe('folder')
    expect(byName.get('app')?.kind).toBe('workspace')
    expect(byName.get('associated')).toBeDefined()
    expect(result.workspaces.find((workspace) => workspace.name === 'note.md')).toBeUndefined()
    expect(result.workspaces[0].path).toBe(openFolder)
  })

  it('keeps remote workspaces optional and hides missing paths', async () => {
    const dataDir = join(root, 'User Data')
    const missing = join(root, 'projects', 'missing')
    const globalStorage = join(dataDir, 'User', 'globalStorage')
    await write(
      join(globalStorage, 'storage.json'),
      JSON.stringify({
        windowsState: { openedWindows: [] },
        profileAssociations: {
          workspaces: {
            'vscode-remote://ssh-remote%2Bdevbox/cloudide/workspace': '__default__profile__',
            [`file://${missing}`]: '__default__profile__'
          }
        }
      })
    )

    const remote = await indexVariant(variant(dataDir), {
      platform: 'darwin',
      home: root,
      includeRemote: true,
      hideMissing: true,
      maxItems: 100
    })
    expect(remote.workspaces).toHaveLength(1)
    expect(remote.workspaces[0].remote?.host).toBe('devbox')
    expect(remote.workspaces[0].displayPath).toBe('devbox:/cloudide/workspace')

    const hidden = await indexVariant(variant(dataDir), {
      platform: 'darwin',
      home: root,
      includeRemote: false,
      hideMissing: false,
      maxItems: 100
    })
    expect(hidden.workspaces).toHaveLength(1)
    expect(hidden.workspaces[0].name).toBe('missing')
    expect(hidden.workspaces[0].exists).toBe(false)
  })
})
