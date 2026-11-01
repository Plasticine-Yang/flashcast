import { describe, expect, it } from 'vitest'
import {
  gnomeWallpaperKeys,
  parseGSettingsString,
  parseWindowsWallpaper,
  wallpaperPathFromUri,
  wallpaperRequestTheme
} from './wallpaper-path'

describe('wallpaper path parsing', () => {
  it('reads a quoted gsettings string and unescapes quotes', () => {
    expect(parseGSettingsString("'file:///home/me/Pictures/wall.jpg'\n")).toBe(
      'file:///home/me/Pictures/wall.jpg'
    )
    expect(parseGSettingsString("'file:///home/me/it\\'s.jpg'")).toBe("file:///home/me/it's.jpg")
    expect(parseGSettingsString('uint32 1')).toBeNull()
    expect(parseGSettingsString('')).toBeNull()
  })

  it('decodes file URIs and rejects remote URLs', () => {
    expect(wallpaperPathFromUri('file:///home/me/%E5%A3%81%E7%BA%B8.jpg')).toBe(
      '/home/me/壁纸.jpg'
    )
    expect(wallpaperPathFromUri('https://example.com/wall.jpg')).toBeNull()
    expect(wallpaperPathFromUri(null)).toBeNull()
  })

  it('parses Windows reg query output', () => {
    const output = `
HKEY_CURRENT_USER\\Control Panel\\Desktop
    Wallpaper    REG_SZ    C:\\Users\\me\\Pictures\\wall.jpg

`
    expect(parseWindowsWallpaper(output)).toBe('C:\\Users\\me\\Pictures\\wall.jpg')
    expect(parseWindowsWallpaper('')).toBeNull()
  })

  it('prefers the dark GNOME wallpaper when the theme is dark', () => {
    expect(gnomeWallpaperKeys('dark')).toEqual(['picture-uri-dark', 'picture-uri'])
    expect(gnomeWallpaperKeys('light')).toEqual(['picture-uri'])
  })

  it('only accepts the current wallpaper host and reads the theme', () => {
    expect(wallpaperRequestTheme('flashcast-wallpaper://current?theme=light')).toBe('light')
    expect(wallpaperRequestTheme('flashcast-wallpaper://current?theme=dark')).toBe('dark')
    expect(wallpaperRequestTheme('flashcast-wallpaper://other')).toBeNull()
    expect(wallpaperRequestTheme('not a url')).toBeNull()
  })
})
