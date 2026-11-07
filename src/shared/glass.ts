import type { ResolvedTheme } from './types'

/**
 * User-tunable frostiness of the launcher glass.
 *
 * The launcher window is transparent, so a `backdrop-filter` cannot reliably
 * sample the desktop behind it (Electron limitation, see docs/ui-ux.md §3).
 * What actually controls "how much I can see through the panel" is the panel's
 * own alpha plus the wallpaper-proxy layer, so one 0–100 value drives both of
 * those together with the blur radii used for in-page overlays.
 */
export const GLASS_INTENSITY_MIN = 0
export const GLASS_INTENSITY_MAX = 100
export const GLASS_INTENSITY_STEP = 2
export const GLASS_INTENSITY_DEFAULT = 80

export interface GlassSettings {
  /** Panel background alpha (0–1). */
  alpha: number
  /** Backdrop blur for the panel and popovers, in px. */
  blur: number
  /** Backdrop blur for cards/toasts, in px. */
  softBlur: number
  /** Strength of the wallpaper-proxy layer (0–1). */
  wallpaperOpacity: number
  /** Wallpaper-proxy blur radius, in px. */
  wallpaperBlur: number
  /** Frost grain opacity. */
  frostOpacity: number
}

export function normalizeGlassIntensity(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return GLASS_INTENSITY_DEFAULT
  return Math.min(GLASS_INTENSITY_MAX, Math.max(GLASS_INTENSITY_MIN, Math.round(value)))
}

const round3 = (value: number): number => Math.round(value * 1000) / 1000

export function glassSettings(intensity: number, theme: ResolvedTheme): GlassSettings {
  const t = normalizeGlassIntensity(intensity) / 100
  const dark = theme === 'dark'
  return {
    // 0.86 (通透) → 0.99 (磨砂): enough range to feel the difference without
    // ever fully hiding the wallpaper hue at the low end.
    alpha: round3((dark ? 0.86 : 0.84) + (dark ? 0.13 : 0.145) * t),
    blur: Math.round((dark ? 30 : 28) + 66 * t),
    softBlur: Math.round((dark ? 18 : 16) + 40 * t),
    wallpaperOpacity: round3((dark ? 0.58 : 0.52) - (dark ? 0.32 : 0.3) * t),
    wallpaperBlur: Math.round(48 + 80 * t),
    frostOpacity: round3(dark ? 0.03 + 0.045 * t : 0.022 + 0.03 * t)
  }
}

/** CSS custom properties consumed by tokens.css / launcher.css. */
export function glassCssVariables(
  intensity: number,
  theme: ResolvedTheme
): Record<string, string> {
  const settings = glassSettings(intensity, theme)
  return {
    '--fc-window-alpha': String(settings.alpha),
    '--fc-glass-blur': `${settings.blur}px`,
    '--fc-glass-blur-soft': `${settings.softBlur}px`,
    '--fc-wallpaper-opacity': String(settings.wallpaperOpacity),
    '--fc-wallpaper-blur': `${settings.wallpaperBlur}px`,
    '--fc-frost-opacity': String(settings.frostOpacity)
  }
}

export function glassIntensityLabel(intensity: number): string {
  const value = normalizeGlassIntensity(intensity)
  if (value < 25) return '通透'
  if (value < 60) return '轻柔'
  if (value < 88) return '标准'
  return '浓郁'
}
