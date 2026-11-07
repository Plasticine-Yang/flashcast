import { readFile } from 'node:fs/promises'
import { join } from 'node:path'
import { describe, expect, it } from 'vitest'
import { THEME_TOKEN_CSS, parseThemeDefinition } from '../theme'

describe('examples/hello-theme', () => {
  it('stays a valid installable theme pack', async () => {
    const raw = JSON.parse(
      await readFile(join(process.cwd(), 'examples/hello-theme/theme.json'), 'utf8')
    ) as unknown
    const parsed = parseThemeDefinition(raw)

    expect(parsed.errors).toEqual([])
    expect(parsed.warnings).toEqual([])
    expect(parsed.definition?.id).toBe('hello-harbor')
    expect(Object.keys(parsed.definition?.variants ?? {}).sort()).toEqual(['dark', 'light'])
    for (const variant of Object.values(parsed.definition?.variants ?? {})) {
      expect(Object.keys(variant?.tokens ?? {}).sort()).toEqual(
        Object.keys(THEME_TOKEN_CSS).sort()
      )
    }
  })
})
