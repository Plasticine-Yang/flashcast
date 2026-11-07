/**
 * Tokyo Night: deep indigo with neon cyan and blue highlights.
 * https://github.com/enkia/tokyo-night-vscode-theme
 */
import { defineTheme, type ThemePalette } from '../theme'

const TOKYO_NIGHT_DARK_PALETTE: ThemePalette = {
  bg: '#1a1b26',
  bgElevated: '#24283b',
  surface: '#7aa2f7',
  border: '#a9b1d6',
  text: '#c0caf5',
  textSecondary: '#a9b1d6',
  textTertiary: '#9aa5ce',
  accent: '#7aa2f7',
  onAccent: '#1a1b26',
  success: '#9ece6a',
  warning: '#e0af68',
  danger: '#f7768e',
  focus: '#7dcfff',
  accentFill: '#7aa2f7',
  accentFillHover: '#6a92e7'
}

export const TOKYO_NIGHT_THEME = defineTheme({
  schemaVersion: 1,
  id: 'tokyo-night',
  name: 'Tokyo Night',
  description: '深靛蓝与霓虹青的东京夜景。',
  version: '1.0.0',
  author: 'enkia',
  license: 'MIT',
  homepage: 'https://github.com/enkia/tokyo-night-vscode-theme',
  application: 'generated-tokens',
  variants: {
    dark: {
      label: 'Night',
      palette: TOKYO_NIGHT_DARK_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(122, 162, 247, 0.20), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(125, 207, 255, 0.12), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(61, 89, 161, 0.34), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(125, 207, 255, 0.14), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(122, 162, 247, 0.20), transparent 64%), linear-gradient(158deg, #1a1b26 0%, #16161e 52%, #101018 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(61, 89, 161, 0.55), rgba(26, 27, 38, 0.65))',
        rowSelectedRim: 'rgba(122, 162, 247, 0.42)',
        glassFilter: 'blur(83px) saturate(1.55) brightness(0.66)'
      },
      wallpaperOpacity: 0.22,
      wallpaperSaturate: 1.0
    }
  }
})

export default TOKYO_NIGHT_THEME
