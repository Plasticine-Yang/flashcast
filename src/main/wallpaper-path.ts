import { fileURLToPath } from 'node:url'

/**
 * `gsettings get` prints GVariant text: a single-quoted string for string keys,
 * e.g. `'file:///home/me/wall.jpg'`. Unescape the small set of sequences a
 * desktop wallpaper path can contain.
 */
export function parseGSettingsString(stdout: string): string | null {
  const value = stdout.trim()
  if (value.length < 2 || !value.startsWith("'") || !value.endsWith("'")) return null
  return value.slice(1, -1).replace(/\\(['"\\])/g, '$1')
}

/** Only local `file://` wallpapers are safe to read; URLs are ignored. */
export function wallpaperPathFromUri(uri: string | null): string | null {
  if (!uri || !uri.startsWith('file://')) return null
  try {
    return fileURLToPath(uri)
  } catch {
    return null
  }
}

/** Parse `reg query "HKCU\Control Panel\Desktop" /v Wallpaper` output. */
export function parseWindowsWallpaper(stdout: string): string | null {
  const match = /^\s*Wallpaper\s+REG_\w+\s+(.+)$/im.exec(stdout)
  if (!match) return null
  const value = match[1].trim().replace(/^"(.*)"$/, '$1')
  return value || null
}

/** GNOME stores the filesystem path in `picture-uri` / `picture-uri-dark`. */
export function gnomeWallpaperKeys(theme: 'dark' | 'light'): string[] {
  return theme === 'dark' ? ['picture-uri-dark', 'picture-uri'] : ['picture-uri']
}

/** Only the `current` host is served; the theme selects the wallpaper variant. */
export function wallpaperRequestTheme(url: string): 'dark' | 'light' | null {
  try {
    const parsed = new URL(url)
    if (parsed.hostname !== 'current') return null
    return parsed.searchParams.get('theme') === 'light' ? 'light' : 'dark'
  } catch {
    return null
  }
}
