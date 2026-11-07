/* ------------------------------------------------------------------ */
/* Theme domain contract                                              */
/*                                                                    */
/* A theme is a pair of fully-built token sets (one per mode) plus the */
/* palette metadata they were generated from. Bundled themes keep      */
/* their CSS in the app; generated themes are serialized as JSON and   */
/* turned into CSS custom properties at runtime.                       */
/* ------------------------------------------------------------------ */

export type ThemeMode = 'dark' | 'light'
export type ThemePreference = 'system' | ThemeMode
export type ThemeSource = 'builtin' | 'workspace'
export type ThemeApplication = 'bundled-css' | 'generated-tokens'

export const DEFAULT_THEME_ID = 'blue-glass'

export interface ThemePalette {
  bg: string
  bgElevated: string
  surface: string
  border: string
  text: string
  textSecondary: string
  textTertiary: string
  accent: string
  onAccent: string
  success: string
  warning: string
  danger: string
  focus: string
  accentFill?: string
  accentFillHover?: string
}

/**
 * Token names are deliberately camelCase in TypeScript: they are the stable
 * theme-file key. `THEME_TOKEN_CSS` maps them onto the `--fc-*` custom
 * properties consumed by the renderer.
 */
export const THEME_TOKEN_CSS = {
  window: '--fc-window',
  windowSolid: '--fc-window-solid',
  glassOpaque: '--fc-glass-opaque',
  frostOpacity: '--fc-frost-opacity',
  wallpaperOpacity: '--fc-wallpaper-opacity',
  wallpaperBlur: '--fc-wallpaper-blur',
  wallpaperSaturate: '--fc-wallpaper-saturate',
  glassSheen: '--fc-glass-sheen',
  surface1: '--fc-surface-1',
  surface2: '--fc-surface-2',
  surfaceHover: '--fc-surface-hover',
  surfaceSelected: '--fc-surface-selected',
  surfaceSelectedStrong: '--fc-surface-selected-strong',
  borderSubtle: '--fc-border-subtle',
  borderStrong: '--fc-border-strong',
  rowSelectedBg: '--fc-row-selected-bg',
  rowSelectedRim: '--fc-row-selected-rim',
  rowSelectedGlow: '--fc-row-selected-glow',
  glassBorder: '--fc-glass-border',
  glassRimTop: '--fc-glass-rim-top',
  glassRimBottom: '--fc-glass-rim-bottom',
  glassFilter: '--fc-glass-filter',
  glassFilterSoft: '--fc-glass-filter-soft',
  scrim: '--fc-scrim',
  previewBg: '--fc-preview-bg',
  previewBorder: '--fc-preview-border',
  textPrimary: '--fc-text-primary',
  textSecondary: '--fc-text-secondary',
  textTertiary: '--fc-text-tertiary',
  accent: '--fc-accent',
  accentStrong: '--fc-accent-strong',
  accentSoft: '--fc-accent-soft',
  accentLine: '--fc-accent-line',
  accentFill: '--fc-accent-fill',
  accentFillHover: '--fc-accent-fill-hover',
  accentFillDeep: '--fc-accent-fill-deep',
  accentFillHoverDeep: '--fc-accent-fill-hover-deep',
  onAccent: '--fc-on-accent',
  success: '--fc-success',
  successSoft: '--fc-success-soft',
  warning: '--fc-warning',
  warningSoft: '--fc-warning-soft',
  danger: '--fc-danger',
  dangerSoft: '--fc-danger-soft',
  focusRing: '--fc-focus-ring',
  shadowWindow: '--fc-shadow-window',
  shadowPopover: '--fc-shadow-popover',
  shadowAccent: '--fc-shadow-accent',
  highlightLine: '--fc-highlight-line',
  aurora: '--fc-aurora',
  scrollbarThumb: '--fc-scrollbar-thumb',
  scrollbarThumbHover: '--fc-scrollbar-thumb-hover',
  keycapHighlight: '--fc-keycap-highlight',
  checkerA: '--fc-checker-a',
  checkerB: '--fc-checker-b'
} as const

export type ThemeTokenKey = keyof typeof THEME_TOKEN_CSS
export type ThemeTokens = Record<ThemeTokenKey, string>

export interface ThemeVariant {
  label: string
  palette: ThemePalette
  tokens: ThemeTokens
  contrastTokens: ThemeTokens
  wallpaperOpacity: number
  wallpaperSaturate: number
}

export interface ThemeDefinition {
  schemaVersion: 1
  id: string
  name: string
  description: string
  version: string
  author: string
  license: string
  homepage?: string
  source: ThemeSource
  application: ThemeApplication
  variants: Partial<Record<ThemeMode, ThemeVariant>>
}

export interface ThemeVariantInput {
  label: string
  palette: ThemePalette
  overrides?: Partial<ThemeTokens>
  wallpaperOpacity?: number
  wallpaperSaturate?: number
}

