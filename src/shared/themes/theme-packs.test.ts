import { describe, expect, it } from 'vitest'
import { BUILTIN_THEMES, BUILTIN_THEME_IDS, getBuiltinTheme } from './index'
import {
  BLUE_GLASS_THEME,
  THEME_TOKEN_CSS,
  contrastRatio,
  supportedThemeModes,
  validateThemeDefinition,
  type ThemeMode,
  type ThemePalette,
  type ThemeTokenKey
} from '../theme'

const EXPECTED_THEME_IDS = [
  'blue-glass',
  'dracula',
  'tokyo-night',
  'kanagawa',
  'atelier',
  'everforest'
]

const EXPECTED_MODES: Record<string, ThemeMode[]> = {
  'blue-glass': ['dark', 'light'],
  dracula: ['dark'],
  'tokyo-night': ['dark'],
  kanagawa: ['dark', 'light'],
  atelier: ['dark', 'light'],
  everforest: ['dark', 'light']
}

const EXPECTED_LABELS: Record<string, Partial<Record<ThemeMode, string>>> = {
  'blue-glass': { dark: '深色', light: '浅色' },
  dracula: { dark: 'Dracula' },
  'tokyo-night': { dark: 'Night' },
  kanagawa: { dark: 'Wave', light: 'Lotus' },
  atelier: { dark: 'Ink', light: 'Paper' },
  everforest: { dark: 'Dark', light: 'Light' }
}

const EXPECTED_AUTHORS: Record<string, string> = {
  'blue-glass': 'Flashcast',
  dracula: 'Dracula Theme',
  'tokyo-night': 'enkia',
  kanagawa: 'Tommaso Laurenzi',
  atelier: 'Flashcast',
  everforest: 'Sainnhe Park'
}

const EXPECTED_HOMEPAGES: Record<string, string | undefined> = {
  'blue-glass': undefined,
  dracula: 'https://draculatheme.com',
  'tokyo-night': 'https://github.com/enkia/tokyo-night-vscode-theme',
  kanagawa: 'https://github.com/rebelot/kanagawa.nvim',
  atelier: undefined,
  everforest: 'https://github.com/sainnhe/everforest'
}

/** Spot-checks that the authored packs kept their intended background/accent. */
const EXPECTED_SIGNATURE_COLORS: Record<string, Partial<Record<ThemeMode, [string, string]>>> = {
  dracula: { dark: ['#282a36', '#bd93f9'] },
  'tokyo-night': { dark: ['#1a1b26', '#7aa2f7'] },
  kanagawa: { dark: ['#1f1f28', '#7e9cd8'], light: ['#f2ecbc', '#4d699b'] },
  atelier: { dark: ['#1f1e1d', '#d9a066'], light: ['#f7f3ea', '#8a5a00'] },
  everforest: { dark: ['#2d353b', '#a7c080'], light: ['#fdf6e3', '#5f6b00'] }
}

const EXPECTED_WALLPAPER: Record<
  string,
  Partial<Record<ThemeMode, { opacity: number; saturate: number }>>
> = {
  dracula: { dark: { opacity: 0.18, saturate: 0.85 } },
  'tokyo-night': { dark: { opacity: 0.22, saturate: 1.0 } },
  kanagawa: {
    dark: { opacity: 0.2, saturate: 0.9 },
    light: { opacity: 0.14, saturate: 0.8 }
  },
  atelier: {
    light: { opacity: 0.14, saturate: 0.9 },
    dark: { opacity: 0.18, saturate: 0.95 }
  },
  everforest: {
    light: { opacity: 0.14, saturate: 0.8 },
    dark: { opacity: 0.18, saturate: 0.9 }
  }
}

const TOKEN_KEYS = Object.keys(THEME_TOKEN_CSS).sort() as ThemeTokenKey[]

type ContrastPaletteKey =
  | 'text'
  | 'textSecondary'
  | 'textTertiary'
  | 'accent'
  | 'success'
  | 'warning'
  | 'danger'

