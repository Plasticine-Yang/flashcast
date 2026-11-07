/**
 * Kanagawa: Tommaso Laurenzi's ink-on-washi palette, with the dark "Wave"
 * and light "Lotus" variants.
 * https://github.com/rebelot/kanagawa.nvim
 */
import { defineTheme, type ThemePalette } from '../theme'

const KANAGAWA_WAVE_PALETTE: ThemePalette = {
  bg: '#1f1f28',
  bgElevated: '#2a2a37',
  surface: '#7e9cd8',
  border: '#dcd7ba',
  text: '#dcd7ba',
  textSecondary: '#c8c093',
  textTertiary: '#a6a69c',
  accent: '#7e9cd8',
  onAccent: '#1f1f28',
  success: '#98bb6c',
  warning: '#e6c384',
  danger: '#e46876',
  focus: '#7fb4ca',
  accentFill: '#7e9cd8',
  accentFillHover: '#6d8ac4'
}

const KANAGAWA_LOTUS_PALETTE: ThemePalette = {
  bg: '#f2ecbc',
  bgElevated: '#e7dba0',
  surface: '#4d699b',
  border: '#545464',
  text: '#545464',
  textSecondary: '#43436c',
  textTertiary: '#5f5c50',
  accent: '#4d699b',
  onAccent: '#ffffff',
  success: '#3f6b3f',
  warning: '#8f5b00',
  danger: '#a83245',
  focus: '#4d699b',
  accentFill: '#4d699b',
  accentFillHover: '#3f587f'
}

export const KANAGAWA_THEME = defineTheme({
  schemaVersion: 1,
  id: 'kanagawa',
  name: 'Kanagawa',
  description: '和纸与墨色，Wave 与 Lotus 两套明暗变体。',
  version: '1.0.0',
  author: 'Tommaso Laurenzi',
  license: 'MIT',
  homepage: 'https://github.com/rebelot/kanagawa.nvim',
  application: 'generated-tokens',
  variants: {
    dark: {
      label: 'Wave',
      palette: KANAGAWA_WAVE_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(126, 156, 216, 0.18), transparent 58%), radial-gradient(90% 110% at 104% 112%, rgba(127, 180, 202, 0.12), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(34, 50, 73, 0.55), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(106, 149, 137, 0.18), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(149, 127, 184, 0.16), transparent 64%), linear-gradient(158deg, #1f1f28 0%, #181820 52%, #12121a 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(45, 79, 103, 0.65), rgba(31, 31, 40, 0.75))',
        rowSelectedRim: 'rgba(126, 156, 216, 0.40)',
        glassFilter: 'blur(83px) saturate(1.55) brightness(0.66)'
      },
      wallpaperOpacity: 0.2,
      wallpaperSaturate: 0.9
    },
    light: {
      label: 'Lotus',
      palette: KANAGAWA_LOTUS_PALETTE,
      overrides: {
        glassSheen:
          'radial-gradient(120% 130% at 6% -24%, rgba(255, 255, 255, 0.94), transparent 62%), radial-gradient(90% 110% at 104% 112%, rgba(77, 105, 155, 0.18), transparent 62%)',
        aurora:
          'radial-gradient(1100px 720px at 4% -14%, rgba(77, 105, 155, 0.18), transparent 62%), radial-gradient(900px 640px at 104% -8%, rgba(106, 149, 137, 0.16), transparent 58%), radial-gradient(1040px 720px at 82% 116%, rgba(78, 140, 162, 0.10), transparent 64%), linear-gradient(158deg, #f2ecbc 0%, #e7dba0 52%, #dcd5ac 100%)',
        rowSelectedBg:
          'linear-gradient(180deg, rgba(255, 255, 255, 0.95), rgba(77, 105, 155, 0.10))',
        rowSelectedRim: 'rgba(77, 105, 155, 0.22)',
        glassFilter: 'blur(83px) saturate(1.5) brightness(1.03)'
      },
      wallpaperOpacity: 0.14,
      wallpaperSaturate: 0.8
    }
  }
})

export default KANAGAWA_THEME