export interface ThemeDefinitionInput {
  schemaVersion: 1
  id: string
  name: string
  description: string
  version: string
  author: string
  license: string
  homepage?: string
  application?: ThemeApplication
  variants: Partial<Record<ThemeMode, ThemeVariantInput>>
}

/* ------------------------------------------------------------------ */
/* Color helpers                                                      */
/* ------------------------------------------------------------------ */

export interface ThemeColor {
  r: number
  g: number
  b: number
  a: number
}

export function parseColor(input: string): ThemeColor | null {
  if (typeof input !== 'string') return null
  const value = input.trim().toLowerCase()
  if (value.length === 0) return null

  if (value.startsWith('#')) {
    const hex = value.slice(1)
    if (!/^[0-9a-f]+$/.test(hex)) return null
    const pair = (chunk: string): number => parseInt(chunk.length === 1 ? chunk + chunk : chunk, 16)
    if (hex.length === 3) {
      return { r: pair(hex[0]), g: pair(hex[1]), b: pair(hex[2]), a: 1 }
    }
    if (hex.length === 6 || hex.length === 8) {
      return {
        r: pair(hex.slice(0, 2)),
        g: pair(hex.slice(2, 4)),
        b: pair(hex.slice(4, 6)),
        a: hex.length === 8 ? pair(hex.slice(6, 8)) / 255 : 1
      }
    }
    return null
  }

  const fn = /^(rgba?)\(([^()]*)\)$/.exec(value)
  if (!fn) return null
  const parts = fn[2]
    .replace(/\//g, ' ')
    .split(/[\s,]+/)
    .filter(Boolean)
  const expectedParts = fn[1] === 'rgba' ? 4 : 3
  if (parts.length !== expectedParts) return null
  const [r, g, b] = parts.slice(0, 3).map(Number)
  const a = fn[1] === 'rgba' ? Number(parts[3]) : 1
  if (![r, g, b, a].every(Number.isFinite)) return null
  if ([r, g, b].some((channel) => channel < 0 || channel > 255)) return null
  if (a < 0 || a > 1) return null
  return { r: Math.round(r), g: Math.round(g), b: Math.round(b), a }
}

function toHex(r: number, g: number, b: number): string {
  return `#${[r, g, b].map((channel) => channel.toString(16).padStart(2, '0')).join('')}`
}

function formatAlpha(value: number): string {
  return String(Number(Math.min(1, Math.max(0, value)).toFixed(4)))
}

/** `rgba()` with the given alpha, replacing any alpha already on the color. */
export function alpha(color: string, opacity: number): string {
  const parsed = parseColor(color)
  if (!parsed) throw new Error(`Unsupported color: ${color}`)
  return `rgba(${parsed.r}, ${parsed.g}, ${parsed.b}, ${formatAlpha(opacity)})`
}

/** sRGB channel interpolation. `weight = 0` returns `from`, `1` returns `to`. */
export function mix(from: string, to: string, weight: number): string {
  const a = parseColor(from)
  const b = parseColor(to)
  if (!a || !b) throw new Error(`Unsupported color mix: ${from} / ${to}`)
  const t = Math.min(1, Math.max(0, weight))
  const channel = (left: number, right: number): number => Math.round(left * (1 - t) + right * t)
  const opacity = a.a * (1 - t) + b.a * t
  const r = channel(a.r, b.r)
  const g = channel(a.g, b.g)
  const blue = channel(a.b, b.b)
  return opacity >= 1 ? toHex(r, g, blue) : `rgba(${r}, ${g}, ${blue}, ${formatAlpha(opacity)})`
}

/** WCAG relative luminance; alpha is ignored (tokens are composites). */
export function relativeLuminance(color: string): number {
  const parsed = parseColor(color)
  if (!parsed) throw new Error(`Unsupported color: ${color}`)
  const linear = (channel: number): number => {
    const c = channel / 255
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
  }
  return 0.2126 * linear(parsed.r) + 0.7152 * linear(parsed.g) + 0.0722 * linear(parsed.b)
}

export function contrastRatio(foreground: string, background: string): number {
  const foregroundLuminance = relativeLuminance(foreground)
  const backgroundLuminance = relativeLuminance(background)
  const lighter = Math.max(foregroundLuminance, backgroundLuminance)
  const darker = Math.min(foregroundLuminance, backgroundLuminance)
  return (lighter + 0.05) / (darker + 0.05)
}

/* ------------------------------------------------------------------ */
/* Token building                                                     */
/* ------------------------------------------------------------------ */

export function isThemeTokenKey(value: string): value is ThemeTokenKey {
  return Object.prototype.hasOwnProperty.call(THEME_TOKEN_CSS, value)
}

function applyTokenOverrides(
  base: ThemeTokens,
  overrides: Partial<ThemeTokens> | undefined
): ThemeTokens {
  if (!overrides) return base
  const next: ThemeTokens = { ...base }
  for (const [key, value] of Object.entries(overrides)) {
    if (!isThemeTokenKey(key)) throw new Error(`Unknown theme token override: ${key}`)
    if (typeof value !== 'string' || value.trim().length === 0) {
      throw new Error(`Theme token override "${key}" must be a non-empty string`)
    }
    next[key] = value
  }
  return next
}

function buildContrastTokens(mode: ThemeMode, tokens: ThemeTokens, palette: ThemePalette): ThemeTokens {
  const dark = mode === 'dark'
  return {
    ...tokens,
    window: alpha(palette.bg, 0.98),
    glassOpaque: alpha(palette.bgElevated, 0.99),
    surface1: alpha(palette.surface, dark ? 0.14 : 0.08),
    surface2: alpha(palette.surface, dark ? 0.2 : 0.12),
    surfaceSelected: alpha(palette.accent, dark ? 0.3 : 0.16),
    surfaceSelectedStrong: alpha(palette.accent, dark ? 0.42 : 0.24),
    borderSubtle: alpha(palette.border, dark ? 0.32 : 0.24),
    borderStrong: alpha(palette.border, dark ? 0.48 : 0.36),
    textSecondary: palette.text,
    textTertiary: palette.text,
    wallpaperOpacity: '0',
    rowSelectedBg: dark
      ? `linear-gradient(180deg, ${alpha(palette.accent, 0.35)}, ${alpha(palette.bgElevated, 0.85)})`
      : `linear-gradient(180deg, ${mix('#ffffff', palette.accent, 0.12)}, ${mix('#ffffff', palette.accent, 0.22)})`,
    rowSelectedRim: alpha(palette.accent, dark ? 0.55 : 0.32)
  }
}

export function buildThemeVariant(mode: ThemeMode, input: ThemeVariantInput): ThemeVariant {
  const dark = mode === 'dark'
  const palette = input.palette
  const wallpaperOpacity = input.wallpaperOpacity ?? (dark ? 0.24 : 0.2)
  const wallpaperSaturate = input.wallpaperSaturate ?? (dark ? 1.2 : 1.4)
  const accentFill = palette.accentFill ?? palette.accent
  const accentFillHover = palette.accentFillHover ?? mix(accentFill, '#000000', 0.1)

  const generated: ThemeTokens = {
    window: alpha(palette.bg, dark ? 0.964 : 0.956),
    windowSolid: palette.bg,
    glassOpaque: alpha(palette.bgElevated, 0.97),
    frostOpacity: dark ? '0.066' : '0.046',
    wallpaperOpacity: String(wallpaperOpacity),
    wallpaperBlur: '112px',
    wallpaperSaturate: String(wallpaperSaturate),
    glassSheen: `radial-gradient(120% 130% at 6% -24%, ${alpha(palette.accent, 0.2)}, transparent 58%), radial-gradient(90% 110% at 104% 112%, ${alpha(palette.accent, 0.12)}, transparent 62%)`,
    surface1: alpha(palette.surface, dark ? 0.08 : 0.055),
    surface2: alpha(palette.surface, dark ? 0.13 : 0.09),
    surfaceHover: alpha(palette.surface, dark ? 0.1 : 0.07),
    surfaceSelected: alpha(palette.accent, dark ? 0.2 : 0.1),
    surfaceSelectedStrong: alpha(palette.accent, dark ? 0.3 : 0.16),
    borderSubtle: alpha(palette.border, dark ? 0.16 : 0.14),
    borderStrong: alpha(palette.border, dark ? 0.3 : 0.24),
    rowSelectedBg: dark
      ? `linear-gradient(180deg, ${alpha(palette.accent, 0.22)}, ${alpha(palette.bgElevated, 0.62)})`
      : `linear-gradient(180deg, ${mix('#ffffff', palette.accent, 0.06)}, ${mix('#ffffff', palette.accent, 0.12)})`,
    rowSelectedRim: alpha(palette.accent, dark ? 0.4 : 0.22),
    rowSelectedGlow: dark
      ? `0 8px 24px -14px ${alpha(palette.accent, 0.55)}`
      : `0 8px 22px -16px ${alpha(palette.accent, 0.38)}`,
    glassBorder: dark ? alpha(palette.border, 0.22) : 'rgba(255,255,255,0.9)',
    glassRimTop: dark ? 'rgba(255,255,255,0.26)' : 'rgba(255,255,255,0.95)',
    glassRimBottom: alpha(palette.accent, dark ? 0.08 : 0.25),
    glassFilter: dark
      ? 'blur(83px) saturate(1.6) brightness(0.68)'
      : 'blur(83px) saturate(1.5) brightness(1.03)',
    glassFilterSoft: dark
      ? 'blur(50px) saturate(1.5) brightness(0.84)'
      : 'blur(50px) saturate(1.4) brightness(1.02)',
    scrim: dark ? alpha(palette.bg, 0.58) : 'rgba(255,255,255,0.60)',
    previewBg: dark
      ? alpha(palette.bgElevated, 0.95)
      : `linear-gradient(180deg, rgba(255,255,255,0.97), ${alpha(palette.accent, 0.05)})`,
    previewBorder: alpha(palette.border, dark ? 0.26 : 0.18),
    textPrimary: palette.text,
    textSecondary: palette.textSecondary,
    textTertiary: palette.textTertiary,
    accent: palette.accent,
    accentStrong: dark
      ? mix(palette.accent, '#ffffff', 0.25)
      : mix(palette.accent, '#000000', 0.18),
    accentSoft: alpha(palette.accent, dark ? 0.18 : 0.1),
    accentLine: alpha(palette.accent, dark ? 0.5 : 0.45),
    accentFill,
    accentFillHover,
    accentFillDeep: mix(accentFill, '#000000', 0.24),
    accentFillHoverDeep: mix(accentFillHover, '#000000', 0.2),
    onAccent: palette.onAccent,
    success: palette.success,
    successSoft: alpha(palette.success, dark ? 0.14 : 0.09),
    warning: palette.warning,
    warningSoft: alpha(palette.warning, dark ? 0.14 : 0.09),
    danger: palette.danger,
    dangerSoft: alpha(palette.danger, dark ? 0.14 : 0.09),
    focusRing: palette.focus,
    shadowWindow: dark
      ? `0 32px 90px ${alpha(palette.bg, 0.62)}, 0 6px 24px ${alpha(palette.bg, 0.36)}, 0 0 80px -24px ${alpha(palette.accent, 0.55)}, 0 0 0 1px ${alpha(palette.accent, 0.06)}`
      : `0 30px 84px ${alpha(palette.border, 0.18)}, 0 6px 20px ${alpha(palette.border, 0.08)}, 0 0 70px -28px ${alpha(palette.accent, 0.35)}, 0 0 0 1px rgba(255,255,255,0.5)`,
    shadowPopover: dark
      ? `0 22px 64px ${alpha(palette.bg, 0.58)}, 0 2px 10px ${alpha(palette.bg, 0.32)}`
      : `0 22px 60px ${alpha(palette.border, 0.16)}, 0 2px 10px ${alpha(palette.border, 0.08)}`,
    shadowAccent: `0 10px 26px -10px ${alpha(palette.accent, 0.65)}`,
    highlightLine: dark
      ? 'inset 0 1px 0 rgba(255,255,255,0.16)'
      : 'inset 0 1px 0 rgba(255,255,255,0.9)',
    aurora: dark
      ? `radial-gradient(1100px 720px at 4% -14%, ${alpha(palette.accent, 0.32)}, transparent 62%), radial-gradient(900px 640px at 104% -8%, ${alpha(palette.accent, 0.16)}, transparent 58%), radial-gradient(1040px 720px at 82% 116%, ${alpha(palette.accent, 0.2)}, transparent 64%), linear-gradient(158deg, ${palette.bg} 0%, ${mix(palette.bg, '#000000', 0.16)} 52%, ${mix(palette.bg, '#000000', 0.3)} 100%)`
      : `radial-gradient(1100px 720px at 4% -14%, ${alpha(palette.accent, 0.16)}, transparent 62%), radial-gradient(900px 640px at 104% -8%, ${alpha(palette.accent, 0.1)}, transparent 58%), radial-gradient(1040px 720px at 82% 116%, ${alpha(palette.accent, 0.14)}, transparent 64%), linear-gradient(158deg, ${palette.bg} 0%, ${mix(palette.bg, palette.accent, 0.05)} 52%, ${mix(palette.bg, palette.accent, 0.09)} 100%)`,
    scrollbarThumb: dark ? alpha(palette.border, 0.24) : alpha(palette.accent, 0.22),
    scrollbarThumbHover: dark ? alpha(palette.border, 0.34) : alpha(palette.accent, 0.32),
    keycapHighlight: dark ? 'rgba(255,255,255,0.08)' : 'rgba(255,255,255,0.85)',
    checkerA: dark ? alpha(palette.text, 0.05) : alpha(palette.accent, 0.06),
    checkerB: 'transparent'
  }

  const tokens = applyTokenOverrides(generated, input.overrides)
  return {
    label: input.label,
    palette,
    tokens,
    contrastTokens: buildContrastTokens(mode, tokens, palette),
    wallpaperOpacity,
    wallpaperSaturate
  }
}

export function defineTheme(
  input: ThemeDefinitionInput,
  source: ThemeSource = 'builtin'
): ThemeDefinition {
  const variants: Partial<Record<ThemeMode, ThemeVariant>> = {}
  if (input.variants.dark) variants.dark = buildThemeVariant('dark', input.variants.dark)
  if (input.variants.light) variants.light = buildThemeVariant('light', input.variants.light)
  return {
    schemaVersion: 1,
    id: input.id,
    name: input.name,
    description: input.description,
    version: input.version,
    author: input.author,
    license: input.license,
    ...(input.homepage !== undefined ? { homepage: input.homepage } : {}),
    source,
    application: input.application ?? 'generated-tokens',
    variants
  }
}

/* ------------------------------------------------------------------ */
/* Blue Glass: the bundled default theme                              */
/* ------------------------------------------------------------------ */

const BLUE_GLASS_DARK_PALETTE: ThemePalette = {
  bg: '#0a1222',
  bgElevated: '#101c32',
  surface: '#8db4ff',
  border: '#93beff',
  text: '#f2f6ff',
  textSecondary: '#c2cfe6',
  textTertiary: '#b2c0d9',
  accent: '#9ac4ff',
  onAccent: '#ffffff',
  success: '#4ade80',
  warning: '#fbbf24',
  danger: '#ff9e97',
  focus: '#9ac4ff',
  accentFill: '#2563eb',
  accentFillHover: '#1d4fd8'
}

const BLUE_GLASS_LIGHT_PALETTE: ThemePalette = {
  bg: '#f2f7ff',
  bgElevated: '#fafcff',
  surface: '#183c78',
  border: '#1c3f78',
  text: '#0d1a2f',
  textSecondary: '#2f4260',
  textTertiary: '#43536f',
  accent: '#1a4db8',
  onAccent: '#ffffff',
  success: '#0b5b34',
  warning: '#704700',
  danger: '#9f1d14',
  focus: '#1b4fc4',
  accentFill: '#2563eb',
  accentFillHover: '#1d4fd8'
}

/**
 * The default theme ships with the app's CSS bundle; its generated tokens are
 * metadata only (used for previews and for themes derived from it), not the
 * source of truth for Blue Glass rendering.
 */
export const BLUE_GLASS_THEME: ThemeDefinition = defineTheme(
  {
    schemaVersion: 1,
    id: DEFAULT_THEME_ID,
    name: 'Blue Glass',
    description: '冷蓝玻璃，Flashcast 默认主题。',
    version: '1.0.0',
    author: 'Flashcast',
    license: 'MIT',
    application: 'bundled-css',
    variants: {
      dark: { label: '深色', palette: BLUE_GLASS_DARK_PALETTE },
      light: { label: '浅色', palette: BLUE_GLASS_LIGHT_PALETTE }
    }
  },
  'builtin'
)

/* ------------------------------------------------------------------ */
/* Resolution                                                          */
/* ------------------------------------------------------------------ */

export interface ResolvedThemeState {
  themeId: string
  preference: ThemePreference
  mode: ThemeMode
  locked: boolean
  supportedModes: ThemeMode[]
}

export function supportedThemeModes(theme: ThemeDefinition | undefined): ThemeMode[] {
  if (!theme) return []
  const modes: ThemeMode[] = []
  if (theme.variants.dark) modes.push('dark')
  if (theme.variants.light) modes.push('light')
  return modes
}

export function resolvePreferenceMode(
  preference: ThemePreference,
  systemMode: ThemeMode
): ThemeMode {
  return preference === 'system' ? systemMode : preference
}

/**
 * Resolve the effective mode for a theme.
 *
 * - A missing/empty theme falls back to `fallback` (Blue Glass by default).
 * - A single-variant theme locks to that variant.
 * - A dual-variant theme follows the OS in `system` mode, otherwise the
 *   explicit preference.
 *
 * `preference` is never mutated; callers keep the user's original choice.
 */
export function resolveThemeState(
  theme: ThemeDefinition | undefined,
  preference: ThemePreference,
  systemMode: ThemeMode,
  fallback: ThemeDefinition = BLUE_GLASS_THEME
): ResolvedThemeState {
  const preferredMode = resolvePreferenceMode(preference, systemMode)
  const candidate =
    theme && supportedThemeModes(theme).length > 0 ? theme : fallback
  const supportedModes = supportedThemeModes(candidate)

  if (supportedModes.length === 0) {
    return {
      themeId: candidate?.id ?? DEFAULT_THEME_ID,
      preference,
      mode: preferredMode,
      locked: false,
      supportedModes
    }
  }

  if (supportedModes.length === 1) {
    return {
      themeId: candidate.id,
      preference,
      mode: supportedModes[0],
      locked: true,
      supportedModes
    }
  }

  return {
    themeId: candidate.id,
    preference,
    mode: supportedModes.includes(preferredMode) ? preferredMode : supportedModes[0],
    locked: false,
    supportedModes
  }
}

/* ------------------------------------------------------------------ */
/* External theme parsing / validation                                */
/* ------------------------------------------------------------------ */

export interface ThemeValidationIssue {
  path: string
  message: string
}

export interface ParsedTheme {
  definition?: ThemeDefinition
  errors: ThemeValidationIssue[]
  warnings: ThemeValidationIssue[]
}

const THEME_ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,63}$/
const MAX_TOKEN_VALUE_LENGTH = 512

