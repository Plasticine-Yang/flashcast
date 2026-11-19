import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const state = vi.hoisted(() => ({
  userData: '',
  version: '0.4.7',
  trusted: false
}))

vi.mock('electron', () => ({
  app: {
    getPath: () => state.userData,
    getAppPath: () => state.userData,
    getVersion: () => state.version,
    isPackaged: false
  },
  systemPreferences: {
    isTrustedAccessibilityClient: () => state.trusted
  }
}))

const { PermissionService } = await import('./permissions')

let root: string

beforeEach(async () => {
  root = await mkdtemp(join(tmpdir(), 'flashcast-permissions-'))
  state.userData = root
  state.version = '0.4.7'
  state.trusted = false
})

afterEach(async () => {
  await rm(root, { recursive: true, force: true })
})

async function writeState(value: unknown): Promise<void> {
  await writeFile(join(root, 'permissions.json'), JSON.stringify(value), 'utf8')
}

describe('PermissionService', () => {
  it('reports everything granted on non-macOS platforms', async () => {
    const status = await new PermissionService('linux').status()
    expect(status.needsAttention).toBe(false)
    expect(status.stale).toBe(false)
  })

  it('flags a stale grant after the app version changed', async () => {
    await writeState({ lastGrantedVersion: '0.4.6', lastGrantedAt: 1 })
    const status = await new PermissionService('darwin').status()
    expect(status.accessibility).toBe(false)
    expect(status.needsAttention).toBe(true)
    expect(status.previouslyGranted).toBe(true)
    expect(status.stale).toBe(true)
  })

  it('records the current version once the grant works again', async () => {
    await writeState({ lastGrantedVersion: '0.4.6', lastGrantedAt: 1 })
    state.trusted = true
    const service = new PermissionService('darwin')
    const status = await service.status()
    expect(status.needsAttention).toBe(false)
    expect(status.stale).toBe(false)
    expect(status.lastGrantedVersion).toBe('0.4.7')

    await vi.waitFor(async () => {
      const stored = JSON.parse(await readFile(join(root, 'permissions.json'), 'utf8')) as {
        lastGrantedVersion: string
      }
      expect(stored.lastGrantedVersion).toBe('0.4.7')
    })
  })

  it('explains that reset cannot run without a bundle identifier', async () => {
    const result = await new PermissionService('darwin').reset('accessibility')
    expect(result.ok).toBe(false)
    expect(result.error).toContain('手动')
  })
})
