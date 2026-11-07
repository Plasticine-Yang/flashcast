/**
 * Install sources are written into the workspace and therefore into Git.
 * HTTP(S) URLs may carry a token or password in the userinfo section; strip
 * that before it can be committed. SSH URLs keep their username because it is
 * part of the authentication identity, not a secret.
 */
export function sanitizeSource(source: string): string {
  const trimmed = source.trim()
  try {
    const url = new URL(trimmed)
    if (url.protocol === 'http:' || url.protocol === 'https:') {
      if (url.username || url.password) {
        url.username = ''
        url.password = ''
        return url.toString()
      }
    }
  } catch {
    // Not a URL (local path or scp-like git address): keep it as typed.
  }
  return trimmed
}
