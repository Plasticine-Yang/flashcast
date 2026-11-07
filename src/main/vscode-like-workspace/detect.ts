import { existsSync } from 'node:fs'
import { readFile, readdir } from 'node:fs/promises'
import { homedir } from 'node:os'
import { basename, dirname, join } from 'node:path'
import type { EditorVariant, PlatformId } from '@shared/types'

export interface VariantRecord extends EditorVariant {
  appPath: string
  appDir: string
  cliPath: string | null
  cliScript: string | null
  executable: string | null
  launchKind: 'cli' | 'exe' | 'open'
  hasUserData: boolean
}

export interface DetectOptions {
  platform?: PlatformId
  home?: string
  env?: NodeJS.ProcessEnv
  roots?: string[]
}

interface ProductInfo {
  applicationName?: string
  nameShort?: string
  nameLong?: string
  dataFolderName?: string
  urlProtocol?: string
  version?: string
}

interface PackageInfo {
  name?: string
  version?: string
  main?: string
}

interface AppCandidate {
  appPath: string
  appDir: string
  productPath: string
  packagePath: string
}

const SKIP_DIRS = new Set([
  'node_modules',
  '.git',
  'locales',
  'locales2',
  'resources',
  'docs',
  'man',
  'icons',
  'fonts',
  'applications',
  'doc',
  'include',
  'lib',
  'share'
])

const MAX_SCAN_ENTRIES = 4000

async function readJson<T>(path: string): Promise<T | null> {
  try {
    return JSON.parse(await readFile(path, 'utf8')) as T
  } catch {
    return null
  }
}

function has(path: string): boolean {
  return existsSync(path)
}

function candidateRoots(platform: PlatformId, home: string, env: NodeJS.ProcessEnv): string[] {
  if (platform === 'darwin') {
    return ['/Applications', join(home, 'Applications')]
  }
  if (platform === 'win32') {
    return [
      env.LOCALAPPDATA ? join(env.LOCALAPPDATA, 'Programs') : '',
      env.PROGRAMFILES ?? '',
      env['PROGRAMFILES(X86)'] ?? '',
      env.LOCALAPPDATA ?? ''
    ].filter(Boolean)
  }
  return ['/usr/share', '/usr/local/share', '/opt', join(home, '.local/share')]
}

async function findAppCandidates(
  root: string,
  platform: PlatformId
): Promise<AppCandidate[]> {
  const candidates: AppCandidate[] = []
  let visited = 0

  if (platform === 'darwin') {
    let entries
    try {
      entries = await readdir(root, { withFileTypes: true })
    } catch {
      return candidates
    }
    for (const entry of entries) {
      if (!entry.name.endsWith('.app')) continue
      const appPath = join(root, entry.name)
      const appDir = join(appPath, 'Contents', 'Resources', 'app')
      const productPath = join(appDir, 'product.json')
      if (!has(productPath)) continue
      candidates.push({
        appPath,
        appDir,
        productPath,
        packagePath: join(appDir, 'package.json')
      })
    }
    return candidates
  }

  const walk = async (dir: string, depth: number): Promise<void> => {
    if (visited >= MAX_SCAN_ENTRIES) return
    visited += 1
    const productPath = join(dir, 'resources', 'app', 'product.json')
    if (has(productPath)) {
      candidates.push({
        appPath: dir,
        appDir: join(dir, 'resources', 'app'),
        productPath,
        packagePath: join(dir, 'resources', 'app', 'package.json')
      })
      return
    }
    if (depth >= 3) return

    let entries
    try {
      entries = await readdir(dir, { withFileTypes: true })
    } catch {
      return
    }
    for (const entry of entries) {
      if (!entry.isDirectory()) continue
      if (entry.name.startsWith('.') || SKIP_DIRS.has(entry.name)) continue
      await walk(join(dir, entry.name), depth + 1)
    }
  }

  await walk(root, 0)
  return candidates
}

function dataDirCandidates(
  platform: PlatformId,
  home: string,
  env: NodeJS.ProcessEnv,
  info: ProductInfo,
  pkg: PackageInfo,
  appPath: string
): string[] {
  const name = info.nameShort ?? info.nameLong ?? info.applicationName ?? ''
  const bundle = basename(appPath).replace(/\.app$/i, '')
  const candidates: string[] = []

  if (platform === 'darwin') {
    const support = join(home, 'Library', 'Application Support')
    candidates.push(join(support, name), join(support, bundle))
    if (pkg.name) candidates.push(join(support, pkg.name))
  } else if (platform === 'win32') {
    const roaming = env.APPDATA ?? join(home, 'AppData', 'Roaming')
    candidates.push(join(roaming, name))
    if (env.LOCALAPPDATA) candidates.push(join(env.LOCALAPPDATA, name))
  } else {
    const config = env.XDG_CONFIG_HOME ?? join(home, '.config')
    candidates.push(join(config, name))
    if (info.applicationName) candidates.push(join(config, info.applicationName))
  }

  return [...new Set(candidates.filter(Boolean))]
}

function pickDataDir(candidates: string[]): { dataDir: string; hasUserData: boolean } {
  for (const candidate of candidates) {
    if (has(join(candidate, 'User'))) return { dataDir: candidate, hasUserData: true }
  }
  return { dataDir: candidates[0] ?? '', hasUserData: false }
}

