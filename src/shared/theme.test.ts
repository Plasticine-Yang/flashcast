import { describe, expect, it } from 'vitest'
import {
  BLUE_GLASS_THEME,
  THEME_TOKEN_CSS,
  alpha,
  contrastRatio,
  defineTheme,
  mix,
  parseColor,
  parseThemeDefinition,
  relativeLuminance,
  resolvePreferenceMode,
  resolveThemeState,
  validateThemeDefinition,
  type ThemeDefinitionInput,
  type ThemePalette
} from './theme'

function darkPalette(): ThemePalette {
  return {
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
}

function lightPalette(): ThemePalette {
  return {
    bg: '#f4f7fc',
    bgElevated: '#ffffff',
    surface: '#1e4d8f',
    border: '#244a7d',
    text: '#0b1526',
    textSecondary: '#33445f',
    textTertiary: '#44536a',
    accent: '#1c4fae',
    onAccent: '#ffffff',
    success: '#0b5b34',
    warning: '#6b4400',
    danger: '#991b12',
    focus: '#1b4fc4',
    accentFill: '#2563eb',
    accentFillHover: '#1d4fd8'
  }
}

function validInput(): ThemeDefinitionInput {
  return {
    schemaVersion: 1,
    id: 'test-theme',
    name: 'Test Theme',
    description: 'A theme used by unit tests.',
    version: '1.0.0',
    author: 'Flashcast Tests',
    license: 'MIT',
    variants: {
      dark: { label: 'Dark', palette: darkPalette() },
      light: { label: 'Light', palette: lightPalette() }
    }
  }
}

describe('theme color helpers', () => {
  it('parses hex and rgb/rgba colors', () => {
    expect(parseColor('#abc')).toEqual({ r: 170, g: 187, b: 204, a: 1 })
    expect(parseColor('#0a1222')).toEqual({ r: 10, g: 18, b: 34, a: 1 })
    expect(parseColor('#0a122280')).toEqual({ r: 10, g: 18, b: 34, a: 128 / 255 })
    expect(parseColor('rgba(37, 99, 235, 0.5)')).toEqual({ r: 37, g: 99, b: 235, a: 0.5 })
    expect(parseColor('not-a-color')).toBeNull()
    expect(parseColor('#12345')).toBeNull()
  })

  it('applies alpha and mixes colors', () => {
    expect(alpha('#2563eb', 0.2)).toBe('rgba(37, 99, 235, 0.2)')
    expect(mix('#ffffff', '#000000', 0.5)).toBe('#808080')
    expect(mix('#ffffff', '#000000', 0)).toBe('#ffffff')
    expect(mix('#ffffff', '#000000', 1)).toBe('#000000')
  })

  it('computes relative luminance and contrast ratios', () => {
    expect(relativeLuminance('#ffffff')).toBeCloseTo(1, 5)
    expect(relativeLuminance('#000000')).toBeCloseTo(0, 5)
    expect(contrastRatio('#000000', '#ffffff')).toBeCloseTo(21, 5)
    expect(contrastRatio('#ffffff', '#ffffff')).toBeCloseTo(1, 5)
  })
})

describe('theme token builder', () => {
  it('builds every token key for both modes', () => {
    const theme = defineTheme(validInput())
    const expectedKeys = Object.keys(THEME_TOKEN_CSS).sort()

    expect(Object.keys(theme.variants).sort()).toEqual(['dark', 'light'])
    for (const mode of ['dark', 'light'] as const) {
      const variant = theme.variants[mode]
      expect(variant).toBeDefined()
      expect(Object.keys(variant!.tokens).sort()).toEqual(expectedKeys)
      expect(Object.keys(variant!.contrastTokens).sort()).toEqual(expectedKeys)
      for (const value of Object.values(variant!.tokens)) {
        expect(typeof value).toBe('string')
        expect(value.length).toBeGreaterThan(0)
      }
      expect(variant!.wallpaperOpacity).toBe(mode === 'dark' ? 0.24 : 0.2)
      expect(variant!.wallpaperSaturate).toBe(mode === 'dark' ? 1.2 : 1.4)
    }
  })

  it('maps every token onto a unique --fc-* custom property', () => {
    const cssVariables = Object.values(THEME_TOKEN_CSS)
    expect(THEME_TOKEN_CSS.window).toBe('--fc-window')
    expect(THEME_TOKEN_CSS.rowSelectedBg).toBe('--fc-row-selected-bg')
    expect(THEME_TOKEN_CSS.checkerB).toBe('--fc-checker-b')
    expect(cssVariables.every((name) => name.startsWith('--fc-'))).toBe(true)
    expect(new Set(cssVariables).size).toBe(cssVariables.length)
  })

  it('derives mode-specific token values from the palette', () => {
    const theme = defineTheme(validInput())
    const dark = theme.variants.dark!
    const light = theme.variants.light!

    expect(dark.tokens.window).toBe(alpha(darkPalette().bg, 0.964))
    expect(light.tokens.window).toBe(alpha(lightPalette().bg, 0.956))
    expect(dark.tokens.surface1).toBe(alpha(darkPalette().surface, 0.08))
    expect(light.tokens.surface1).toBe(alpha(lightPalette().surface, 0.055))
    expect(dark.tokens.accentStrong).toBe(mix(darkPalette().accent, '#ffffff', 0.25))
    expect(light.tokens.accentStrong).toBe(mix(lightPalette().accent, '#000000', 0.18))
    expect(dark.tokens.accentFill).toBe(darkPalette().accentFill)
    expect(dark.tokens.checkerB).toBe('transparent')
  })

  it('applies overrides after generated values and rejects unknown keys', () => {
    const theme = defineTheme({
      ...validInput(),
      variants: {
        dark: {
          label: 'Dark',
          palette: darkPalette(),
          overrides: { accent: '#ff0000', glassFilter: 'blur(10px)' }
        }
      }
    })
    expect(theme.variants.dark!.tokens.accent).toBe('#ff0000')
    expect(theme.variants.dark!.tokens.glassFilter).toBe('blur(10px)')

    const invalid = {
      ...validInput(),
      variants: {
        dark: {
          label: 'Dark',
          palette: darkPalette(),
          overrides: { notAToken: '#ffffff' }
        }
      }
    } as unknown as ThemeDefinitionInput
    expect(() => defineTheme(invalid)).toThrow(/Unknown theme token override/)
  })

  it('builds high-contrast tokens on top of the normal tokens', () => {
    const theme = defineTheme(validInput())
    const dark = theme.variants.dark!
    const light = theme.variants.light!

    expect(dark.contrastTokens.textSecondary).toBe(darkPalette().text)
    expect(dark.contrastTokens.textTertiary).toBe(darkPalette().text)
    expect(dark.contrastTokens.wallpaperOpacity).toBe('0')
    expect(dark.contrastTokens.window).toBe(alpha(darkPalette().bg, 0.98))
    expect(light.contrastTokens.surface2).toBe(alpha(lightPalette().surface, 0.12))
    expect(dark.contrastTokens.rowSelectedRim).toBe(alpha(darkPalette().accent, 0.55))
  })
})

describe('theme resolution', () => {
  it('resolves system preference against the system mode', () => {
    expect(resolvePreferenceMode('system', 'light')).toBe('light')
    expect(resolvePreferenceMode('system', 'dark')).toBe('dark')
    expect(resolvePreferenceMode('dark', 'light')).toBe('dark')
    expect(resolvePreferenceMode('light', 'dark')).toBe('light')
  })

  it('falls back to blue-glass when the theme is missing', () => {
    expect(resolveThemeState(undefined, 'system', 'light')).toEqual({
      themeId: 'blue-glass',
      preference: 'system',
      mode: 'light',
      locked: false,
      supportedModes: ['dark', 'light']
    })
  })

  it('locks a single-variant theme to its only mode', () => {
    const single = defineTheme({
      ...validInput(),
      variants: { dark: { label: 'Dark', palette: darkPalette() } }
    })

    expect(resolveThemeState(single, 'light', 'light')).toEqual({
      themeId: 'test-theme',
      preference: 'light',
      mode: 'dark',
      locked: true,
      supportedModes: ['dark']
    })
  })

  it('follows system mode for dual-variant themes while keeping explicit preference', () => {
    const theme = defineTheme(validInput())

    expect(resolveThemeState(theme, 'system', 'light').mode).toBe('light')
    expect(resolveThemeState(theme, 'system', 'dark').mode).toBe('dark')
    expect(resolveThemeState(theme, 'dark', 'light').mode).toBe('dark')
    expect(resolveThemeState(theme, 'system', 'dark').locked).toBe(false)
    expect(resolveThemeState(theme, 'system', 'dark').preference).toBe('system')
  })

  it('uses the fallback theme when the requested theme has no variants', () => {
    const empty = { ...defineTheme(validInput()), id: 'empty-theme', variants: {} }
    const fallback = defineTheme({ ...validInput(), id: 'fallback-theme' })

    const state = resolveThemeState(empty, 'light', 'dark', fallback)
    expect(state.themeId).toBe('fallback-theme')
    expect(state.mode).toBe('light')
    expect(state.supportedModes).toEqual(['dark', 'light'])
  })

  it('orders supported modes dark then light regardless of insertion order', () => {
    const theme = defineTheme({
      ...validInput(),
      variants: {
        light: { label: 'Light', palette: lightPalette() },
        dark: { label: 'Dark', palette: darkPalette() }
      }
    })
    expect(resolveThemeState(theme, 'system', 'dark').supportedModes).toEqual(['dark', 'light'])
  })
})

describe('theme parsing and validation', () => {
  it('parses a valid external theme', () => {
    const parsed = parseThemeDefinition(validInput())

    expect(parsed.errors).toEqual([])
    expect(parsed.warnings).toEqual([])
    expect(parsed.definition).toBeDefined()
    expect(parsed.definition!.id).toBe('test-theme')
    expect(parsed.definition!.source).toBe('workspace')
    expect(parsed.definition!.application).toBe('generated-tokens')
    expect(parsed.definition!.variants.dark?.tokens.window).toBe(
      alpha(darkPalette().bg, 0.964)
    )
  })

  it('honours an explicit source for bundled themes', () => {
    const parsed = parseThemeDefinition(validInput(), { source: 'builtin' })
    expect(parsed.errors).toEqual([])
    expect(parsed.definition!.source).toBe('builtin')
  })

  it('rejects invalid ids, schema versions and variant keys', () => {
    const badId = validInput()
    badId.id = 'Blue Glass'
    expect(parseThemeDefinition(badId).errors.some((issue) => issue.path === 'id')).toBe(true)

    const badSchema: Record<string, unknown> = { ...validInput(), schemaVersion: 2 }
    expect(
      parseThemeDefinition(badSchema).errors.some((issue) => issue.path === 'schemaVersion')
    ).toBe(true)

    const badVariant: Record<string, unknown> = {
      ...validInput(),
      variants: { sepia: { label: 'Sepia', palette: darkPalette() } }
    }
    const result = parseThemeDefinition(badVariant)
    expect(result.errors.some((issue) => issue.path === 'variants.sepia')).toBe(true)
    expect(result.errors.some((issue) => issue.path === 'variants')).toBe(true)
  })

  it('rejects unsafe override values', () => {
    const unsafeValues = [
      'red; background: blue',
      'url(https://evil.test/x.png)',
      'var(--fc-accent)',
      'expression(alert(1))',
      '#fff}',
      'data:text/html,<script>alert(1)</script>',
      '/* comment */ red',
      'rotate(10deg)',
      'a'.repeat(513)
    ]

    for (const value of unsafeValues) {
      const input = validInput()
      input.variants.dark = {
        label: 'Dark',
        palette: darkPalette(),
        overrides: { accent: value }
      }
      const parsed = parseThemeDefinition(input)
      expect(parsed.errors.length).toBeGreaterThan(0)
      expect(parsed.definition).toBeUndefined()
    }
  })

  it('accepts safe gradient overrides', () => {
    const input = validInput()
    const gradient = 'linear-gradient(180deg, rgba(37, 99, 235, 0.2), #ffffff)'
    input.variants.dark = {
      label: 'Dark',
      palette: darkPalette(),
      overrides: { rowSelectedBg: gradient }
    }

    const parsed = parseThemeDefinition(input)
    expect(parsed.errors).toEqual([])
    expect(parsed.definition!.variants.dark!.tokens.rowSelectedBg).toBe(gradient)
  })

  it('rejects unparseable palette colors', () => {
    const input = validInput()
    input.variants.dark = {
      label: 'Dark',
      palette: { ...darkPalette(), bg: 'not-a-color' }
    }
    const parsed = parseThemeDefinition(input)
    expect(parsed.errors.some((issue) => issue.path === 'variants.dark.palette.bg')).toBe(true)
    expect(parsed.definition).toBeUndefined()
  })

  it('rejects a final-token override that makes text unreadable', () => {
    const input = validInput()
    input.variants.dark = {
      label: 'Dark',
      palette: darkPalette(),
      overrides: { textPrimary: '#111111' }
    }
    const parsed = parseThemeDefinition(input)
    expect(parsed.definition).toBeUndefined()
    expect(
      parsed.errors.some((issue) => issue.path === 'variants.dark.tokens.textPrimary')
    ).toBe(true)
  })

  it('rejects themes that fail hard contrast checks', () => {
    const input = validInput()
    input.variants.dark = {
      label: 'Dark',
      palette: { ...darkPalette(), text: '#101010' }
    }
    const parsed = parseThemeDefinition(input)
    expect(parsed.definition).toBeUndefined()
    expect(parsed.errors.some((issue) => issue.path === 'variants.dark.palette.text')).toBe(true)

    const badTheme = defineTheme({
      ...validInput(),
      variants: {
        dark: { label: 'Dark', palette: { ...darkPalette(), text: '#101010' } }
      }
    })
    expect(
      validateThemeDefinition(badTheme).some(
        (issue) => issue.path === 'variants.dark.palette.text'
      )
    ).toBe(true)
  })

  it('rejects out-of-range wallpaper settings', () => {
    const input = validInput()
    input.variants.dark = {
      label: 'Dark',
      palette: darkPalette(),
      wallpaperOpacity: 1.2,
      wallpaperSaturate: 3
    }
    const parsed = parseThemeDefinition(input)
    expect(
      parsed.errors.some((issue) => issue.path === 'variants.dark.wallpaperOpacity')
    ).toBe(true)
    expect(
      parsed.errors.some((issue) => issue.path === 'variants.dark.wallpaperSaturate')
    ).toBe(true)
  })

  it('ships a valid Blue Glass default theme', () => {
    expect(BLUE_GLASS_THEME.id).toBe('blue-glass')
    expect(BLUE_GLASS_THEME.source).toBe('builtin')
    expect(BLUE_GLASS_THEME.application).toBe('bundled-css')
    expect(Object.keys(BLUE_GLASS_THEME.variants).sort()).toEqual(['dark', 'light'])
    expect(validateThemeDefinition(BLUE_GLASS_THEME)).toEqual([])
  })
})
