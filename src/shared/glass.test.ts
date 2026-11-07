import { describe, expect, it } from 'vitest'
import {
  GLASS_INTENSITY_DEFAULT,
  GLASS_INTENSITY_MAX,
  GLASS_INTENSITY_MIN,
  glassCssVariables,
  glassIntensityLabel,
  glassSettings,
  normalizeGlassIntensity
} from './glass'

describe('glass intensity normalization', () => {
  it('clamps and rounds user input', () => {
    expect(normalizeGlassIntensity(42)).toBe(42)
    expect(normalizeGlassIntensity(41.6)).toBe(42)
    expect(normalizeGlassIntensity(-20)).toBe(GLASS_INTENSITY_MIN)
    expect(normalizeGlassIntensity(250)).toBe(GLASS_INTENSITY_MAX)
  })

  it('falls back to the default for junk', () => {
    expect(normalizeGlassIntensity(Number.NaN)).toBe(GLASS_INTENSITY_DEFAULT)
    expect(normalizeGlassIntensity('80')).toBe(GLASS_INTENSITY_DEFAULT)
    expect(normalizeGlassIntensity(null)).toBe(GLASS_INTENSITY_DEFAULT)
  })

  it('has a sane default inside the range', () => {
    expect(GLASS_INTENSITY_DEFAULT).toBeGreaterThanOrEqual(GLASS_INTENSITY_MIN)
    expect(GLASS_INTENSITY_DEFAULT).toBeLessThanOrEqual(GLASS_INTENSITY_MAX)
  })
})

describe('glass settings mapping', () => {
  it('gets more opaque, blurrier and less wallpaper-forward as intensity rises', () => {
    const clear = glassSettings(0, 'dark')
    const frosted = glassSettings(100, 'dark')
    expect(frosted.alpha).toBeGreaterThan(clear.alpha)
    expect(frosted.blur).toBeGreaterThan(clear.blur)
    expect(frosted.softBlur).toBeGreaterThan(clear.softBlur)
    expect(frosted.wallpaperBlur).toBeGreaterThan(clear.wallpaperBlur)
    expect(frosted.wallpaperOpacity).toBeLessThan(clear.wallpaperOpacity)
    expect(frosted.frostOpacity).toBeGreaterThan(clear.frostOpacity)
  })

  it('never exceeds full opacity and stays above the old default', () => {
    for (const theme of ['dark', 'light'] as const) {
      for (const level of [0, 25, 50, 75, 100]) {
        const { alpha } = glassSettings(level, theme)
        expect(alpha).toBeGreaterThan(0.8)
        expect(alpha).toBeLessThanOrEqual(1)
      }
      // The shipped panel used rgba(…, 0.92); the new default must be frostier.
      expect(glassSettings(GLASS_INTENSITY_DEFAULT, theme).alpha).toBeGreaterThan(0.94)
    }
  })

  it('keeps the light theme slightly more translucent than dark at the same level', () => {
    expect(glassSettings(60, 'light').alpha).toBeLessThan(glassSettings(60, 'dark').alpha)
  })
})

describe('glass css variables', () => {
  it('exports px-typed radii and numeric opacities', () => {
    const vars = glassCssVariables(GLASS_INTENSITY_DEFAULT, 'dark')
    expect(vars['--fc-glass-blur']).toMatch(/^\d+px$/)
    expect(vars['--fc-glass-blur-soft']).toMatch(/^\d+px$/)
    expect(vars['--fc-wallpaper-blur']).toMatch(/^\d+px$/)
    expect(Number(vars['--fc-window-alpha'])).toBeGreaterThan(0.9)
    expect(Number(vars['--fc-wallpaper-opacity'])).toBeLessThan(0.5)
    expect(Number(vars['--fc-frost-opacity'])).toBeLessThan(0.1)
  })
})

describe('glass labels', () => {
  it('names the four buckets', () => {
    expect(glassIntensityLabel(0)).toBe('通透')
    expect(glassIntensityLabel(35)).toBe('轻柔')
    expect(glassIntensityLabel(80)).toBe('标准')
    expect(glassIntensityLabel(100)).toBe('浓郁')
  })
})