async function findExecutable(
  platform: PlatformId,
  appPath: string,
  info: ProductInfo,
  pkg: PackageInfo
): Promise<string | null> {
  const names = [
    info.nameShort,
    info.applicationName,
    pkg.name,
    'Code',
    'code'
  ].filter((value): value is string => Boolean(value))

  if (platform === 'darwin') {
    const macos = join(appPath, 'Contents', 'MacOS')
    for (const name of names) {
      const path = join(macos, name)
      if (has(path)) return path
    }
    const electron = join(macos, 'Electron')
    return has(electron) ? electron : null
  }

  if (platform === 'win32') {
    for (const name of names) {
      for (const suffix of ['.exe', '']) {
        const path = join(appPath, `${name}${suffix}`)
        if (has(path)) return path
      }
    }
    try {
      const entries = await readdir(appPath, { withFileTypes: true })
      const exe = entries.find((entry) => entry.isFile() && entry.name.toLowerCase().endsWith('.exe'))
      return exe ? join(appPath, exe.name) : null
    } catch {
      return null
    }
  }

  for (const name of names) {
    const path = join(appPath, name)
    if (has(path)) return path
  }
  return null
}

async function toRecord(candidate: AppCandidate, platform: PlatformId, home: string, env: NodeJS.ProcessEnv): Promise<VariantRecord | null> {
  const info = await readJson<ProductInfo>(candidate.productPath)
  if (!info) return null
  const applicationName = typeof info.applicationName === 'string' ? info.applicationName.trim() : ''
  const nameShort = typeof info.nameShort === 'string' ? info.nameShort.trim() : ''
  if (!applicationName || !nameShort) return null
  if (!info.dataFolderName && !info.urlProtocol) return null
  if (!has(join(candidate.appDir, 'out')) && !has(join(candidate.appDir, 'package.json'))) return null

  const pkg = (await readJson<PackageInfo>(candidate.packagePath)) ?? {}
  const { dataDir, hasUserData } = pickDataDir(
    dataDirCandidates(platform, home, env, info, pkg, candidate.appPath)
  )

  const bin = join(candidate.appDir, 'bin')
  let cliPath: string | null = null
  let cliScript: string | null = null
  let launchKind: VariantRecord['launchKind'] = platform === 'darwin' ? 'open' : 'exe'

  if (platform === 'darwin' || platform === 'linux') {
    for (const name of [applicationName, 'code']) {
      const path = join(bin, name)
      if (has(path)) {
        cliPath = path
        launchKind = 'cli'
        break
      }
    }
    if (!cliPath) {
      const rootBin = join(candidate.appPath, 'bin')
      for (const name of [applicationName, 'code']) {
        const path = join(rootBin, name)
        if (has(path)) {
          cliPath = path
          launchKind = 'cli'
          break
        }
      }
    }
  }

  if (platform === 'win32') {
    const script = join(candidate.appDir, 'out', 'cli.js')
    if (has(script)) cliScript = script
  }

  const executable = await findExecutable(platform, candidate.appPath, info, pkg)
  if (platform === 'darwin' && !cliPath) launchKind = 'open'
  if (platform === 'win32') launchKind = executable ? 'exe' : 'open'
  if (platform === 'linux' && !cliPath) launchKind = executable ? 'exe' : 'open'

  return {
    id: applicationName,
    name: nameShort,
    version: String(info.version ?? pkg.version ?? ''),
    dataDir,
    installed: true,
    error: null,
    appPath: candidate.appPath,
    appDir: candidate.appDir,
    cliPath,
    cliScript,
    executable,
    launchKind,
    hasUserData
  }
}

function compareVersions(left: string, right: string): number {
  const parse = (value: string): number[] =>
    value
      .split(/[^\d]+/)
      .filter(Boolean)
      .map(Number)
  const a = parse(left)
  const b = parse(right)
  const length = Math.max(a.length, b.length)
  for (let index = 0; index < length; index += 1) {
    const delta = (a[index] ?? 0) - (b[index] ?? 0)
    if (delta !== 0) return delta
  }
  return 0
}

/**
 * Detects installed VS Code-family editors from their `product.json`.
 *
 * The file is the only stable identity across VS Code, Trae, Cursor,
 * Windsurf, VSCodium and future forks; app names and data-folder names differ
 * per product and per platform.
 */
export async function detectVariants(options: DetectOptions = {}): Promise<VariantRecord[]> {
  const platform = options.platform ?? (process.platform as PlatformId)
  const home = options.home ?? homedir()
  const env = options.env ?? process.env
  const roots = options.roots ?? candidateRoots(platform, home, env)
  const seen = new Set<string>()
  const records: VariantRecord[] = []

  for (const root of roots) {
    if (!root || !has(root)) continue
    const candidates = await findAppCandidates(root, platform)
    for (const candidate of candidates) {
      const record = await toRecord(candidate, platform, home, env)
      if (!record) continue
      const existingIndex = records.findIndex((entry) => entry.id === record.id)
      if (existingIndex >= 0) {
        const existing = records[existingIndex]
        const better =
          (record.hasUserData && !existing.hasUserData) ||
          (record.hasUserData === existing.hasUserData &&
            compareVersions(record.version, existing.version) > 0)
        if (better) records[existingIndex] = record
        continue
      }
      if (seen.has(record.id)) continue
      seen.add(record.id)
      records.push(record)
    }
  }

  return records.sort((left, right) => left.name.localeCompare(right.name, 'zh-CN'))
}

/** Exposed for tests and diagnostics. */
export function variantRoots(platform: PlatformId, home: string, env: NodeJS.ProcessEnv): string[] {
  return candidateRoots(platform, home, env)
}

/** Exposed for tests: the install root is the directory holding `resources/app`. */
export function variantInstallRoot(record: VariantRecord): string {
  return record.appDir.endsWith(`${join('resources', 'app')}`)
    ? dirname(dirname(record.appDir))
    : record.appPath
}
