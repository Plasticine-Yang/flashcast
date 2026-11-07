/** Extracts the domain used for favicon lookups without throwing on bad URLs. */
export function domainOf(url: string): string {
  try {
    return new URL(url).hostname
  } catch {
    return ''
  }
}
