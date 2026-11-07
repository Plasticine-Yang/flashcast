import { execFile } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync } from 'node:fs'
import type { Dirent } from 'node:fs'
import { lstat, mkdir, readFile, readdir, rename, rm, writeFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join, resolve } from 'node:path'
import { promisify } from 'node:util'
import { BUILTIN_THEMES } from '@shared/themes'
import {
  parseThemeDefinition,
  type ThemeDefinition,
  type ThemeValidationIssue
} from '@shared/theme'
import type { ActionResult, ThemePackError } from '@shared/types'
import type { WorkspaceLayout } from './paths'
import { sanitizeSource } from './source'

const execFileAsync = promisify(execFile)

const MAX_THEME_JSON_BYTES = 256 * 1024
const MAX_LICENSE_BYTES = 64 * 1024
const INSPECTION_TTL_MS = 10 * 60 * 1000
const THEME_ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,63}$/

export interface ThemeSourceMeta {
  sourceType: 'git' | 'local'
  source: string
  installedAt?: number
  version?: string
}

interface InspectionRecord {
  token: string
  definition: ThemeDefinition
  sourceType: 'git' | 'local'
  source: string
  dir: string
  expiresAt: number
  alreadyInstalled: boolean
  installedVersion: string | null
}

export interface ThemeSnapshot {
  themes: ThemeDefinition[]
  errors: ThemePackError[]
}

/**
 * Owns installed theme packs for one workspace.
 *
 * Theme packs are data only: `theme.json` plus an optional `LICENSE`. Nothing
 * from an inspected pack is executed, and only those two files are copied into
 * the workspace, so a malicious archive cannot drop extra files.
 */
export class ThemeManager {
  private workspaceThemes: ThemeDefinition[] = []
  private workspaceById = new Map<string, ThemeDefinition>()
  private broken = new Map<string, ThemePackError>()
  private inspections = new Map<string, InspectionRecord>()

  constructor(
    private layout: WorkspaceLayout,
    private readonly onChanged: (snapshot: ThemeSnapshot) => void,
    private readonly toast: (message: string, tone?: 'default' | 'success' | 'warning' | 'danger') => void
  ) {}

  async init(): Promise<void> {
    await mkdir(this.layout.themesDir, { recursive: true })
    await this.reload()
  }

  async setLayout(layout: WorkspaceLayout): Promise<void> {
    this.layout = layout
    this.workspaceThemes = []
    this.workspaceById.clear()
    this.broken.clear()
    this.inspections.clear()
    await mkdir(this.layout.themesDir, { recursive: true })
    await this.reload()
  }

  async reload(): Promise<ThemeSnapshot> {
    const errors: ThemePackError[] = []
    const loaded: ThemeDefinition[] = []

    let entries: Dirent[]
    try {
      entries = await readdir(this.layout.themesDir, { withFileTypes: true })
    } catch {
      entries = []
    }

    const seen = new Set(BUILTIN_THEMES.map((theme) => theme.id))

    for (const entry of entries) {
      if (!entry.isDirectory() || entry.name.startsWith('.')) continue
      const dirName = entry.name
      const dir = join(this.layout.themesDir, dirName)
      const themeFile = join(dir, 'theme.json')
      if (!existsSync(themeFile)) continue

      let definition: ThemeDefinition | undefined
      try {
        const raw = await readBoundedJson(themeFile, MAX_THEME_JSON_BYTES)
        const parsed = parseThemeDefinition(raw, { source: 'workspace' })
        if (parsed.errors.length > 0) {
          errors.push({ id: dirName, message: formatIssues(parsed.errors) })
          continue
        }
        definition = parsed.definition
      } catch (error) {
        errors.push({ id: dirName, message: (error as Error).message })
        continue
      }

      if (!definition) {
        errors.push({ id: dirName, message: '主题文件校验失败' })
        continue
      }
      if (definition.id !== dirName) {
        errors.push({
          id: dirName,
          message: `主题 id「${definition.id}」必须与目录名「${dirName}」一致`
        })
        continue
      }
      if (seen.has(definition.id)) {
        const builtin = BUILTIN_THEMES.some((theme) => theme.id === definition!.id)
        errors.push({
          id: dirName,
          message: builtin
            ? `主题 id「${definition.id}」与内置主题冲突`
            : `主题 id「${definition.id}」重复`
        })
        continue
      }

      // LICENSE is recommended but optional; oversized files are simply ignored.
      await readOptionalText(join(dir, 'LICENSE'), MAX_LICENSE_BYTES)

      seen.add(definition.id)
      loaded.push({ ...definition, source: 'workspace' })
    }

    loaded.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN'))
    this.workspaceThemes = loaded
    this.workspaceById = new Map(loaded.map((theme) => [theme.id, theme]))
    this.broken = new Map(errors.map((error) => [error.id, error]))
    this.notify()
    return this.snapshot()
  }

