import { pinyin } from 'pinyin-pro'
import type { FuzzyMatch } from './fuzzy'

/**
 * Pinyin view of a searchable field.
 *
 * Every segment is either one CJK syllable (`kua`, `ke`, …) or a single
 * non-CJK letter/digit. Each character of `text` maps back to a character in
 * the original string, so a pinyin hit can be highlighted on the hanzi.
 */
export interface PinyinSegment {
  text: string
  /** One original character index per character of `text`. */
  charIndices: number[]
  cjk: boolean
}

export interface PinyinIndex {
  text: string
  pinyin: string
  segments: PinyinSegment[]
}

const CJK_PATTERN = /[\u3400-\u9fff\uf900-\ufaff]/
const CACHE_LIMIT = 2000
const MAX_TEXT_LENGTH = 200
const MAX_QUERY_LENGTH = 32
const PINYIN_SCORE_CAP = 360

const cache = new Map<string, PinyinIndex | null>()

export function hasCJK(text: string): boolean {
  return CJK_PATTERN.test(text)
}

/**
 * Normalises a Latin query for pinyin matching: lowercases, folds `ü`/`u:`
 * onto `v`, strips tone marks and separators. Returns an empty string when the
 * token is not a Latin pinyin candidate.
 */
export function normalizePinyinQuery(token: string): string {
  return token
    .toLowerCase()
    .replace(/ü/g, 'v')
    .replace(/u:/g, 'v')
    .normalize('NFKD')
    .replace(/[\u0300-\u036f]/g, '')
    .replace(/[^a-z0-9]/g, '')
}

function normalizeSyllable(syllable: string): string {
  return syllable.toLowerCase().replace(/ü/g, 'v')
}

function buildPinyinIndex(text: string): PinyinIndex | null {
  const entries = pinyin(text, { type: 'all', toneType: 'none' })
  const segments: PinyinSegment[] = []

  for (let index = 0; index < entries.length; index += 1) {
    const entry = entries[index]
    if (entry.isZh) {
      const syllable = normalizeSyllable(entry.pinyin)
      if (!syllable) continue
      segments.push({
        text: syllable,
        charIndices: Array.from({ length: syllable.length }, () => index),
        cjk: true
      })
      continue
    }

    const origin = entry.origin.normalize('NFKC')
    if (/^[a-z0-9]$/i.test(origin)) {
      segments.push({
        text: origin.toLowerCase(),
        charIndices: [index],
        cjk: false
      })
    }
  }

  if (segments.length === 0) return null
  return {
    text,
    pinyin: segments.map((segment) => segment.text).join(''),
    segments
  }
}

/**
 * Builds (and caches) the pinyin view of a field. Returns `null` for fields
 * without CJK, very long text, or when conversion fails. Callers can safely
 * treat `null` as "no pinyin representation".
 */
export function pinyinIndexOf(text: string): PinyinIndex | null {
  if (!text || text.length > MAX_TEXT_LENGTH || !hasCJK(text)) return null
  if (cache.has(text)) return cache.get(text) ?? null

  let index: PinyinIndex | null = null
  try {
    index = buildPinyinIndex(text)
  } catch {
    index = null
  }

  if (cache.size >= CACHE_LIMIT) {
    const oldest = cache.keys().next().value
    if (oldest !== undefined) cache.delete(oldest)
  }
  cache.set(text, index)
  return index
}

interface PathStep {
  segment: number
  length: number
}

interface State {
  j: number
  contiguous: 0 | 1
  score: number
  path: PathStep[]
}

/**
 * Matches a Latin token against the pinyin view.
 *
 * The DP walks segments in order. A segment may be skipped, or it may consume
 * a matching prefix of the remaining query (`kua`, `ku`, `k`). This single
 * model covers full pinyin, partial syllables and initials:
 *
 *   kuake    → 夸克          (two full syllables)
 *   kualiulan→ 夸 + 浏览     (skip 克, then two full syllables)
 *   kualanqi → 夸 + 览器     (boundary-aligned subsequence)
 *   kllq     → 夸克浏览器    (four initials)
 *
 * Scores are capped below a confident literal match, so pinyin only fills the
 * gap when literal matching is absent or weak.
 */
export function pinyinMatchToken(index: PinyinIndex, token: string): FuzzyMatch | null {
  if (token.length < 2 || token.length > MAX_QUERY_LENGTH) return null
  const query = normalizePinyinQuery(token)
  if (query.length < 2 || query.length > MAX_QUERY_LENGTH) return null
  if (index.pinyin.length === 0) return null

  let states: State[] = [{ j: 0, contiguous: 0, score: 0, path: [] }]

  for (let segmentIndex = 0; segmentIndex < index.segments.length; segmentIndex += 1) {
    const segment = index.segments[segmentIndex]
    const next = new Map<string, State>()

    const keep = (state: State): void => {
      const key = `${state.j}:${state.contiguous}`
      const current = next.get(key)
      if (!current || state.score > current.score) next.set(key, state)
    }

    for (const state of states) {
      if (state.j >= query.length) {
        keep(state)
        continue
      }

      const skipPenalty = state.path.length === 0 ? 0.8 : 6
      keep({
        j: state.j,
        contiguous: 0,
        score: state.score - skipPenalty,
        path: state.path
      })

      const limit = Math.min(segment.text.length, query.length - state.j)
      for (let length = 1; length <= limit; length += 1) {
        if (segment.text.slice(0, length) !== query.slice(state.j, state.j + length)) break
        let add = length * 18
        if (length === segment.text.length) add += 35
        if (length === 1) add += 14
        if (state.contiguous === 1) add += 25
        if (state.path.length === 0) {
          // Starting on the first character beats starting mid-name, so `kllq`
          // highlights 夸浏览器 rather than 克浏览器.
          add += segmentIndex === 0 ? 32 : 20
        }
        keep({
          j: state.j + length,
          contiguous: 1,
          score: state.score + add,
          path: [...state.path, { segment: segmentIndex, length }]
        })
      }
    }

    states = [...next.values()]
  }

  const finals = states.filter((state) => state.j === query.length)
  if (finals.length === 0) return null
  const best = finals.reduce((left, right) => (right.score > left.score ? right : left))

  let score = best.score
  score += (query.length / index.pinyin.length) * 40
  score -= index.pinyin.length * 0.15
  score = Math.min(PINYIN_SCORE_CAP, Math.max(1, score))

  const indices = new Set<number>()
  for (const step of best.path) {
    const segment = index.segments[step.segment]
    for (let i = 0; i < step.length; i += 1) indices.add(segment.charIndices[i])
  }
  if (indices.size === 0) return null

  return {
    score,
    indices: [...indices].sort((a, b) => a - b)
  }
}