const REQUIRED_PALETTE_KEYS = [
  'bg',
  'bgElevated',
  'surface',
  'border',
  'text',
  'textSecondary',
  'textTertiary',
  'accent',
  'onAccent',
  'success',
  'warning',
  'danger',
  'focus'
] as const

const OPTIONAL_PALETTE_KEYS = ['accentFill', 'accentFillHover'] as const

const BLOCKED_VALUE_FRAGMENTS = [
  '{',
  '}',
  ';',
  '@',
  'url(',
  'var(',
  'expression(',
  'javascript:',
  'data:',
  '/*',
  '*/',
  '<',
  '>'
] as const

const ALLOWED_CSS_FUNCTIONS = new Set([
  'rgb',
  'rgba',
  'hsl',
  'hsla',
  'color-mix',
  'linear-gradient',
  'radial-gradient',
  'blur',
  'saturate',
  'brightness',
  'inset',
  'drop-shadow'
])

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function readStringField(
  record: Record<string, unknown>,
  key: string,
  path: string,
  errors: ThemeValidationIssue[],
  options: { required?: boolean; allowEmpty?: boolean } = {}
): string | undefined {
  const { required = true, allowEmpty = false } = options
  const value = record[key]
  if (value === undefined) {
    if (required) errors.push({ path, message: 'is required' })
    return undefined
  }
  if (typeof value !== 'string') {
    errors.push({ path, message: 'must be a string' })
    return undefined
  }
  if (!allowEmpty && value.trim().length === 0) {
    errors.push({ path, message: 'must be a non-empty string' })
    return undefined
  }
  return value
}