  getThemes(): ThemeDefinition[] {
    return [...BUILTIN_THEMES, ...this.workspaceThemes]
  }

  getTheme(id: string | undefined): ThemeDefinition | undefined {
    if (!id) return undefined
    return BUILTIN_THEMES.find((theme) => theme.id === id) ?? this.workspaceById.get(id)
  }

  hasTheme(id: string): boolean {
    return this.getTheme(id) !== undefined
  }

  getErrors(): ThemePackError[] {
    return [...this.broken.values()]
  }

  snapshot(): ThemeSnapshot {
    return { themes: this.getThemes(), errors: this.getErrors() }
  }

  async inspect(input: string): Promise<{
    token: string
    definition: ThemeDefinition
    sourceType: 'git' | 'local'
    source: string
    warnings: ThemeValidationIssue[]
    alreadyInstalled: boolean
    installedVersion: string | null
  }> {
    const trimmed = input.trim()
    if (!trimmed) throw new Error('请输入主题仓库地址或文件夹路径')

    const sourceType: 'git' | 'local' =
      /^(https?:\/\/|git@|ssh:\/\/)/.test(trimmed) || trimmed.endsWith('.git') ? 'git' : 'local'

    let dir: string
    let cloned = false
    if (sourceType === 'git') {
      dir = await this.cloneSource(trimmed)
      cloned = true
    } else {
      dir = resolve(expandHome(trimmed))
      if (!existsSync(dir)) throw new Error('路径不存在')
    }

    try {
      const { definition, warnings } = await this.readDefinition(dir)
      if (BUILTIN_THEMES.some((theme) => theme.id === definition.id)) {
        throw new Error(`主题 id「${definition.id}」与内置主题冲突`)
      }

      const token = randomUUID()
      const safeSource = sanitizeSource(trimmed)
      await this.pruneInspections()
      const installed = this.getTheme(definition.id)
      const record: InspectionRecord = {
        token,
        definition,
        sourceType,
        source: safeSource,
        dir,
        expiresAt: Date.now() + INSPECTION_TTL_MS,
        alreadyInstalled: installed !== undefined,
        installedVersion: installed?.version ?? null
      }
      this.inspections.set(token, record)
      return {
        token,
        definition,
        sourceType,
        source: safeSource,
        warnings,
        alreadyInstalled: record.alreadyInstalled,
        installedVersion: record.installedVersion
      }
    } catch (error) {
      if (cloned) await rm(dir, { recursive: true, force: true })
      throw error
    }
  }

  async install(token: string): Promise<ThemeDefinition> {
    const record = this.inspections.get(token)
    if (!record) throw new Error('安装信息已过期，请重新检查')
    if (Date.now() > record.expiresAt) {
      this.inspections.delete(token)
      throw new Error('安装信息已过期，请重新检查')
    }
    if (BUILTIN_THEMES.some((theme) => theme.id === record.definition.id)) {
      throw new Error('不能覆盖内置主题')
    }

    try {
      await this.replaceThemeDirectory(
        record.dir,
        {
          sourceType: record.sourceType,
          source: record.source,
          installedAt: Date.now()
        },
        record.definition.id
      )
    } finally {
      this.inspections.delete(token)
      await this.removeTempClone(record.dir)
    }
    await this.reload()
    const installed = this.getTheme(record.definition.id)
    if (!installed) throw new Error('主题安装后未能加载，请检查 themes 目录')
    return installed
  }

  async update(id: string): Promise<ActionResult> {
    if (BUILTIN_THEMES.some((theme) => theme.id === id)) {
      return { ok: false, error: '内置主题随 Flashcast 版本更新' }
    }
    const dir = this.layout.themeDir(id)
    if (!existsSync(dir)) return { ok: false, error: '主题不存在' }

    const meta = await this.readSourceMeta(dir)
    if (!meta) return { ok: false, error: '没有可用的更新源' }

    let sourceDir: string | null = null
    let cloned = false
    try {
      if (meta.sourceType === 'git') {
        sourceDir = await this.cloneSource(meta.source)
        cloned = true
      } else {
        sourceDir = resolve(expandHome(meta.source))
        if (!existsSync(sourceDir)) throw new Error('本地主题源目录不存在')
      }

      const { definition } = await this.readDefinition(sourceDir)
      if (definition.id !== id) throw new Error('更新源的主题 id 与已安装主题不一致')

      const currentVersion = this.getTheme(id)?.version ?? meta.version
      if (currentVersion && currentVersion === definition.version) {
        return { ok: true, toast: { message: '已是最新版本', tone: 'default' } }
      }

      await this.replaceThemeDirectory(
        sourceDir,
        { sourceType: meta.sourceType, source: meta.source, installedAt: Date.now() },
        id
      )
      await this.reload()
      return {
        ok: true,
        toast: { message: `已更新主题「${definition.name}」`, tone: 'success' }
      }
    } catch (error) {
      return { ok: false, error: (error as Error).message }
    } finally {
      if (cloned && sourceDir) await rm(sourceDir, { recursive: true, force: true })
    }
  }

