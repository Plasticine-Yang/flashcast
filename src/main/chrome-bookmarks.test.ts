import { afterEach, describe, expect, it, vi } from 'vitest'
import { mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import {
  discoverChromeProfiles,
  flattenBookmarks,
  readChromeBookmarks
} from './chrome-bookmarks'
import { parseDesktopEntry, parseExec } from './apps'

const fixtureDirs: string[] = []

async function createChromeFixture(
  localState: unknown,
  profiles: Record<string, Record<string, string>>
): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), 'flashcast-chrome-'))
  fixtureDirs.push(root)
  if (localState !== undefined) {
    await writeFile(join(root, 'Local State'), JSON.stringify(localState), 'utf8')
  }
  for (const [name, files] of Object.entries(profiles)) {
    const profileDir = join(root, name)
    await mkdir(profileDir, { recursive: true })
    for (const [file, content] of Object.entries(files)) {
      await writeFile(join(profileDir, file), content, 'utf8')
    }
  }
  vi.stubEnv('FLASHCAST_CHROME_USER_DATA', root)
  return root
}

afterEach(async () => {
  vi.unstubAllEnvs()
  await Promise.all(fixtureDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
})

describe('Chrome profile discovery', () => {
  it('finds a profile that has never written a Bookmarks file', async () => {
    const root = await createChromeFixture(
      { profile: { info_cache: { Default: { name: '文锋' } } } },
      { Default: { Preferences: '{}' } }
    )

    const { profiles, userDataDir } = await discoverChromeProfiles('linux')

    expect(userDataDir).toBe(root)
    expect(profiles).toEqual([
      { dir: 'Default', name: '文锋', bookmarksPath: join(root, 'Default', 'Bookmarks') }
    ])
  })

  it('trusts Local State profiles even before Chrome writes profile markers', async () => {
    const root = await createChromeFixture(
      { profile: { info_cache: { 'Profile 1': { name: 'Work' } } } },
      { 'Profile 1': {} }
    )

    const { profiles } = await discoverChromeProfiles('linux')

    expect(profiles).toEqual([
      { dir: 'Profile 1', name: 'Work', bookmarksPath: join(root, 'Profile 1', 'Bookmarks') }
    ])
  })

  it('falls back to profile directories when Local State is missing', async () => {
    await createChromeFixture(undefined, {
      Default: { Preferences: '{}' },
      'Profile 1': { Bookmarks: '{"roots":{}}' },
      Crashpad: {}
    })

    const { profiles } = await discoverChromeProfiles('linux')

    expect(profiles.map((profile) => profile.dir)).toEqual(['Default', 'Profile 1'])
  })

  it('treats a missing Bookmarks file as an empty profile instead of failing', async () => {
    const root = await createChromeFixture(
      { profile: { info_cache: { Default: { name: '文锋' } } } },
      { Default: { Preferences: '{}' } }
    )

    const result = await readChromeBookmarks('linux', null)

    expect(result.error).toBeNull()
    expect(result.selectedProfile).toBe('Default')
    expect(result.bookmarks).toEqual([])
    expect(result.bookmarkFile).toBe(join(root, 'Default', 'Bookmarks'))
  })
})

describe('Chrome bookmark parsing', () => {
  it('flattens nested folders in bookmark-bar order', () => {
    const bookmarks = flattenBookmarks({
      roots: {
        bookmark_bar: {
          children: [
            { type: 'url', name: 'GitHub', url: 'https://github.com', id: 'a' },
            {
              type: 'folder',
              name: '工作',
              children: [
                { type: 'url', name: 'Docs', url: 'https://example.com/docs', id: 'b' },
                { type: 'folder', name: '子目录', children: [{ type: 'url', name: 'Deep', url: 'https://deep.example', id: 'c' }] }
              ]
            }
          ]
        },
        other: { children: [{ type: 'url', name: 'Other', url: 'https://other.example', id: 'd' }] }
      }
    })
    expect(bookmarks.map((bookmark) => bookmark.title)).toEqual(['GitHub', 'Docs', 'Deep', 'Other'])
    expect(bookmarks.find((bookmark) => bookmark.id === 'b')?.folder).toEqual(['工作'])
    expect(bookmarks.find((bookmark) => bookmark.id === 'c')?.folder).toEqual(['工作', '子目录'])
    expect(bookmarks.find((bookmark) => bookmark.id === 'a')?.domain).toBe('github.com')
  })

  it('skips unsupported protocols and malformed URLs', () => {
    const bookmarks = flattenBookmarks({
      roots: {
        bookmark_bar: {
          children: [
            { type: 'url', name: 'JS', url: 'javascript:alert(1)' },
            { type: 'url', name: 'Valid', url: 'https://valid.example' }
          ]
        }
      }
    })
    expect(bookmarks.map((bookmark) => bookmark.title)).toEqual(['Valid'])
  })
})

describe('desktop entry parsing', () => {
  const raw = `[Desktop Entry]
Type=Application
Name=Example
Name[zh_CN]=示例应用
GenericName=Example App
Comment=An example application
Exec=/usr/bin/example %U
Icon=example
Keywords=demo;example;
Categories=Development;IDE;
Terminal=false
NoDisplay=false
`

  it('parses localized names and exec lines', () => {
    const entry = parseDesktopEntry(raw, '/usr/share/applications/example.desktop')
    expect(entry?.name).toBe('示例应用')
    expect(entry?.exec).toBe('/usr/bin/example %U')
    expect(entry?.categories).toEqual(['Development', 'IDE'])
    expect(parseExec(entry!.exec)).toEqual(['/usr/bin/example'])
  })

  it('ignores hidden and non-application entries', () => {
    expect(parseDesktopEntry(raw.replace('NoDisplay=false', 'NoDisplay=true'), 'x.desktop')).toBeNull()
    expect(
      parseDesktopEntry(raw.replace('Type=Application', 'Type=Link'), 'x.desktop')
    ).toBeNull()
  })
})