function unsafeValueReason(value: string): string | null {
  if (value.trim().length === 0) return 'must not be empty'
  if (value.length > MAX_TOKEN_VALUE_LENGTH) {
    return `must be at most ${MAX_TOKEN_VALUE_LENGTH} characters`
  }
  const lower = value.toLowerCase()
  for (const fragment of BLOCKED_VALUE_FRAGMENTS) {
    if (lower.includes(fragment)) return `must not contain "${fragment}"`
  }
  for (const match of value.matchAll(/([a-zA-Z-]+)\s*\(/g)) {
    const name = match[1].toLowerCase()
    if (!ALLOWED_CSS_FUNCTIONS.has(name)) {
      return `must not use the unsupported CSS function "${name}()"`
    }
  }
  return null
}

function parseOptionalNumber(
  record: Record<string, unknown>,
  key: string,
  path: string,
  errors: ThemeValidationIssue[],
  min: number,
  max: number
): number | undefined {
  const value = record[key]
  if (value === undefined) return undefined
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    errors.push({ path, message: 'must be a finite number' })
    return undefined
  }
  if (value < min || value > max) {
    errors.push({ path, message: `must be between ${min} and ${max}` })
    return undefined
  }
  return value
}

function parsePalette(
  raw: unknown,
  path: string,
  errors: ThemeValidationIssue[]
): ThemePalette | undefined {
  if (!isRecord(raw)) {
    errors.push({ path, message: 'must be an object' })
    return undefined
  }
  const before = errors.length
  const palette: Partial<ThemePalette> = {}

  for (const key of REQUIRED_PALETTE_KEYS) {
    const value = raw[key]
    const valuePath = `${path}.${key}`
    if (typeof value !== 'string' || value.trim().length === 0) {
      errors.push({ path: valuePath, message: 'is required and must be a color string' })
      continue
    }
    const parsed = parseColor(value)
    if (!parsed) {
      errors.push({ path: valuePath, message: 'must be a hex or rgb/rgba color' })
      continue
    }
    if (parsed.a < 1) {
      errors.push({ path: valuePath, message: 'must be opaque; use overrides for translucent colors' })
      continue
    }
    palette[key] = value
  }

  for (const key of OPTIONAL_PALETTE_KEYS) {
    const value = raw[key]
    if (value === undefined) continue
    const valuePath = `${path}.${key}`
    if (typeof value !== 'string' || value.trim().length === 0) {
      errors.push({ path: valuePath, message: 'must be a color string when provided' })
      continue
    }
    const parsed = parseColor(value)
    if (!parsed) {
      errors.push({ path: valuePath, message: 'must be a hex or rgb/rgba color' })
      continue
    }
    if (parsed.a < 1) {
      errors.push({ path: valuePath, message: 'must be opaque; use overrides for translucent colors' })
      continue
    }
    palette[key] = value
  }

  return errors.length > before ? undefined : (palette as ThemePalette)
}