  async uninstall(id: string): Promise<ActionResult> {
    if (BUILTIN_THEMES.some((theme) => theme.id === id)) {
      return { ok: false, error: '内置主题不能卸载' }
    }
    const dir = this.layout.themeDir(id)
    const installed = this.getTheme(id)
    const known = installed !== undefined || this.broken.has(id) || existsSync(dir)
    if (!known) return { ok: false, error: '主题不存在' }

    await rm(dir, { recursive: true, force: true })
    this.broken.delete(id)
    await this.reload()
    return {
      ok: true,
      toast: { message: installed ? `已卸载主题「${installed.name}」` : '已清理无效主题目录', tone: 'success' }
    }
  }

  async openDirectory(id: string): Promise<void> {
    if (BUILTIN_THEMES.some((theme) => theme.id === id)) {
      throw new Error('内置主题没有独立的主题目录')
    }
    const dir = this.layout.themeDir(id)
    if (!existsSync(dir)) throw new Error('主题目录不存在')
    const { shell } = await import('electron')
    const error = await shell.openPath(dir)
    if (error) throw new Error(error)
  }

  /* ------------------------------- internals ------------------------------ */

  private notify(): void {
    this.onChanged(this.snapshot())
  }

  private async pruneInspections(): Promise<void> {
    const now = Date.now()
    for (const [token, record] of [...this.inspections]) {
      if (record.expiresAt > now) continue
      this.inspections.delete(token)
      if (record.sourceType === 'git') await this.removeTempClone(record.dir)
    }
  }

  private async cloneSource(source: string): Promise<string> {
    const cloneDir = join(this.layout.installTempDir, randomUUID())
    await mkdir(this.layout.installTempDir, { recursive: true })
    try {
      await execFileAsync(
        'git',
        ['clone', '--depth', '1', '--single-branch', source, cloneDir],
        {
          timeout: 90_000,
          maxBuffer: 4 * 1024 * 1024,
          env: { ...process.env, GIT_TERMINAL_PROMPT: '0' }
        }
      )
    } catch (error) {
      await rm(cloneDir, { recursive: true, force: true })
      throw new Error(`克隆失败：${(error as Error).message.split('\n')[0]}`)
    }
    return cloneDir
  }

  private async removeTempClone(dir: string): Promise<void> {
    const tempRoot = resolve(this.layout.installTempDir)
    const target = resolve(dir)
    if (target.startsWith(`${tempRoot}/`) || target.startsWith(`${tempRoot}\\`)) {
      await rm(target, { recursive: true, force: true })
    }
  }

  private async readDefinition(
    dir: string
  ): Promise<{ definition: ThemeDefinition; warnings: ThemeValidationIssue[] }> {
    const themeFile = join(dir, 'theme.json')
    if (!existsSync(themeFile)) throw new Error('主题目录缺少 theme.json')
    const raw = await readBoundedJson(themeFile, MAX_THEME_JSON_BYTES)
    const parsed = parseThemeDefinition(raw, { source: 'workspace' })
    if (!parsed.definition) {
      throw new Error(`主题校验失败：${formatIssues(parsed.errors)}`)
    }
    if (!THEME_ID_PATTERN.test(parsed.definition.id)) {
      throw new Error('主题 id 不合法')
    }
    return { definition: parsed.definition, warnings: parsed.warnings }
  }

  private async readSourceMeta(dir: string): Promise<ThemeSourceMeta | null> {
    try {
      const raw = JSON.parse(await readFile(join(dir, '.flashcast-source.json'), 'utf8')) as unknown
      if (
        raw &&
        typeof raw === 'object' &&
        !Array.isArray(raw) &&
        ((raw as ThemeSourceMeta).sourceType === 'git' ||
          (raw as ThemeSourceMeta).sourceType === 'local') &&
        typeof (raw as ThemeSourceMeta).source === 'string'
      ) {
        const meta = raw as ThemeSourceMeta
        return {
          sourceType: meta.sourceType,
          source: sanitizeSource(String(meta.source)),
          installedAt: typeof meta.installedAt === 'number' ? meta.installedAt : undefined,
          version: typeof meta.version === 'string' ? meta.version : undefined
        }
      }
    } catch {
      // No metadata: theme was installed manually or was copied into the workspace.
    }
    return null
  }

