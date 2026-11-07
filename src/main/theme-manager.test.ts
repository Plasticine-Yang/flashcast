import { existsSync } from 'node:fs'
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import type { ThemePalette } from '@shared/theme'
import type { WorkspaceLayout } from './paths'
import { ThemeManager, type ThemeSnapshot } from './theme-manager'

const PALETTE: ThemePalette = {
  bg: '#0b1020',
  bgElevated: '#121a30',
  surface: '#8fb3ff',
  border: '#9db9e8',
  text: '#ffffff',
  textSecondary: '#d5ddef',
  textTertiary: '#c4cfe4',
  accent: '#9cc7ff',
  onAccent: '#ffffff',
  success: '#4ade80',
  warning: '#fbbf24',
  danger: '#ff9e97',
  focus: '#9cc7ff',
  accentFill: '#2563eb',
  accentFillHover: '#1d4fd8'
}

function themeJson(id: string, name = id, version = '1.0.0'): Record<string, unknown> {
  return {
    schemaVersion: 1,
    id,
    name,
    description: 'A workspace theme used by unit tests.',
    version,
    author: 'Flashcast Tests',
    license: 'MIT',
    variants: {
      dark: { label: 'Dark', palette: PALETTE }
    }
  }
}

/** Minimal WorkspaceLayout for ThemeManager; only themes paths are used. */
function layoutFor(root: string): WorkspaceLayout {
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

async function writeTheme(root: string, dirName: string, json: unknown): Promise<void> {
  const dir = join(root, 'themes', dirName)
  await mkdir(dir, { recursive: true })
  await writeFile(join(dir, 'theme.json'), JSON.stringify(json, null, 2), 'utf8')
}

describe('ThemeManager', () => {
  let root: string
  let snapshots: ThemeSnapshot[]
  let manager: ThemeManager

  beforeEach(async () => {
    root = await mkdtemp(join(tmpdir(), 'flashcast-themes-'))
    snapshots = []
    manager = new ThemeManager(
      layoutFor(root),
      (snapshot) => snapshots.push(snapshot),
      () => {}
    )
    await manager.init()
  })

  afterEach(async () => {
    await rm(root, { recursive: true, force: true })
  })

  it('loads the six built-in themes with Blue Glass first', () => {
    expect(manager.getThemes().map((theme) => theme.id)).toEqual([
      'blue-glass',
      'dracula',
      'tokyo-night',
      'kanagawa',
      'atelier',
      'everforest'
    ])
    expect(manager.getThemes()[0].name).toBe('Blue Glass')
    expect(manager.getThemes()[0].source).toBe('builtin')
    expect(manager.getErrors()).toEqual([])
    expect(snapshots.length).toBeGreaterThan(0)
  })

  it('scans a valid workspace theme and exposes it after the built-ins', async () => {
    await writeTheme(root, 'aurora', themeJson('aurora', 'Aurora'))
    await manager.reload()

    const themes = manager.getThemes()
    expect(themes.map((theme) => theme.id)).toEqual([
      'blue-glass',
      'dracula',
      'tokyo-night',
      'kanagawa',
      'atelier',
      'everforest',
      'aurora'
    ])
    expect(manager.getTheme('aurora')?.source).toBe('workspace')
    expect(manager.hasTheme('aurora')).toBe(true)
    expect(manager.getErrors()).toEqual([])
  })

  it('reports invalid JSON, id mismatch and built-in collisions', async () => {
    await mkdir(join(root, 'themes', 'broken-json'), { recursive: true })
    await writeFile(join(root, 'themes', 'broken-json', 'theme.json'), '{ not json', 'utf8')
    await writeTheme(root, 'wrong-dir', themeJson('other-id'))
    await writeTheme(root, 'blue-glass', themeJson('blue-glass', 'Fake Blue Glass'))

    await manager.reload()

    expect(manager.getThemes().map((theme) => theme.id)).toEqual([
      'blue-glass',
      'dracula',
      'tokyo-night',
      'kanagawa',
      'atelier',
      'everforest'
    ])
    const errors = manager.getErrors()
    expect(errors.map((error) => error.id).sort()).toEqual([
      'blue-glass',
      'broken-json',
      'wrong-dir'
    ])
    expect(errors.find((error) => error.id === 'wrong-dir')?.message).toContain('目录名')
    expect(errors.find((error) => error.id === 'blue-glass')?.message).toContain('内置主题冲突')
    expect(manager.getTheme('blue-glass')?.source).toBe('builtin')
  })

  it('inspects, installs and uninstalls a local theme directory', async () => {
    const source = await mkdtemp(join(tmpdir(), 'flashcast-theme-source-'))
    try {
      await writeFile(join(source, 'theme.json'), JSON.stringify(themeJson('solar-flare', 'Solar Flare', '1.2.0'), null, 2), 'utf8')
      await writeFile(join(source, 'LICENSE'), 'MIT License\n', 'utf8')

      const inspection = await manager.inspect(source)
      expect(inspection.sourceType).toBe('local')
      expect(inspection.definition.id).toBe('solar-flare')
      expect(inspection.definition.name).toBe('Solar Flare')
      expect(inspection.warnings).toEqual([])
      expect(inspection.alreadyInstalled).toBe(false)
      expect(inspection.installedVersion).toBeNull()

      const installed = await manager.install(inspection.token)
      expect(installed.id).toBe('solar-flare')
      expect(installed.source).toBe('workspace')
      expect(manager.hasTheme('solar-flare')).toBe(true)

      const themeDir = join(root, 'themes', 'solar-flare')
      expect(await readFile(join(themeDir, 'LICENSE'), 'utf8')).toBe('MIT License\n')
      const meta = JSON.parse(
        await readFile(join(themeDir, '.flashcast-source.json'), 'utf8')
      ) as { sourceType: string; source: string; version: string }
      expect(meta.sourceType).toBe('local')
      expect(meta.source).toBe(source)
      expect(meta.version).toBe('1.2.0')

      const again = await manager.inspect(source)
      expect(again.alreadyInstalled).toBe(true)
      expect(again.installedVersion).toBe('1.2.0')

      await writeFile(
        join(source, 'theme.json'),
        JSON.stringify(themeJson('solar-flare', 'Solar Flare', '1.3.0'), null, 2),
        'utf8'
      )
      const updated = await manager.update('solar-flare')
      expect(updated.ok).toBe(true)
      expect(manager.getTheme('solar-flare')?.version).toBe('1.3.0')
      const latest = await manager.update('solar-flare')
      expect(latest.ok).toBe(true)
      expect(latest.toast?.message).toBe('已是最新版本')

      const result = await manager.uninstall('solar-flare')
      expect(result.ok).toBe(true)
      expect(manager.hasTheme('solar-flare')).toBe(false)
      expect(existsSync(themeDir)).toBe(false)
    } finally {
      await rm(source, { recursive: true, force: true })
    }
  })

  it('keeps built-in themes out of updates and uninstall', async () => {
    expect((await manager.update('blue-glass')).ok).toBe(false)
    expect((await manager.uninstall('blue-glass')).ok).toBe(false)
    expect(manager.hasTheme('blue-glass')).toBe(true)
  })
})