const TEXT_CONTRAST_CHECKS: Array<[ContrastPaletteKey, number]> = [
  ['text', 4.5],
  ['textSecondary', 4.5],
  ['textTertiary', 4.5],
  ['accent', 4.5],
  ['success', 4.5],
  ['warning', 4.5],
  ['danger', 4.5]
]

function paletteFor(themeId: string, mode: ThemeMode): ThemePalette {
  const theme = getBuiltinTheme(themeId)
  expect(theme, `missing built-in theme ${themeId}`).toBeDefined()
  const variant = theme!.variants[mode]
  expect(variant, `missing ${mode} variant for ${themeId}`).toBeDefined()
  return variant!.palette
}

describe('built-in theme catalog', () => {
  it('ships six uniquely identified themes with Blue Glass first', () => {
    expect(BUILTIN_THEMES).toHaveLength(6)
    expect(BUILTIN_THEMES.map((theme) => theme.id)).toEqual(EXPECTED_THEME_IDS)
    expect(BUILTIN_THEME_IDS).toEqual(EXPECTED_THEME_IDS)
    expect(new Set(BUILTIN_THEME_IDS).size).toBe(BUILTIN_THEMES.length)
    expect(BUILTIN_THEMES[0]).toBe(BLUE_GLASS_THEME)
    expect(BUILTIN_THEMES[0].id).toBe('blue-glass')
  })

  it('looks themes up by id', () => {
    for (const theme of BUILTIN_THEMES) {
      expect(getBuiltinTheme(theme.id)).toBe(theme)
    }
    expect(getBuiltinTheme('missing-theme')).toBeUndefined()
    expect(getBuiltinTheme(undefined)).toBeUndefined()
  })

  it('declares the expected light/dark support matrix', () => {
    for (const theme of BUILTIN_THEMES) {
      expect(supportedThemeModes(theme)).toEqual(EXPECTED_MODES[theme.id])
      expect(Object.keys(theme.variants).sort()).toEqual(
        [...EXPECTED_MODES[theme.id]].sort()
      )
      for (const [mode, label] of Object.entries(EXPECTED_LABELS[theme.id])) {
        expect(theme.variants[mode as ThemeMode]?.label).toBe(label)
      }
    }
  })

  it('keeps Dracula and Tokyo Night dark-only and the rest dual-mode', () => {
    expect(supportedThemeModes(getBuiltinTheme('dracula'))).toEqual(['dark'])
    expect(supportedThemeModes(getBuiltinTheme('tokyo-night'))).toEqual(['dark'])
    expect(supportedThemeModes(getBuiltinTheme('blue-glass'))).toEqual(['dark', 'light'])
    expect(supportedThemeModes(getBuiltinTheme('kanagawa'))).toEqual(['dark', 'light'])
    expect(supportedThemeModes(getBuiltinTheme('atelier'))).toEqual(['dark', 'light'])
    expect(supportedThemeModes(getBuiltinTheme('everforest'))).toEqual(['dark', 'light'])
  })

  it('bundles only Blue Glass and generates the rest from tokens', () => {
    expect(BLUE_GLASS_THEME.application).toBe('bundled-css')
    for (const theme of BUILTIN_THEMES) {
      expect(theme.application).toBe(
        theme.id === 'blue-glass' ? 'bundled-css' : 'generated-tokens'
      )
      expect(theme.source).toBe('builtin')
    }
  })

  it('carries complete metadata with Chinese descriptions', () => {
    for (const theme of BUILTIN_THEMES) {
      expect(theme.schemaVersion).toBe(1)
      expect(theme.id).toMatch(/^[a-z0-9][a-z0-9._-]{1,63}$/)
      expect(theme.name.trim().length).toBeGreaterThan(0)
      expect(theme.version).toBe('1.0.0')
      expect(theme.author).toBe(EXPECTED_AUTHORS[theme.id])
      expect(theme.license).toBe('MIT')
      expect(theme.homepage).toBe(EXPECTED_HOMEPAGES[theme.id])
      expect(theme.description.trim().length).toBeGreaterThan(0)
      // Blue Glass ships its description from theme.ts; the authored packs are Chinese.
      if (theme.id !== 'blue-glass') {
        expect(theme.description).toMatch(/[\u3400-\u9fff]/)
      }
      // Atelier is the shipped name; "Claude" stays out of the catalog.
      expect(`${theme.name} ${theme.description}`.toLowerCase()).not.toContain('claude')
    }
  })
})

