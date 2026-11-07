import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { GLASS_INTENSITY_DEFAULT } from '@shared/glass'
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

describe('ConfigStore glass preference', () => {
  let root: string

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'flashcast-config-'))
  })

  afterEach(async () => {
    await rm(root, { recursive: true, force: true })
  })

  it('defaults to the shipped frost level and persists updates', async () => {
    const store = new ConfigStore(layoutFor(root), 'darwin')
    await store.init()
    expect(store.getPrefs().glassIntensity).toBe(GLASS_INTENSITY_DEFAULT)

    await store.updatePrefs({ glassIntensity: 42 })
    const raw = JSON.parse(await readFile(join(root, 'config.json'), 'utf8')) as {
      glassIntensity?: number
    }
    expect(raw.glassIntensity).toBe(42)

    const reopened = new ConfigStore(layoutFor(root), 'darwin')
    await reopened.init()
    expect(reopened.getPrefs().glassIntensity).toBe(42)
  })

  it('clamps out-of-range values read from disk', async () => {
    await writeFile(join(root, 'config.json'), JSON.stringify({ version: 1, glassIntensity: 420 }))
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    expect(store.getPrefs().glassIntensity).toBe(100)
  })

  it('falls back to the default when the stored value is not a number', async () => {
    await writeFile(join(root, 'config.json'), JSON.stringify({ version: 1, glassIntensity: 'x' }))
    const store = new ConfigStore(layoutFor(root), 'linux')
    await store.init()
    expect(store.getPrefs().glassIntensity).toBe(GLASS_INTENSITY_DEFAULT)
  })
})
