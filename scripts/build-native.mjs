/**
 * Builds the macOS N-API paste addon.
 *
 * The addon is only needed on macOS; other platforms keep using their existing
 * clipboard/keystroke helpers. Release builds produce one binary per
 * architecture so an x64 or arm64 app always finds a matching module.
 */
import { execFileSync } from 'node:child_process'
import { copyFileSync, existsSync, mkdirSync } from 'node:fs'
import { join } from 'node:path'

const root = join(import.meta.dirname, '..')

if (process.platform !== 'darwin') {
  console.log('[native] not macOS, skipping paste addon')
  process.exit(0)
}

const both = process.argv.includes('--both')
const arches = both ? ['arm64', 'x64'] : [process.arch]
const gyp = join(root, 'node_modules', '.bin', 'node-gyp')
const output = join(root, 'native', 'dist')
const built = join(root, 'native', 'paste', 'build', 'Release', 'paste.node')

if (!existsSync(gyp)) {
  console.error('[native] node-gyp is missing; run pnpm install first')
  process.exit(1)
}

mkdirSync(output, { recursive: true })

for (const arch of arches) {
  console.log(`[native] building paste addon for ${arch}`)
  execFileSync(gyp, ['rebuild', '--directory', 'native/paste', `--arch=${arch}`], {
    cwd: root,
    stdio: 'inherit'
  })
  if (!existsSync(built)) {
    console.error(`[native] ${built} was not produced`)
    process.exit(1)
  }
  copyFileSync(built, join(output, `paste-${arch}.node`))
  console.log(`[native] wrote native/dist/paste-${arch}.node`)
}