function parseOverrides(
  raw: unknown,
  path: string,
  errors: ThemeValidationIssue[]
): Partial<ThemeTokens> | undefined {
  if (raw === undefined) return undefined
  if (!isRecord(raw)) {
    errors.push({ path, message: 'must be an object' })
    return undefined
  }
  const before = errors.length
  const overrides: Partial<ThemeTokens> = {}
  for (const [key, value] of Object.entries(raw)) {
    const valuePath = `${path}.${key}`
    if (!isThemeTokenKey(key)) {
      errors.push({ path: valuePath, message: 'is not a known theme token' })
      continue
    }
    if (typeof value !== 'string') {
      errors.push({ path: valuePath, message: 'must be a string' })
      continue
    }
    const reason = unsafeValueReason(value)
    if (reason) {
      errors.push({ path: valuePath, message: reason })
      continue
    }
    overrides[key] = value
  }
  return errors.length > before ? undefined : overrides
}

function parseVariantInput(
  raw: unknown,
  path: string,
  errors: ThemeValidationIssue[]
): ThemeVariantInput | undefined {
  if (!isRecord(raw)) {
    errors.push({ path, message: 'must be an object' })
    return undefined
  }
  const before = errors.length
  const label = readStringField(raw, 'label', `${path}.label`, errors)
  const palette = parsePalette(raw.palette, `${path}.palette`, errors)
  const overrides = parseOverrides(raw.overrides, `${path}.overrides`, errors)
  const wallpaperOpacity = parseOptionalNumber(
    raw,
    'wallpaperOpacity',
    `${path}.wallpaperOpacity`,
    errors,
    0,
    1
  )
  const wallpaperSaturate = parseOptionalNumber(
    raw,
    'wallpaperSaturate',
    `${path}.wallpaperSaturate`,
    errors,
    0.4,
    2.5
  )

  if (errors.length > before || !label || !palette) return undefined
  return {
    label,
    palette,
    ...(overrides !== undefined ? { overrides } : {}),
    ...(wallpaperOpacity !== undefined ? { wallpaperOpacity } : {}),
    ...(wallpaperSaturate !== undefined ? { wallpaperSaturate } : {})
  }
}

