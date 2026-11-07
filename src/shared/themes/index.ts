/**
 * The built-in theme catalog, in the order the theme picker renders it.
 * Blue Glass stays first because it is the bundled default.
 */
import type { ThemeDefinition } from '../theme'
import { BLUE_GLASS_THEME } from './blue-glass'
import { DRACULA_THEME } from './dracula'
import { TOKYO_NIGHT_THEME } from './tokyo-night'
import { KANAGAWA_THEME } from './kanagawa'
import { ATELIER_THEME } from './atelier'
import { EVERFOREST_THEME } from './everforest'

export { BLUE_GLASS_THEME } from './blue-glass'
export { DRACULA_THEME } from './dracula'
export { TOKYO_NIGHT_THEME } from './tokyo-night'
export { KANAGAWA_THEME } from './kanagawa'
export { ATELIER_THEME } from './atelier'
export { EVERFOREST_THEME } from './everforest'

export const BUILTIN_THEMES: ThemeDefinition[] = [
  BLUE_GLASS_THEME,
  DRACULA_THEME,
  TOKYO_NIGHT_THEME,
  KANAGAWA_THEME,
  ATELIER_THEME,
  EVERFOREST_THEME
]

export const BUILTIN_THEME_IDS: string[] = BUILTIN_THEMES.map((theme) => theme.id)

const BUILTIN_THEME_BY_ID = new Map(BUILTIN_THEMES.map((theme) => [theme.id, theme]))

export function getBuiltinTheme(id: string | undefined): ThemeDefinition | undefined {
  return id === undefined ? undefined : BUILTIN_THEME_BY_ID.get(id)
}
