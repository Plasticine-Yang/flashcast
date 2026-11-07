/**
 * Dracula: the classic high-saturation purple/cyan dark theme.
 * https://draculatheme.com
 */
import { defineTheme, type ThemePalette } from '../theme'

const DRACULA_DARK_PALETTE: ThemePalette = {
  bg: '#282a36',
  bgElevated: '#44475a',
  surface: '#6272a4',
  border: '#f8f8f2',
  text: '#f8f8f2',
  textSecondary: '#c8c6d0',
  textTertiary: '#a9a7bd',
  accent: '#bd93f9',
  onAccent: '#282a36',
  success: '#50fa7b',
  warning: '#f1fa8c',
  danger: '#ff6b6b',
  focus: '#8be9fd',
  accentFill: '#bd93f9',
  accentFillHover: '#a97ef0'
}

export const DRACULA_THEME = defineTheme({
  schemaVersion: 1,
  id: 'dracula',
  name: 'Dracula',
  description: '高饱和紫与青的经典深色主题。',
  version: '1.0.0',
  author: 'Dracula Theme',
  license: 'MIT',
  homepage: 'https://draculatheme.com',
  application: 'generated-tokens',
  variants: {
    dark: {
      label: 'Dracula',
      palette: DRACULA_DARK_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(189, 147, 249, 0.22), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(139, 233, 253, 0.10), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(189, 147, 249, 0.25), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(139, 233, 253, 0.12), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(98, 114, 164, 0.28), transparent 64%), linear-gradient(158deg, #282a36 0%, #1f2030 52%, #15161f 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(189, 147, 249, 0.24), rgba(68, 71, 90, 0.62))',
        rowSelectedRim: 'rgba(189, 147, 249, 0.45)',
        glassFilter: 'blur(83px) saturate(1.55) brightness(0.64)'
      },
      wallpaperOpacity: 0.18,
      wallpaperSaturate: 0.85
    }
  }
})

export default DRACULA_THEME
