import { execFileSync } from 'node:child_process'

/**
 * macOS keeps an accessory app (LSUIElement) in the frontmost slot after its
 * only window is hidden, so a synthesized ⌘V is delivered back to Flashcast
 * instead of the app the palette was opened from. Remembering that app lets
 * the paste helper hand focus over before pressing ⌘V.
 *
 * `lsappinfo` is a plain system tool: reading the front app needs no TCC
 * permission, unlike scripting System Events.
 */

/** `lsappinfo front` prints the ASN of the frontmost application. */
export function parseFrontAsn(stdout: string): string | null {
  return /ASN:[\w-]+:/.exec(stdout)?.[0] ?? null
}

/** `lsappinfo info -only name <asn>` prints `"Safari" ASN:0x0-0x1234: …`. */
export function parseFrontName(stdout: string): string | null {
  return /^\s*"([^"]+)"/.exec(stdout)?.[1] ?? null
}

/** Process name of the frontmost app, or null when it cannot be determined. */
export function frontmostAppName(): string | null {
  if (process.platform !== 'darwin') return null
  try {
    const asn = parseFrontAsn(
      execFileSync('lsappinfo', ['front'], { encoding: 'utf8', timeout: 1500 })
    )
    if (!asn) return null
    return parseFrontName(
      execFileSync('lsappinfo', ['info', '-only', 'name', asn], {
        encoding: 'utf8',
        timeout: 1500
      })
    )
  } catch {
    // Best effort: pasting falls back to letting macOS restore focus.
    return null
  }
}
