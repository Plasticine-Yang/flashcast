const prefix = '[flashcast]'

export const log = {
  info(...args: unknown[]): void {
    console.log(prefix, ...args)
  },
  warn(...args: unknown[]): void {
    console.warn(prefix, ...args)
  },
  error(...args: unknown[]): void {
    console.error(prefix, ...args)
  }
}