/**
 * Validate hard contrast requirements for a fully-built theme. These are the
 * checks external themes must pass before they can be activated.
 */
export function validateThemeDefinition(definition: ThemeDefinition): ThemeValidationIssue[] {
  const issues: ThemeValidationIssue[] = []

  for (const mode of ['dark', 'light'] as const) {
    const variant = definition.variants[mode]
    if (!variant) continue
    const palette = variant.palette
    const prefix = `variants.${mode}.palette`

    if (!parseColor(palette.bg)) {
      issues.push({ path: `${prefix}.bg`, message: 'must be a parseable color' })
      continue
    }

    const checks: Array<[keyof ThemePalette, number]> = [
      ['text', 4.5],
      ['textSecondary', 4.5],
      ['textTertiary', 4.5],
      ['accent', 4.5],
      ['success', 4.5],
      ['warning', 4.5],
      ['danger', 4.5],
      ['focus', 3]
    ]

    for (const [field, minimum] of checks) {
      const color = palette[field]
      if (typeof color !== 'string' || !parseColor(color)) {
        issues.push({ path: `${prefix}.${field}`, message: 'must be a parseable color' })
        continue
      }
      const ratio = contrastRatio(color, palette.bg)
      if (ratio + 0.005 < minimum) {
        issues.push({
          path: `${prefix}.${field}`,
          message: `${field} must reach ${minimum}:1 contrast against bg (got ${ratio.toFixed(2)}:1)`
        })
      }
    }

    const fill = palette.accentFill ?? palette.accent
    if (typeof palette.onAccent !== 'string' || !parseColor(palette.onAccent)) {
      issues.push({ path: `${prefix}.onAccent`, message: 'must be a parseable color' })
    } else if (typeof fill !== 'string' || !parseColor(fill)) {
      issues.push({ path: `${prefix}.accentFill`, message: 'must be a parseable color' })
    } else {
      const ratio = contrastRatio(palette.onAccent, fill)
      if (ratio + 0.005 < 4.5) {
        issues.push({
          path: `${prefix}.onAccent`,
          message: `onAccent must reach 4.5:1 contrast against accentFill (got ${ratio.toFixed(2)}:1)`
        })
      }
    }

    // Palette checks are not enough: a theme may override the final semantic
    // tokens (for example textPrimary) after the palette is built. Validate the
    // values that actually reach the renderer so an override cannot make text
    // unreadable while the palette still looks valid.
    const tokenChecks: Array<[ThemeTokenKey, number]> = [
      ['textPrimary', 4.5],
      ['textSecondary', 4.5],
      ['textTertiary', 4.5],
      ['accent', 4.5],
      ['success', 4.5],
      ['warning', 4.5],
      ['danger', 4.5],
      ['focusRing', 3]
    ]
    const background = variant.tokens.windowSolid
    if (typeof background !== 'string' || !parseColor(background)) {
      issues.push({
        path: `variants.${mode}.tokens.windowSolid`,
        message: 'must be a parseable opaque color'
      })
    } else {
      for (const [token, minimum] of tokenChecks) {
        const value = variant.tokens[token]
        const parsed = typeof value === 'string' ? parseColor(value) : null
        if (!parsed || parsed.a < 1) {
          issues.push({
            path: `variants.${mode}.tokens.${token}`,
            message: 'must be an opaque parseable color'
          })
          continue
        }
        const ratio = contrastRatio(value, background)
        if (ratio + 0.005 < minimum) {
          issues.push({
            path: `variants.${mode}.tokens.${token}`,
            message: `${token} must reach ${minimum}:1 contrast against windowSolid (got ${ratio.toFixed(2)}:1)`
          })
        }
      }
    }

    const finalFill = variant.tokens.accentFill
    const finalOnAccent = variant.tokens.onAccent
    if (
      typeof finalFill === 'string' &&
      parseColor(finalFill) &&
      typeof finalOnAccent === 'string' &&
      parseColor(finalOnAccent)
    ) {
      const ratio = contrastRatio(finalOnAccent, finalFill)
      if (ratio + 0.005 < 4.5) {
        issues.push({
          path: `variants.${mode}.tokens.onAccent`,
          message: `onAccent must reach 4.5:1 contrast against the final accentFill (got ${ratio.toFixed(2)}:1)`
        })
      }
    }
  }

  return issues
}

