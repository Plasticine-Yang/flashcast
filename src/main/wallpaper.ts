import { execFile } from 'node:child_process'
import { readFile, stat } from 'node:fs/promises'
import { extname } from 'node:path'
import { promisify } from 'node:util'
import { nativeImage } from 'electron'
import type { ResolvedTheme } from '@shared/types'
import { log } from './logger'
import {
  gnomeWallpaperKeys,
  parseGSettingsString,
  parseWindowsWallpaper,
  wallpaperPathFromUri
} from './wallpaper-path'

const execFileAsync = promisify(execFile)
const MAX_WIDTH = 960
const READABLE_EXTENSIONS = new Set(['.jpg', '.jpeg', '.png'])

export interface WallpaperImage {
  data: Buffer
  contentType: string
}

/** Locate the current desktop wallpaper. Unknown platforms return null. */
export async function resolveWallpaperPath(theme: ResolvedTheme): Promise<string | null> {
  if (process.platform === 'linux') {
    for (const key of gnomeWallpaperKeys(theme)) {
      try {
        const { stdout } = await execFileAsync(
          'gsettings',
          ['get', 'org.gnome.desktop.background', key],
          { timeout: 1500 }
        )
        const path = wallpaperPathFromUri(parseGSettingsString(stdout))
        if (path) return path
      } catch {
        // gsettings is missing (non-GNOME desktop) or the key is unavailable.
      }
    }
    return null
  }

  if (process.platform === 'win32') {
    try {
      const { stdout } = await execFileAsync(
        'reg',
        ['query', 'HKCU\\Control Panel\\Desktop', '/v', 'Wallpaper'],
        { timeout: 1500 }
      )
      return parseWindowsWallpaper(stdout)
    } catch {
      return null
    }
  }

  // macOS keeps the desktop picture behind an automation permission prompt;
  // the launcher intentionally degrades to plain frosted glass there.
  return null
}

async function encodeWallpaper(path: string): Promise<WallpaperImage | null> {
  if (!READABLE_EXTENSIONS.has(extname(path).toLowerCase())) return null
  try {
    const file = await readFile(path)
    const image = nativeImage.createFromBuffer(file)
    if (image.isEmpty()) return null
    const { width } = image.getSize()
    if (!width) return null
    const resized = width > MAX_WIDTH ? image.resize({ width: MAX_WIDTH, quality: 'good' }) : image
    return { data: resized.toJPEG(74), contentType: 'image/jpeg' }
  } catch (error) {
    log.warn('wallpaper read failed:', (error as Error).message)
    return null
  }
}

interface WallpaperCache {
  key: string
  image: WallpaperImage | null
}

let cache: WallpaperCache | null = null

/**
 * Read (and downscale) the current wallpaper for the frosted backdrop. The
 * result is cached by path + mtime + theme so reopening the launcher does not
 * repeatedly decode a 4K image.
 */
export async function readDesktopWallpaper(theme: ResolvedTheme): Promise<WallpaperImage | null> {
  const path = await resolveWallpaperPath(theme)
  if (!path) return null
  try {
    const info = await stat(path)
    const key = `${theme}:${path}:${info.mtimeMs}`
    if (cache?.key === key) return cache.image
    const image = await encodeWallpaper(path)
    cache = { key, image }
    return image
  } catch {
    return null
  }
}