describe.each(BUILTIN_THEMES.map((theme) => [theme.id, theme] as const))(
  'built-in theme pack: %s',
  (themeId, theme) => {
    it('defines every token key with a non-empty value for each variant', () => {
      const modes = supportedThemeModes(theme)
      expect(modes.length).toBeGreaterThan(0)

      for (const mode of modes) {
        const variant = theme.variants[mode]!
        expect(Object.keys(variant.tokens).sort()).toEqual(TOKEN_KEYS)
        expect(Object.keys(variant.contrastTokens).sort()).toEqual(TOKEN_KEYS)
        expect(variant.label.trim().length).toBeGreaterThan(0)

        for (const key of TOKEN_KEYS) {
          const value = variant.tokens[key]
          expect(typeof value, `${themeId}.${mode}.tokens.${key}`).toBe('string')
          expect(value.trim().length, `${themeId}.${mode}.tokens.${key}`).toBeGreaterThan(0)
        }
        for (const key of TOKEN_KEYS) {
          const value = variant.contrastTokens[key]
          expect(typeof value, `${themeId}.${mode}.contrastTokens.${key}`).toBe('string')
          expect(value.trim().length, `${themeId}.${mode}.contrastTokens.${key}`).toBeGreaterThan(
            0
          )
        }
      }
    })

    it('passes the shared theme validator', () => {
      expect(validateThemeDefinition(theme)).toEqual([])
    })

    it('meets WCAG text and UI contrast requirements', () => {
      for (const mode of supportedThemeModes(theme)) {
        const variant = theme.variants[mode]!
        const palette = variant.palette

        for (const [field, minimum] of TEXT_CONTRAST_CHECKS) {
          const ratio = contrastRatio(palette[field], palette.bg)
          expect(
            ratio,
            `${themeId}.${mode}.${field} vs bg (${palette[field]} on ${palette.bg})`
          ).toBeGreaterThanOrEqual(minimum)
        }

        const focusRatio = contrastRatio(palette.focus, palette.bg)
        expect(
          focusRatio,
          `${themeId}.${mode}.focus vs bg (${palette.focus} on ${palette.bg})`
        ).toBeGreaterThanOrEqual(3)

        const accentFill = palette.accentFill ?? palette.accent
        const onAccentRatio = contrastRatio(palette.onAccent, accentFill)
        expect(
          onAccentRatio,
          `${themeId}.${mode}.onAccent vs accentFill (${palette.onAccent} on ${accentFill})`
        ).toBeGreaterThanOrEqual(4.5)
      }
    })
  }
)

describe('authored theme palettes', () => {
  it('keeps the expected signature background and accent colors', () => {
    for (const [themeId, modes] of Object.entries(EXPECTED_SIGNATURE_COLORS)) {
      for (const [mode, [bg, accent]] of Object.entries(modes)) {
        const palette = paletteFor(themeId, mode as ThemeMode)
        expect(palette.bg, `${themeId}.${mode}.bg`).toBe(bg)
        expect(palette.accent, `${themeId}.${mode}.accent`).toBe(accent)
      }
    }
  })

  it('applies the authored wallpaper tuning', () => {
    for (const [themeId, modes] of Object.entries(EXPECTED_WALLPAPER)) {
      for (const [mode, expected] of Object.entries(modes)) {
        const variant = getBuiltinTheme(themeId)!.variants[mode as ThemeMode]!
        expect(variant.wallpaperOpacity, `${themeId}.${mode}.wallpaperOpacity`).toBe(
          expected.opacity
        )
        expect(variant.wallpaperSaturate, `${themeId}.${mode}.wallpaperSaturate`).toBe(
          expected.saturate
        )
      }
    }
  })
})
