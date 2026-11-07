import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ConfigStore } from './config-store'

/** Minimal WorkspaceLayout used by ConfigStore tests. */
function layoutFor(root: string) {
  return {
    root,
    configFile: join(root, 'config.json'),
    pluginsDir: join(root, 'plugins'),
    pluginDir: (id: string) => join(root, 'plugins', id),
    pluginConfigFile: (id: string) => join(root, 'plugins', id, 'config.json'),
    extensionsDir: join(root, 'extensions'),
    extensionDir: (id: string) => join(root, 'extensions', id),
    themesDir: join(root, 'themes'),
    themeDir: (id: string) => join(root, 'themes', id),
    stateDir: join(root, 'state'),
    clipboardDir: join(root, 'state', 'clipboard'),
    clipboardMediaDir: join(root, 'state', 'clipboard', 'media'),
    clipboardHistoryFile: join(root, 'state', 'clipboard', 'history.json'),
    faviconDir: join(root, 'state', 'favicons'),
    installTempDir: join(root, 'state', 'install-temp')
  }
}

describe('ConfigStore preferences', () => {
  let root: string

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'flashcast-config-'))
  })

  afterEach(async () => {
    await rm(root, { recursive: true, force: true })
  })

  it('starts from defaults and persists theme changes', async () => {
    const store = new ConfigStore(layoutFor(root), 'darwin')
    await store.init()
    expect(store.getPrefs().theme).toBe('dark')
    expect(store.getPrefs().themeId).toBe('blue-glass')
    expect(store.getPrefs().launchAtLogin).toBe(false)

    await store.updatePrefs({ theme: 'light', themeId: 'midnight-azure' })
    const raw = JSON.parse(await readFile(join(root, 'config.json'), 'utf8')) as {
      theme?: string
      themeId?: string
    }
    expect(raw.theme).toBe('light')
    expect(raw.themeId).toBe('midnight-azure')

    const reopened = new ConfigStore(layoutFor(root), 'darwin')
    await reopened.init()
    expect(reopened.getPrefs().theme).toBe('light')
    expect(reopened.getPrefs().themeId).toBe('midnight-azure')
  })

  it('falls back to defaults when the stored theme is invalid', async () => {
    await writeFile(join(root, 'config.json'), JSON.stringify({ version: 1, theme: 'neon' }))
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    expect(store.getPrefs().theme).toBe('dark')
  })

  it('falls back to blue-glass when the stored theme id is invalid', async () => {
    await writeFile(
      join(root, 'config.json'),
      JSON.stringify({ version: 1, theme: 'dark', themeId: '   ' })
    )
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    expect(store.getPrefs().themeId).toBe('blue-glass')
  })

  it('rejects an empty theme id in updates without persisting it', async () => {
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    await store.updatePrefs({ themeId: '  ' })
    expect(store.getPrefs().themeId).toBe('blue-glass')
    const raw = JSON.parse(await readFile(join(root, 'config.json'), 'utf8')) as {
      themeId?: string
    }
    expect(raw.themeId).toBe('blue-glass')
  })

  it('drops unknown legacy keys on the next write', async () => {
    await writeFile(
      join(root, 'config.json'),
      JSON.stringify({ version: 1, theme: 'light', glassIntensity: 80 })
    )
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    await store.updatePrefs({ theme: 'dark' })

    const raw = JSON.parse(await readFile(join(root, 'config.json'), 'utf8')) as Record<
      string,
      unknown
    >
    expect(raw.glassIntensity).toBeUndefined()
    expect(raw.theme).toBe('dark')
  })
})
