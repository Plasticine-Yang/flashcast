import { describe, expect, it } from 'vitest'
import { matchToken } from './fuzzy'

describe('pinyin matching', () => {
  it('matches full pinyin, partial runs and initials', () => {
    const target = '夸克浏览器'
    expect(matchToken(target, 'kuake')?.indices).toEqual([0, 1])
    expect(matchToken(target, 'kualiulan')?.indices).toEqual([0, 2, 3])
    expect(matchToken(target, 'kualanqi')?.indices).toEqual([0, 3, 4])
    const initials = matchToken(target, 'kllq')
    expect(initials).not.toBeNull()
    expect(initials!.indices).toHaveLength(4)
  })

  it('matches a single syllable prefix', () => {
    expect(matchToken('夸克浏览器', 'kua')?.indices).toEqual([0])
  })

  it('maps pinyin highlights back to hanzi', () => {
    expect(matchToken('微信', 'wx')?.indices).toEqual([0, 1])
    expect(matchToken('夸克 Pro', 'kuakepro')?.indices).toEqual([0, 1, 3, 4, 5])
  })

  it('handles ü/v and context-aware polyphones', () => {
    expect(matchToken('旅行', 'lvxing')?.indices).toEqual([0, 1])
    expect(matchToken('旅行', 'lüxing')?.indices).toEqual([0, 1])
    expect(matchToken('银行', 'yinhang')).not.toBeNull()
    expect(matchToken('重庆', 'chongqing')).not.toBeNull()
  })

  it('prefers a short exact title over a long fuzzy one', () => {
    const exact = matchToken('夸克', 'kuake')
    const long = matchToken('夸克浏览器', 'kuake')
    expect(exact).not.toBeNull()
    expect(long).not.toBeNull()
    expect(exact!.score).toBeGreaterThan(long!.score)
  })

  it('keeps confident literal matches above pinyin', () => {
    const literal = matchToken('Kuake Browser', 'kuake')
    const pinyin = matchToken('夸克浏览器', 'kuake')
    expect(literal).not.toBeNull()
    expect(pinyin).not.toBeNull()
    expect(literal!.score).toBeGreaterThan(pinyin!.score)
  })

  it('does not pinyin-match one-character queries', () => {
    expect(matchToken('夸克浏览器', 'k')).toBeNull()
  })

  it('ignores non-CJK targets', () => {
    expect(matchToken('Chrome', 'kuake')).toBeNull()
  })
})
