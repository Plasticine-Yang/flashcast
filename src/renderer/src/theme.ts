/* ------------------------------------------------------------------ */
/* Renderer-side theme application                                     */
/*                                                                    */
/* Blue Glass keeps its tokens in the bundled stylesheet, so switching */
/* to it is just "remove the injected sheet". Generated theme packs are */
/* serialized token sets; they are projected into a single scoped       */
/* `<style>` element so one theme change is one DOM write instead of    */
/* hundreds of inline custom properties.                                */
/* ------------------------------------------------------------------ */

import {
  DEFAULT_THEME_ID,
  THEME_TOKEN_CSS,
  type ThemeDefinition,
  type ThemeMode,
  type ThemeTokenKey,
  type ThemeVariant
} from '@shared/theme'
import type { PlatformId } from '@shared/types'

const STYLE_ELEMENT_ID = 'flashcast-theme-vars'
const CACHE_KEY = 'flashcast.theme-cache.v1'
const THEME_ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,63}$/
const MODE_ORDER: ThemeMode[] = ['dark', 'light']

interface ThemeCacheEntry {
  themeId: string
  mode: ThemeMode
  definition: ThemeDefinition
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function isThemeMode(value: unknown): value is ThemeMode {
  return value === 'dark' || value === 'light'
}

/**
 * Theme files are validated in the main process, but the boot cache lives in
 * renderer storage and can be edited by anything that can reach
 * `localStorage`. Reject anything that could break out of a declaration or
 * make the browser fetch a remote asset.
 */
function sanitizeTokenValue(value: unknown): string | null {
  if (typeof value !== 'string') return null
  const normalized = value.replace(/[\r\n\t]+/g, ' ').trim()
  if (!normalized) return null
  if (/[;{}<>]/.test(normalized)) return null
  const lower = normalized.toLowerCase()
  if (lower.includes('url(') || lower.includes('expression(') || lower.includes('javascript:')) {
    return null
  }
  if (lower.includes('@import') || lower.includes('/*') || lower.includes('*/')) return null
  return normalized
}

function declarationLines(
  tokens: Partial<Record<ThemeTokenKey, string>> | undefined,
  indent: string
): string {
  if (!tokens) return ''
  let css = ''
  for (const [key, property] of Object.entries(THEME_TOKEN_CSS)) {
    const value = sanitizeTokenValue(tokens[key as ThemeTokenKey])
    if (!value) continue
    css += `${indent}${property}: ${value};\n`
  }
  return css
}

function buildStylesheet(definition: ThemeDefinition, variant: ThemeVariant): string {
  const selector = `:root[data-theme-id="${definition.id}"]`
  const solid =
    sanitizeTokenValue(variant.tokens.windowSolid) ??
    sanitizeTokenValue(variant.palette.bg) ??
    'transparent'

  const media = [
    '@media (prefers-reduced-transparency: reduce) {',
    `  ${selector} {`,
    `    --fc-window: ${solid};`,
    `    --fc-glass-opaque: ${solid};`,
    '    --fc-wallpaper-opacity: 0;',
    '    --fc-frost-opacity: 0;',
    '    --fc-glass-filter: none;',
    '    --fc-glass-filter-soft: none;',
    '    --fc-glass-sheen: none;',
    '  }',
    '}',
    ''
  ].join('\n')

  return [
    `${selector} {\n${declarationLines(variant.tokens, '  ')}}\n`,
    `${selector}[data-contrast="more"] {\n${declarationLines(variant.contrastTokens, '  ')}}\n`,
    media
  ].join('\n')
}

function injectStylesheet(css: string): void {
  const existing = document.getElementById(STYLE_ELEMENT_ID)
  // Theme events often arrive in bursts (theme, catalog, bootstrap); skip the
  // DOM write when the generated tokens did not change. A rebuilt style
  // element would otherwise force a document-wide style recalculation.
  if (existing && existing.textContent === css) return
  const next = document.createElement('style')
  next.id = STYLE_ELEMENT_ID
  next.textContent = css
  if (existing) existing.replaceWith(next)
  else document.head.append(next)
}

function removeStylesheet(): void {
  document.getElementById(STYLE_ELEMENT_ID)?.remove()
}

function writeCache(definition: ThemeDefinition, mode: ThemeMode): void {
  try {
    window.localStorage.setItem(
      CACHE_KEY,
      JSON.stringify({ themeId: definition.id, mode, definition } satisfies ThemeCacheEntry)
    )
  } catch {
    // Quota / privacy-mode failures are non-fatal: the theme still renders.
  }
}

/** Enough shape checking that a tampered cache cannot crash the boot path. */
function isCachedDefinition(value: unknown): value is ThemeDefinition {
  if (!isRecord(value)) return false
  if (value.schemaVersion !== 1) return false
  if (typeof value.id !== 'string' || !THEME_ID_PATTERN.test(value.id)) return false
  // Only generated token sets are injected; bundled themes come from CSS.
  if (value.application !== 'generated-tokens') return false
  if (!isRecord(value.variants)) return false
  let usableVariants = 0
  for (const mode of MODE_ORDER) {
    const variant = value.variants[mode]
    if (variant === undefined) continue
    if (!isRecord(variant) || !isRecord(variant.tokens) || !isRecord(variant.contrastTokens)) {
      return false
    }
    usableVariants += 1
  }
  return usableVariants > 0
}

function readCache(): ThemeCacheEntry | null {
  try {
    const raw = window.localStorage.getItem(CACHE_KEY)
    if (!raw) return null
    const parsed: unknown = JSON.parse(raw)
    if (!isRecord(parsed)) return null
    const { themeId, mode, definition } = parsed
    if (typeof themeId !== 'string' || !isThemeMode(mode)) return null
    if (!isCachedDefinition(definition) || definition.id !== themeId) return null
    return { themeId, mode, definition }
  } catch {
    return null
  }
}

/**
 * Apply one theme in one mode.
 *
 * `definition` may be missing while the bootstrap request is still in flight;
 * the renderer then keeps the bundled Blue Glass tokens instead of showing a
 * half-applied pack.
 */
export function applyTheme(
  definition: ThemeDefinition | undefined,
  mode: ThemeMode,
  platform?: PlatformId
): void {
  const root = document.documentElement
  root.dataset.theme = mode
  root.dataset.themeId = definition?.id ?? DEFAULT_THEME_ID
  root.style.colorScheme = mode
  if (platform) root.dataset.platform = platform

  // Blue Glass is bundled CSS: never duplicate it into the document.
  if (
    !definition ||
    definition.application === 'bundled-css' ||
    definition.id === DEFAULT_THEME_ID
  ) {
    removeStylesheet()
    return
  }

  // A single-variant pack locks to the variant it has; report that mode in
  // `data-theme` so the bundled light/dark fallbacks stay coherent.
  const requested = definition.variants[mode]
  const effectiveMode = requested ? mode : MODE_ORDER.find((candidate) => definition.variants[candidate])
  const variant = effectiveMode ? definition.variants[effectiveMode] : undefined
  if (!effectiveMode || !variant) {
    removeStylesheet()
    return
  }

  root.dataset.theme = effectiveMode
  root.style.colorScheme = effectiveMode
  injectStylesheet(buildStylesheet(definition, variant))
  writeCache(definition, effectiveMode)
}

/**
 * Runs before React mounts, from the `theme` / `themeId` query parameters the
 * main process appends to the renderer URL. It reuses the last serialized pack
 * so a custom theme does not flash Blue Glass on every launch; when there is no
 * matching cache entry the bundled tokens are left in place until bootstrap.
 */
export function applyBootTheme(): void {
  if (typeof window === 'undefined') return
  const root = document.documentElement
  const params = new URLSearchParams(window.location.search)
  const mode: ThemeMode = params.get('theme') === 'light' ? 'light' : 'dark'
  const themeId = params.get('themeId') || DEFAULT_THEME_ID
  const cache = readCache()

  if (cache && cache.themeId === themeId && cache.definition.variants[mode]) {
    applyTheme(cache.definition, mode)
    return
  }

  root.dataset.theme = mode
  root.dataset.themeId = themeId
  root.style.colorScheme = mode
  removeStylesheet()
}

export function themeById(
  themes: readonly ThemeDefinition[] | undefined,
  id: string | undefined
): ThemeDefinition | undefined {
  if (!themes || !id) return undefined
  return themes.find((theme) => theme.id === id)
}

/**
 * Keep `themes` in sync with a definition that arrives through an event. The
 * bootstrap list normally already contains it; replacing in place keeps the
 * catalog order and picks up updated versions.
 */
export function upsertThemeDefinition(
  themes: readonly ThemeDefinition[],
  definition: ThemeDefinition
): ThemeDefinition[] {
  const index = themes.findIndex((theme) => theme.id === definition.id)
  if (index === -1) return [...themes, definition]
  const next = [...themes]
  next[index] = definition
  return next
}
