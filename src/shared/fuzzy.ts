import { hasCJK, pinyinIndexOf, pinyinMatchToken } from './pinyin'

export interface FuzzyMatch {
  score: number
  /** Indices into the original (non-normalised) target string. */
  indices: number[]
}

export interface FieldMatch {
  score: number
  /** One index array per field, in the same order as the input fields. */
  indices: number[][]
}

const SEPARATORS = /[\s\-_/.:·|,，、]/

function isBoundary(text: string, index: number): boolean {
  if (index === 0) return true
  return SEPARATORS.test(text[index - 1])
}

function isCamelBoundary(text: string, index: number): boolean {
  if (index === 0) return false
  const prev = text[index - 1]
  const current = text[index]
  return prev.toLowerCase() === prev && current.toUpperCase() === current && /[a-z]/.test(prev)
}

function tokenize(query: string): string[] {
  return query
    .trim()
    .toLowerCase()
    .split(/\s+/)
    .filter(Boolean)
}

interface SingleMatch extends FuzzyMatch {
  start: number
}

function scoreSubstring(target: string, token: string, start: number): SingleMatch {
  const indices: number[] = []
  for (let i = 0; i < token.length; i += 1) indices.push(start + i)

  let score = 170 + token.length * 9
  if (start === 0) score += 120
  if (isBoundary(target, start)) score += 55
  score -= target.length * 0.35
  score += Math.max(0, 24 - Math.abs(target.length - token.length) * 2)

  return { score, indices, start }
}

function bestSubstringMatch(target: string, token: string): SingleMatch | null {
  let from = 0
  let best: SingleMatch | null = null
  let seen = 0
  while (from <= target.length - token.length && seen < 24) {
    const index = target.indexOf(token, from)
    if (index < 0) break
    const candidate = scoreSubstring(target, token, index)
    if (!best || candidate.score > best.score) best = candidate
    if (candidate.score === scoreSubstring(target, token, index).score && index === 0) break
    from = index + 1
    seen += 1
  }
  return best
}

function greedySubsequence(target: string, token: string): SingleMatch | null {
  const indices: number[] = []
  let cursor = 0
  let score = 40
  let previous = -2

  for (const char of token) {
    const found = target.indexOf(char, cursor)
    if (found < 0) return null
    indices.push(found)
    score += 6
    if (isBoundary(target, found) || isCamelBoundary(target, found)) score += 10
    if (found === previous + 1) score += 7
    else if (previous >= 0) score -= Math.min(6, (found - previous - 1) * 0.7)
    if (found === 0) score += 22
    previous = found
    cursor = found + 1
  }

  const coverage = token.length / Math.max(target.length, 1)
  score += coverage * 35
  score -= target.length * 0.2

  return { score, indices, start: indices[0] ?? 0 }
}

/** Literal matches at or above this score are trusted over pinyin. */
const LITERAL_CONFIDENT_SCORE = 300

/**
 * Matches one whitespace-separated token against one field.
 *
 * Latin matching is a case-insensitive subsequence match with bonuses for
 * word boundaries, contiguous runs and prefixes. CJK text naturally resolves
 * through the same substring path, so an intact phrase scores highest.
 *
 * Latin tokens additionally get a pinyin pass (`kuake` → 夸克浏览器) when the
 * target contains CJK and the literal hit is absent or weak. Pinyin scores are
 * capped below confident literal matches so exact Latin names still win.
 */
export function matchToken(target: string, token: string): FuzzyMatch | null {
  if (!target || !token) return null
  const normalisedTarget = target.toLowerCase()
  const normalisedToken = token.toLowerCase()

  if (normalisedTarget === normalisedToken) {
    return {
      score: 1000 + target.length,
      indices: Array.from({ length: target.length }, (_, index) => index)
    }
  }

  const match =
    bestSubstringMatch(normalisedTarget, normalisedToken) ??
    greedySubsequence(normalisedTarget, normalisedToken)
  const literal: FuzzyMatch | null = match
    ? { score: match.score, indices: match.indices }
    : null

  if (literal && literal.score >= LITERAL_CONFIDENT_SCORE) return literal

  if (token.length >= 2 && !hasCJK(token) && hasCJK(target)) {
    const index = pinyinIndexOf(target)
    if (index) {
      const pinyinMatch = pinyinMatchToken(index, token)
      if (pinyinMatch && (!literal || pinyinMatch.score > literal.score)) {
        return pinyinMatch
      }
    }
  }

  return literal
}

/** Matches the complete query against a single field. */
export function fuzzyMatch(query: string, target: string): FuzzyMatch | null {
  const tokens = tokenize(query)
  if (!target) return null
  if (tokens.length === 0) return { score: 0, indices: [] }

  let score = 0
  const indexSet = new Set<number>()
  for (const token of tokens) {
    const result = matchToken(target, token)
    if (!result) return null
    score += result.score
    for (const index of result.indices) indexSet.add(index)
  }
  return { score, indices: [...indexSet].sort((a, b) => a - b) }
}

/**
 * Matches a query against several fields. Every token must match at least one
 * field, so users can mix words from a title with words from a URL/folder.
 * Fields earlier in the array get a small tie-break bonus.
 */
export function matchFields(query: string, fields: string[]): FieldMatch | null {
  const tokens = tokenize(query)
  const indices: number[][] = fields.map(() => [])
  if (tokens.length === 0) return { score: 0, indices }

  let total = 0
  const indexSets = fields.map(() => new Set<number>())

  for (const token of tokens) {
    let best: { score: number; field: number; indices: number[] } | null = null
    fields.forEach((field, fieldIndex) => {
      const result = matchToken(field, token)
      if (!result) return
      const fieldBonus = Math.max(0, (fields.length - fieldIndex) * 2)
      const candidate = {
        score: result.score + fieldBonus,
        field: fieldIndex,
        indices: result.indices
      }
      if (!best || candidate.score > best.score) best = candidate
    })
    if (!best) return null
    const chosen = best as { score: number; field: number; indices: number[] }
    total += chosen.score
    for (const index of chosen.indices) indexSets[chosen.field].add(index)
  }

  return {
    score: total,
    indices: indexSets.map((set) => [...set].sort((a, b) => a - b))
  }
}

export interface HighlightSegment {
  text: string
  match: boolean
}

/** Turns a field + match indices into rendered text segments. */
export function highlightSegments(text: string, indices: number[] | undefined): HighlightSegment[] {
  if (!indices || indices.length === 0 || !text) return [{ text, match: false }]
  const valid = new Set(indices.filter((index) => index >= 0 && index < text.length))
  if (valid.size === 0) return [{ text, match: false }]

  const segments: HighlightSegment[] = []
  let buffer = ''
  let currentMatch = valid.has(0)
  for (let index = 0; index < text.length; index += 1) {
    const isMatch = valid.has(index)
    if (isMatch !== currentMatch) {
      if (buffer) segments.push({ text: buffer, match: currentMatch })
      buffer = ''
      currentMatch = isMatch
    }
    buffer += text[index]
  }
  if (buffer) segments.push({ text: buffer, match: currentMatch })
  return segments
}

export { domainOf } from './url'
