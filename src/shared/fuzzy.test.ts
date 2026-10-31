import { describe, expect, it } from 'vitest'
import { domainOf, fuzzyMatch, highlightSegments, matchFields, matchToken } from './fuzzy'

describe('fuzzy matching', () => {
  it('matches PRD initials example foo-bar-baz with fbb', () => {
    const result = fuzzyMatch('fbb', 'Foo-Bar-Baz')
    expect(result).not.toBeNull()
    expect(result?.indices).toEqual([0, 4, 8])
  })

  it('matches inner subsequences ooar and aaz', () => {
    expect(fuzzyMatch('ooar', 'foo-bar-baz')?.indices).toEqual([1, 2, 5, 6])
    expect(fuzzyMatch('aaz', 'foo-bar-baz')?.indices).toEqual([5, 9, 10])
  })

  it('matches chrome-bookmarks keyword aliases', () => {
    const keyword = 'chrome-bookmarks'
    for (const query of ['book', 'bookmarks', 'chbook', 'chromebook', 'chrome mark']) {
      expect(fuzzyMatch(query, keyword), query).not.toBeNull()
    }
  })

  it('requires every token to match', () => {
    expect(fuzzyMatch('chrome nope', 'chrome-bookmarks')).toBeNull()
    expect(fuzzyMatch('chrome book', 'chrome-bookmarks')).not.toBeNull()
  })

  it('matches Chinese phrases and highlights the exact run', () => {
    const result = fuzzyMatch('书签', '浏览器书签')
    expect(result?.indices).toEqual([3, 4])
  })

  it('prefers exact matches over loose subsequences', () => {
    const exact = matchToken('bookmarks', 'bookmarks')!
    const loose = matchToken('chrome-bookmarks', 'bookmarks')!
    expect(exact.score).toBeGreaterThan(loose.score)
  })

  it('matches across multiple fields and reports per-field indices', () => {
    const result = matchFields('settings github', ['GitHub — Tokens', 'github.com/settings/tokens', '开发'])
    expect(result).not.toBeNull()
    expect(result?.indices[0]?.length).toBeGreaterThan(0)
    expect(result?.indices[1]?.length).toBeGreaterThan(0)
  })

  it('does not match fields when a token is absent', () => {
    expect(matchFields('github missing', ['GitHub', 'github.com'])).toBeNull()
  })

  it('splits highlights into renderable segments', () => {
    const segments = highlightSegments('chrome-bookmarks', [0, 7, 8, 9, 10])
    expect(segments.map((segment) => segment.text).join('')).toBe('chrome-bookmarks')
    expect(segments).toEqual([
      { text: 'c', match: true },
      { text: 'hrome-', match: false },
      { text: 'book', match: true },
      { text: 'marks', match: false }
    ])
  })

  it('extracts domains safely', () => {
    expect(domainOf('https://github.com/settings/tokens')).toBe('github.com')
    expect(domainOf('not a url')).toBe('')
  })
})
