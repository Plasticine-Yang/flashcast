/**
 * Everforest: Sainnhe Park's soft moss-and-mist palette, light and dark.
 * https://github.com/sainnhe/everforest
 */
import { defineTheme, type ThemePalette } from '../theme'

const EVERFOREST_LIGHT_PALETTE: ThemePalette = {
  bg: '#fdf6e3',
  bgElevated: '#f4f0d9',
  surface: '#5f6b00',
  border: '#546268',
  text: '#4b585e',
  textSecondary: '#546268',
  textTertiary: '#5c6a72',
  accent: '#5f6b00',
  onAccent: '#fdf6e3',
  success: '#4f6b3a',
  warning: '#8a5a00',
  danger: '#b32d2a',
  focus: '#4f5f00',
  accentFill: '#5f6b00',
  accentFillHover: '#4f5f00'
}

const EVERFOREST_DARK_PALETTE: ThemePalette = {
  bg: '#2d353b',
  bgElevated: '#343f44',
  surface: '#a7c080',
  border: '#c4bda6',
  text: '#d3c6aa',
  textSecondary: '#c4bda6',
  textTertiary: '#9da9a0',
  accent: '#a7c080',
  onAccent: '#2d353b',
  success: '#a7c080',
  warning: '#dbbc7f',
  danger: '#e88f91',
  focus: '#83c092',
  accentFill: '#a7c080',
  accentFillHover: '#96b06f'
}

export const EVERFOREST_THEME = defineTheme({
  schemaVersion: 1,
  id: 'everforest',
  name: 'Everforest',
  description: '柔和苔绿与晨雾，护眼的森林色。',
  version: '1.0.0',
  author: 'Sainnhe Park',
  license: 'MIT',
  homepage: 'https://github.com/sainnhe/everforest',
  application: 'generated-tokens',
  variants: {
    light: {
      label: 'Light',
      palette: EVERFOREST_LIGHT_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(255, 255, 255, 0.94), transparent 62%), radial-gradient(90% 110% at 104% 112%, rgba(95, 107, 0, 0.14), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(95, 107, 0, 0.14), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(53, 167, 124, 0.12), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(220, 188, 127, 0.16), transparent 64%), linear-gradient(158deg, #fdf6e3 0%, #f4f0d9 52%, #e6e2cc 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(255, 255, 255, 0.95), rgba(95, 107, 0, 0.08))',
        rowSelectedRim: 'rgba(95, 107, 0, 0.20)',
        glassFilter: 'blur(83px) saturate(1.45) brightness(1.03)'
      },
      wallpaperOpacity: 0.14,
      wallpaperSaturate: 0.8
    },
    dark: {
      label: 'Dark',
      palette: EVERFOREST_DARK_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(167, 192, 128, 0.20), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(131, 192, 146, 0.12), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(85, 99, 95, 0.40), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(131, 192, 146, 0.14), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(167, 192, 128, 0.16), transparent 64%), linear-gradient(158deg, #2d353b 0%, #272e33 52%, #1f2529 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(66, 80, 71, 0.75), rgba(45, 53, 59, 0.80))',
        rowSelectedRim: 'rgba(167, 192, 128, 0.32)',
        glassFilter: 'blur(83px) saturate(1.5) brightness(0.67)'
      },
      wallpaperOpacity: 0.18,
      wallpaperSaturate: 0.9
    }
  }
})

export default EVERFOREST_THEME
