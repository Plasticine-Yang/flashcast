/**
 * Atelier: a warm editorial theme — paper and ink for light mode, warm ink
 * and ochre for dark mode.
 */
import { defineTheme, type ThemePalette } from '../theme'

const ATELIER_PAPER_PALETTE: ThemePalette = {
  bg: '#f7f3ea',
  bgElevated: '#efeae0',
  surface: '#8a5a00',
  border: '#57534a',
  text: '#1f1e1d',
  textSecondary: '#57534a',
  textTertiary: '#6e685e',
  accent: '#8a5a00',
  onAccent: '#f7f3ea',
  success: '#4f6b3a',
  warning: '#7a4a00',
  danger: '#a33a2a',
  focus: '#8a5a00',
  accentFill: '#8a5a00',
  accentFillHover: '#7a4a00'
}

const ATELIER_INK_PALETTE: ThemePalette = {
  bg: '#1f1e1d',
  bgElevated: '#2b2a27',
  surface: '#d9a066',
  border: '#c9c3b6',
  text: '#f5f1e8',
  textSecondary: '#c9c3b6',
  textTertiary: '#a8a195',
  accent: '#d9a066',
  onAccent: '#1f1e1d',
  success: '#9cb380',
  warning: '#e0b36a',
  danger: '#e07a5f',
  focus: '#e0b36a',
  accentFill: '#d9a066',
  accentFillHover: '#c98f55'
}

export const ATELIER_THEME = defineTheme({
  schemaVersion: 1,
  id: 'atelier',
  name: 'Atelier',
  description: '暖纸、墨色与赭石，安静克制的编辑感暖色主题。',
  version: '1.0.0',
  author: 'Flashcast',
  license: 'MIT',
  application: 'generated-tokens',
  variants: {
    light: {
      label: 'Paper',
      palette: ATELIER_PAPER_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(255, 255, 255, 0.94), transparent 62%), radial-gradient(90% 110% at 104% 112%, rgba(138, 90, 0, 0.16), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(138, 90, 0, 0.14), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(79, 107, 58, 0.12), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(210, 180, 140, 0.18), transparent 64%), linear-gradient(158deg, #f7f3ea 0%, #efeae0 52%, #e8e1d3 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(255, 255, 255, 0.95), rgba(138, 90, 0, 0.08))',
        rowSelectedRim: 'rgba(138, 90, 0, 0.20)',
        glassFilter: 'blur(83px) saturate(1.45) brightness(1.03)'
      },
      wallpaperOpacity: 0.14,
      wallpaperSaturate: 0.9
    },
    dark: {
      label: 'Ink',
      palette: ATELIER_INK_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(217, 160, 102, 0.20), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(255, 255, 255, 0.06), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(217, 160, 102, 0.18), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(79, 107, 58, 0.12), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(43, 42, 39, 0.55), transparent 64%), linear-gradient(158deg, #1f1e1d 0%, #242320 52%, #171614 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(217, 160, 102, 0.18), rgba(43, 42, 39, 0.72))',
        rowSelectedRim: 'rgba(217, 160, 102, 0.35)',
        glassFilter: 'blur(83px) saturate(1.5) brightness(0.68)'
      },
      wallpaperOpacity: 0.18,
      wallpaperSaturate: 0.95
    }
  }
})

export default ATELIER_THEME
