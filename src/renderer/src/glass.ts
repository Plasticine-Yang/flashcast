import { glassCssVariables, normalizeGlassIntensity } from '@shared/glass'
import type { ResolvedTheme } from '@shared/types'

/**
 * Push the user's frost level into CSS custom properties on <html>.
 *
 * Both windows call this (the launcher for its panel, the settings window for
 * the live preview and popovers) so the two stay visually identical.
 */
export function applyGlass(intensity: number, theme: ResolvedTheme): void {
  if (typeof document === 'undefined') return
  const root = document.documentElement
  const variables = glassCssVariables(normalizeGlassIntensity(intensity), theme)
  for (const [name, value] of Object.entries(variables)) {
    root.style.setProperty(name, value)
  }
}
