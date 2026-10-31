import { describe, expect, it } from 'vitest'
import { flattenBookmarks } from './chrome-bookmarks'
import { parseDesktopEntry, parseExec } from './apps'

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
