import { describe, expect, it } from 'vitest'
import { sanitizeSource } from './source'

describe('sanitizeSource', () => {
  it('strips credentials from http(s) URLs before they are committed', () => {
    expect(sanitizeSource('https://token@github.com/user/plugin.git')).toBe(
      'https://github.com/user/plugin.git'
    )
    expect(sanitizeSource('https://user:pass@example.com/theme.git')).toBe(
      'https://example.com/theme.git'
    )
  })

  it('keeps SSH usernames and local paths intact', () => {
    expect(sanitizeSource('ssh://git@github.com/user/theme.git')).toBe(
      'ssh://git@github.com/user/theme.git'
    )
    expect(sanitizeSource('git@github.com:user/theme.git')).toBe(
      'git@github.com:user/theme.git'
    )
    expect(sanitizeSource('/Users/me/themes/harbor')).toBe('/Users/me/themes/harbor')
  })
})
