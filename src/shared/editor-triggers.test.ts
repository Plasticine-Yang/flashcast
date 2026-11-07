import { describe, expect, it } from 'vitest'
import {
  normalizeTrigger,
  RESERVED_TRIGGERS,
  TRIGGER_PATTERN,
  validateTriggers,
  type TriggerDraft
} from './editor-triggers'

const current: Record<string, TriggerDraft> = {
  code: { enabled: true, trigger: 'vsc' },
  'trae-cn': { enabled: true, trigger: 'trae' },
  cursor: { enabled: true, trigger: '' }
}

function run(drafts: Record<string, TriggerDraft>, others: string[] = []) {
  return validateTriggers(Object.keys(current), drafts, current, new Set(others))
}

describe('editor trigger validation', () => {
  it('normalises case and surrounding whitespace', () => {
    expect(normalizeTrigger('  Trae  ')).toBe('trae')
  })

  it('keeps existing triggers when nothing changed', () => {
    const { accepted, errors } = run(current)
    expect(errors).toEqual({})
    expect(accepted).toEqual(current)
  })

  it('allows an empty trigger to mean "no global entry"', () => {
    const { accepted, errors } = run({ ...current, code: { enabled: true, trigger: '   ' } })
    expect(errors).toEqual({})
    expect(accepted.code.trigger).toBe('')
  })

  it('rejects duplicates inside the plugin and keeps the previous value', () => {
    const { accepted, errors } = run({ ...current, 'trae-cn': { enabled: true, trigger: 'vsc' } })
    expect(errors['trae-cn']).toBe('触发词「vsc」重复了')
    expect(accepted['trae-cn'].trigger).toBe('trae')
  })

  it('rejects reserved words and invalid characters', () => {
    expect(run({ ...current, cursor: { enabled: true, trigger: 'quit' } }).errors.cursor).toContain(
      '系统保留字'
    )
    expect(
      run({ ...current, cursor: { enabled: true, trigger: 'two words' } }).errors.cursor
    ).toContain('只能包含')
    expect(
      run({ ...current, cursor: { enabled: true, trigger: 'x'.repeat(17) } }).errors.cursor
    ).toContain('只能包含')
  })

  it('rejects triggers used by other plugins', () => {
    const { errors } = run({ ...current, cursor: { enabled: true, trigger: 'bookmarks' } }, [
      'bookmarks'
    ])
    expect(errors.cursor).toBe('触发词「bookmarks」已被其他插件使用')
  })

  it('accepts CJK triggers and dots', () => {
    const { accepted, errors } = run({ ...current, cursor: { enabled: true, trigger: '光标.ide' } })
    expect(errors).toEqual({})
    expect(accepted.cursor.trigger).toBe('光标.ide')
  })

  it('accepts a disabled variant but still registers its trigger only when enabled', () => {
    const { accepted, errors } = run({ ...current, cursor: { enabled: false, trigger: 'cur' } })
    expect(errors).toEqual({})
    expect(accepted.cursor).toEqual({ enabled: false, trigger: 'cur' })
  })

  it('exposes the same pattern and reserved list the UI relies on', () => {
    expect(TRIGGER_PATTERN.test('trae')).toBe(true)
    expect(TRIGGER_PATTERN.test('tr ae')).toBe(false)
    expect([...RESERVED_TRIGGERS]).toContain('settings')
  })
})