/**
 * Parse and validate an external theme JSON file. Structural errors and hard
 * contrast failures both leave `definition` undefined; warnings are advisory.
 */
export function parseThemeDefinition(
  raw: unknown,
  options?: { source?: ThemeSource }
): ParsedTheme {
  const errors: ThemeValidationIssue[] = []
  const warnings: ThemeValidationIssue[] = []
  const source = options?.source ?? 'workspace'

  if (!isRecord(raw)) {
    return {
      errors: [{ path: '', message: 'Theme must be a JSON object' }],
      warnings
    }
  }

  if (raw.schemaVersion !== 1) {
    errors.push({ path: 'schemaVersion', message: 'must be 1' })
  }

  const id = readStringField(raw, 'id', 'id', errors)
  if (id !== undefined && !THEME_ID_PATTERN.test(id)) {
    errors.push({
      path: 'id',
      message: 'must match /^[a-z0-9][a-z0-9._-]{1,63}$/'
    })
  }
  const name = readStringField(raw, 'name', 'name', errors)
  const description = readStringField(raw, 'description', 'description', errors, {
    allowEmpty: true
  })
  const version = readStringField(raw, 'version', 'version', errors)
  const author = readStringField(raw, 'author', 'author', errors)
  const license = readStringField(raw, 'license', 'license', errors)
  const homepage = readStringField(raw, 'homepage', 'homepage', errors, { required: false })

  let application: ThemeApplication = 'generated-tokens'
  if (raw.application !== undefined) {
    if (raw.application === 'generated-tokens') {
      application = 'generated-tokens'
    } else if (raw.application === 'bundled-css' && source === 'builtin') {
      application = 'bundled-css'
    } else {
      errors.push({
        path: 'application',
        message:
          source === 'builtin'
            ? 'must be "bundled-css" or "generated-tokens"'
            : 'external themes must use "generated-tokens"'
      })
    }
  }

  const variants: Partial<Record<ThemeMode, ThemeVariantInput>> = {}
  const rawVariants = raw.variants
  if (!isRecord(rawVariants)) {
    errors.push({ path: 'variants', message: 'must be an object' })
  } else {
    const keys = Object.keys(rawVariants)
    if (!keys.some((key) => key === 'dark' || key === 'light')) {
      errors.push({ path: 'variants', message: 'must define at least a dark or light variant' })
    }
    for (const key of keys) {
      if (key !== 'dark' && key !== 'light') {
        errors.push({ path: `variants.${key}`, message: 'only "dark" and "light" are supported' })
        continue
      }
      const variant = parseVariantInput(rawVariants[key], `variants.${key}`, errors)
      if (variant) variants[key] = variant
    }
  }

  if (errors.length > 0) return { errors, warnings }

  const definition = defineTheme(
    {
      schemaVersion: 1,
      id: id as string,
      name: name as string,
      description: description as string,
      version: version as string,
      author: author as string,
      license: license as string,
      ...(homepage !== undefined ? { homepage } : {}),
      application,
      variants
    },
    source
  )

  const contrastErrors = validateThemeDefinition(definition)
  if (contrastErrors.length > 0) return { errors: contrastErrors, warnings }

  return { definition, errors: [], warnings }
}
