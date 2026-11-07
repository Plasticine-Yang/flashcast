import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ConfigStore } from './config-store'

/** Only the paths ConfigStore.init()/persist() touches. */
function layoutFor(root: string) {
  return {
    root,
    configFile: join(root, 'config.json'),
    pluginsDir: join(root, 'plugins'),
    pluginDir: (id: string) => join(root, 'plugins', id),
    pluginConfigFile: (id: string) => join(root, 'plugins', id, 'config.json'),
    extensionsDir: join(root, 'extensions'),
    extensionDir: (id: string) => join(root, 'extensions', id),
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
    expect(store.getPrefs().launchAtLogin).toBe(false)

    await store.updatePrefs({ theme: 'light' })
    const raw = JSON.parse(await readFile(join(root, 'config.json'), 'utf8')) as {
      theme?: string
    }
    expect(raw.theme).toBe('light')

    const reopened = new ConfigStore(layoutFor(root), 'darwin')
    await reopened.init()
    expect(reopened.getPrefs().theme).toBe('light')
  })

  it('falls back to defaults when the stored theme is invalid', async () => {
    await writeFile(join(root, 'config.json'), JSON.stringify({ version: 1, theme: 'neon' }))
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    expect(store.getPrefs().theme).toBe('dark')
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