  /**
   * Stages exactly `theme.json` (+ optional LICENSE), validates the staged copy,
   * then swaps it into place. The old directory is only removed after the new
   * one is live, so a failed update leaves the previous theme usable.
   */
  private async replaceThemeDirectory(
    sourceDir: string,
    source: ThemeSourceMeta,
    id: string
  ): Promise<void> {
    if (!THEME_ID_PATTERN.test(id)) throw new Error('主题 id 不合法')
    const sourceTheme = join(sourceDir, 'theme.json')
    if (!existsSync(sourceTheme)) throw new Error('主题目录缺少 theme.json')

    await mkdir(this.layout.installTempDir, { recursive: true })
    const staging = join(this.layout.installTempDir, `theme-staging-${id}-${randomUUID()}`)
    const backup = join(this.layout.installTempDir, `theme-backup-${id}-${randomUUID()}`)
    await mkdir(staging, { recursive: true })

    try {
      await copyRegularFile(sourceTheme, join(staging, 'theme.json'), MAX_THEME_JSON_BYTES)
      const licenseSource = join(sourceDir, 'LICENSE')
      if (existsSync(licenseSource)) {
        await copyRegularFile(licenseSource, join(staging, 'LICENSE'), MAX_LICENSE_BYTES)
      }

      const raw = await readBoundedJson(join(staging, 'theme.json'), MAX_THEME_JSON_BYTES)
      const parsed = parseThemeDefinition(raw, { source: 'workspace' })
      if (!parsed.definition) throw new Error(`主题校验失败：${formatIssues(parsed.errors)}`)
      if (parsed.definition.id !== id) {
        throw new Error(`主题 id「${parsed.definition.id}」与安装 id「${id}」不一致`)
      }

      await writeFile(
        join(staging, '.flashcast-source.json'),
        `${JSON.stringify(
          {
            sourceType: source.sourceType,
            source: source.source,
            installedAt: source.installedAt ?? Date.now(),
            version: parsed.definition.version
          },
          null,
          2
        )}\n`,
        'utf8'
      )

      const destination = this.layout.themeDir(id)
      await mkdir(this.layout.themesDir, { recursive: true })
      let hadExisting = false
      if (existsSync(destination)) {
        await rm(backup, { recursive: true, force: true })
        await rename(destination, backup)
        hadExisting = true
      }
      try {
        await rename(staging, destination)
        if (hadExisting) await rm(backup, { recursive: true, force: true })
      } catch (error) {
        if (hadExisting) {
          await rm(destination, { recursive: true, force: true })
          await rename(backup, destination)
        }
        throw error
      }
    } catch (error) {
      await rm(staging, { recursive: true, force: true })
      await rm(backup, { recursive: true, force: true })
      throw error
    }
  }
}

/* --------------------------------- helpers -------------------------------- */

function formatIssues(issues: ThemeValidationIssue[]): string {
  return issues.map((issue) => `${issue.path}: ${issue.message}`).join('; ')
}

function expandHome(value: string): string {
  if (value === '~') return homedir()
  if (value.startsWith('~/') || value.startsWith('~\\')) return join(homedir(), value.slice(2))
  return value
}

async function readBoundedJson(path: string, maxBytes: number): Promise<unknown> {
  const text = await readBoundedText(path, maxBytes)
  try {
    return JSON.parse(text) as unknown
  } catch (error) {
    throw new Error(`theme.json 解析失败：${(error as Error).message}`)
  }
}

async function readBoundedText(path: string, maxBytes: number): Promise<string> {
  const info = await lstat(path)
  if (info.isSymbolicLink()) throw new Error('theme.json 不能是符号链接')
  if (!info.isFile()) throw new Error('theme.json 不是文件')
  if (info.size > maxBytes) {
    throw new Error(`theme.json 超过 ${Math.round(maxBytes / 1024)}KB 限制`)
  }
  return readFile(path, 'utf8')
}

async function readOptionalText(path: string, maxBytes: number): Promise<string | null> {
  try {
    const info = await lstat(path)
    if (!info.isFile() || info.size > maxBytes) return null
    return await readFile(path, 'utf8')
  } catch {
    return null
  }
}

async function copyRegularFile(from: string, to: string, maxBytes: number): Promise<void> {
  const info = await lstat(from)
  if (info.isSymbolicLink()) throw new Error('主题包中不允许使用符号链接')
  if (!info.isFile()) throw new Error('主题包文件类型不受支持')
  if (info.size > maxBytes) {
    throw new Error(`主题文件超过 ${Math.round(maxBytes / 1024)}KB 限制`)
  }
  await writeFile(to, await readFile(from))
}
