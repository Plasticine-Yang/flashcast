import { DatabaseSync } from 'node:sqlite'
import { mkdir, mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { EditorWorkspace } from '@shared/types'
import type { VariantRecord } from './detect'
import { pruneWorkspaces } from './prune'

let root: string

async function write(path: string, content: string): Promise<void> {
  await mkdir(join(path, '..'), { recursive: true })
  await writeFile(path, content, 'utf8')
}

function workspace(path: string, id: string): EditorWorkspace {
  return {
    id,
    variantId: 'code',
    name: id,
    path,
    displayPath: path,
    uri: `file://${path}`,
    kind: 'folder',
    remote: null,
    isOpen: false,
    lastActiveAt: 0,
    exists: true
  }
}

function variant(dataDir: string): VariantRecord {
  return {
    id: 'code',
    name: 'Code',
    version: '1.0.0',
    dataDir,
    installed: true,
    error: null,
    appPath: join(root, 'nowhere.app'),
    appDir: join(root, 'nowhere.app', 'Contents', 'Resources', 'app'),
    cliPath: null,
    cliScript: null,
    executable: null,
    launchKind: 'open',
    hasUserData: true
  }
}

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flashcast-prune-'))
})

afterEach(async () => {
  await rm(root, { recursive: true, force: true })
})

describe('workspace history pruning', () => {
  it('removes one workspace from every history source and keeps the project folder', async () => {
    const dataDir = join(root, 'User Data')
    const keep = join(root, 'projects', 'keep')
    const drop = join(root, 'projects', 'drop')
    await mkdir(keep, { recursive: true })
    await mkdir(drop, { recursive: true })
    await write(join(drop, 'package.json'), '{}')

    const globalStorage = join(dataDir, 'User', 'globalStorage')
    await write(
      join(globalStorage, 'storage.json'),
      JSON.stringify({
        windowsState: {
          openedWindows: [{ folder: `file://${drop}` }, { folder: `file://${keep}` }],
          lastActiveWindow: { folder: `file://${drop}` }
        },
        openedPathsList: {
          entries: [{ folderUri: `file://${drop}` }, { folderUri: `file://${keep}` }]
        },
        profileAssociations: {
          workspaces: { [`file://${drop}`]: '__default__profile__', [`file://${keep}`]: '__default__profile__' }
        },
        backupWorkspaces: {
          folders: [{ folderUri: `file://${drop}` }],
          workspaces: []
        }
      })
    )

    const db = new DatabaseSync(join(globalStorage, 'state.vscdb'))
    db.exec('CREATE TABLE ItemTable (key TEXT, value TEXT)')
    db.prepare('INSERT INTO ItemTable (key, value) VALUES (?, ?)').run(
      'history.recentlyOpenedPathsList',
      JSON.stringify({
        entries: [
          { folderUri: `file://${drop}` },
          { folderUri: `file://${keep}` },
          { fileUri: `file://${join(root, 'notes.md')}` }
        ]
      })
    )
    db.close()

    await write(
      join(dataDir, 'User', 'workspaceStorage', 'drop-hash', 'workspace.json'),
      JSON.stringify({ folder: `file://${drop}` })
    )
    await write(
      join(dataDir, 'User', 'workspaceStorage', 'keep-hash', 'workspace.json'),
      JSON.stringify({ folder: `file://${keep}` })
    )
    await write(join(dataDir, 'User', 'workspaceStorage', 'drop-hash', 'state.vscdb'), 'x')

    const backupRoot = join(root, 'backups')
    const result = await pruneWorkspaces(
      variant(dataDir),
      [workspace(drop, 'drop')],
      backupRoot,
      'darwin'
    )

    expect(result.running).toBe(false)
    expect(result.failures).toEqual([])
    expect(result.removed).toBeGreaterThan(0)
    expect(result.backupDir).not.toBeNull()

    const storage = JSON.parse(
      await readFile(join(globalStorage, 'storage.json'), 'utf8')
    ) as {
      windowsState: { openedWindows: Array<{ folder: string }>; lastActiveWindow: unknown }
      openedPathsList: { entries: Array<{ folderUri: string }> }
      profileAssociations: { workspaces: Record<string, string> }
      backupWorkspaces: { folders: Array<{ folderUri: string }> }
    }
    expect(storage.windowsState.openedWindows).toHaveLength(1)
    expect(storage.windowsState.openedWindows[0].folder).toContain('keep')
    expect(storage.windowsState.lastActiveWindow).toBeNull()
    expect(storage.openedPathsList.entries).toHaveLength(1)
    expect(Object.keys(storage.profileAssociations.workspaces)).toEqual([`file://${keep}`])
    expect(storage.backupWorkspaces.folders).toEqual([])

    const check = new DatabaseSync(join(globalStorage, 'state.vscdb'), { readOnly: true })
    const row = check
      .prepare('SELECT value FROM ItemTable WHERE key = ?')
      .get('history.recentlyOpenedPathsList') as { value: string }
    check.close()
    const entries = (JSON.parse(row.value) as { entries: Array<{ folderUri?: string; fileUri?: string }> })
      .entries
    expect(entries).toHaveLength(2)
    expect(entries.some((entry) => entry.folderUri?.includes('drop'))).toBe(false)

    const remaining = await readdir(join(dataDir, 'User', 'workspaceStorage'))
    expect(remaining).toEqual(['keep-hash'])
    const backedUp = await readdir(join(result.backupDir!, 'workspaceStorage'))
    expect(backedUp).toEqual(['drop-hash'])

    // The project directory itself is never removed.
    expect(existsSync(join(drop, 'package.json'))).toBe(true)
    expect(existsSync(join(result.backupDir!, 'state.vscdb'))).toBe(true)
    expect(existsSync(join(result.backupDir!, 'storage.json'))).toBe(true)
  })

  it('does nothing when the target list is empty', async () => {
    const dataDir = join(root, 'User Data')
    const result = await pruneWorkspaces(variant(dataDir), [], join(root, 'backups'), 'darwin')
    expect(result).toMatchObject({ removed: 0, backupDir: null, running: false, failures: [] })
  })

  it('leaves unrelated workspaces and unknown URIs alone', async () => {
    const dataDir = join(root, 'User Data')
    const keep = join(root, 'projects', 'keep')
    await mkdir(keep, { recursive: true })
    const globalStorage = join(dataDir, 'User', 'globalStorage')
    await write(
      join(globalStorage, 'storage.json'),
      JSON.stringify({
        windowsState: { openedWindows: [{ folder: `file://${keep}` }] },
        profileAssociations: { workspaces: { [`file://${keep}`]: 'p' } }
      })
    )

    const result = await pruneWorkspaces(
      variant(dataDir),
      [workspace(join(root, 'projects', 'drop'), 'drop')],
      join(root, 'backups'),
      'darwin'
    )
    expect(result.removed).toBe(0)
    expect(result.backupDir).toBeNull()

    const storage = JSON.parse(
      await readFile(join(globalStorage, 'storage.json'), 'utf8')
    ) as { windowsState: { openedWindows: unknown[] } }
    expect(storage.windowsState.openedWindows).toHaveLength(1)
  })
})
