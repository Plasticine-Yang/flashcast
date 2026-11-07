import { shell } from 'electron'
import { execFile } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, lstatSync, readFileSync } from 'node:fs'
import { cp, mkdir, readFile, readdir, rename, rm, stat, writeFile } from 'node:fs/promises'
import { createRequire } from 'node:module'
import { basename, extname, isAbsolute, join, relative, resolve } from 'node:path'
import { pathToFileURL } from 'node:url'
import { promisify } from 'node:util'
import type {
  ActionResult,
  ExternalPluginItem,
  ExternalPluginPage,
  PluginManifest,
  PluginSettingsPage,
  PluginSummary,
  SettingsField
} from '@shared/types'
import type { ConfigStore } from './config-store'
import { BUILTIN_MANIFESTS } from './builtins/manifests'
import { log } from './logger'
import { sanitizeSource } from './source'
import type { WorkspaceLayout } from './paths'

const execFileAsync = promisify(execFile)
const nativeImport = new Function('specifier', 'return import(specifier)') as (
  specifier: string
) => Promise<Record<string, unknown>>

const ID_PATTERN = /^[a-z0-9][a-z0-9._-]{1,63}$/
const MAX_PLUGIN_BYTES = 20 * 1024 * 1024
const EXCLUDED_PLUGIN_SEGMENTS = new Set(['.git', 'node_modules', '.DS_Store', '.tmp'])
const PLUGIN_CODE_COMMIT_NOTICE =
  '插件代码会写入工作区 extensions/ 并随工作区 Git 提交；仅安装可信来源。'

export interface ExternalSettingDefinition {
  key: string
  label: string
  type: 'select' | 'number' | 'switch' | 'text'
  description?: string
  options?: Array<{ value: string; label: string }>
  min?: number
  max?: number
  step?: number
  placeholder?: string
  mono?: boolean
  unit?: string
}

interface ExternalManifest {
  id: string
  name: string
  version: string
  description: string
  keywords: string[]
  icon?: string
  iconFile?: string
  entry: string
  permissions: string[]
  settings: ExternalSettingDefinition[]
  defaults: Record<string, unknown>
  layout: 'compact' | 'wide'
  searchable: boolean
  primaryAction: string
}

interface SourceMeta {
  sourceType: 'local' | 'git'
  source: string
  installedAt: number
}

interface PluginModule {
  setup?: (ctx: ExternalPluginContext) => void | Promise<void>
  dispose?: (ctx: ExternalPluginContext) => void | Promise<void>
  search?: (ctx: ExternalPluginContext, query: string) => Promise<ExternalPluginItem[]> | ExternalPluginItem[]
  activate?: (
    ctx: ExternalPluginContext,
    item: ExternalPluginItem,
    modifiers: { alt: boolean }
  ) => Promise<ActionResult | void> | ActionResult | void
  settings?: (ctx: ExternalPluginContext) => Promise<ExternalSettingDefinition[]> | ExternalSettingDefinition[]
  onConfigChange?: (ctx: ExternalPluginContext, config: Record<string, unknown>) => void | Promise<void>
}

interface LoadedPlugin {
  manifest: ExternalManifest
  dir: string
  module: PluginModule | null
  context: ExternalPluginContext | null
  disposers: Array<() => void | Promise<void>>
  error: string | null
  items: Map<string, ExternalPluginItem>
}

export interface ExternalPluginContext {
  id: string
  env: { platform: NodeJS.Platform; version: string; workspace: string }
  config: {
    get: () => Promise<Record<string, unknown>>
    set: (patch: Record<string, unknown>) => Promise<Record<string, unknown>>
  }
  services: {
    openExternal: (url: string) => Promise<void>
    copyText: (text: string) => Promise<void>
    readClipboardText: () => Promise<string>
    toast: (message: string, tone?: 'default' | 'success' | 'warning' | 'danger') => void
  }
  logger: {
    info: (...args: unknown[]) => void
    warn: (...args: unknown[]) => void
    error: (...args: unknown[]) => void
  }
  effect: (disposer: () => void | Promise<void>) => void
  onDispose: (disposer: () => void | Promise<void>) => void
  plugin: (nested: { setup?: (ctx: ExternalPluginContext) => void | Promise<void> }) => void
}

interface InspectionRecord {
  token: string
  manifest: ExternalManifest
  source: SourceMeta
  dir: string
  fileCount: number
  size: number
  expiresAt: number
  alreadyInstalled: boolean
  installedVersion: string | null
}

export class ExternalPluginManager {
  private plugins = new Map<string, LoadedPlugin>()
  private broken = new Map<string, { dir: string; error: string }>()
  private inspections = new Map<string, InspectionRecord>()

  constructor(
    private readonly config: ConfigStore,
    private layout: WorkspaceLayout,
    private readonly onChanged: (plugins: PluginSummary[]) => void,
    private readonly toast: (message: string, tone?: 'default' | 'success' | 'warning' | 'danger') => void
  ) {}

  async setLayout(layout: WorkspaceLayout): Promise<void> {
    await this.unloadAll()
    this.layout = layout
    this.plugins.clear()
    this.broken.clear()
    this.inspections.clear()
  }

  get extensionsDir(): string {
    return this.layout.extensionsDir
  }

  async init(): Promise<void> {
    await mkdir(this.layout.extensionsDir, { recursive: true })
    await rm(this.layout.installTempDir, { recursive: true, force: true })
    await mkdir(this.layout.installTempDir, { recursive: true })
    try {
      const entries = await readdir(this.layout.extensionsDir, { withFileTypes: true })
      await Promise.all(
        entries
          .filter(
            (entry) =>
              entry.isDirectory() &&
              (entry.name.startsWith('.staging-') || entry.name.startsWith('.backup-'))
          )
          .map((entry) =>
            rm(join(this.layout.extensionsDir, entry.name), { recursive: true, force: true })
          )
      )
    } catch {
      // Best-effort cleanup only.
    }
    await this.scanAndLoad()
  }

  async reload(): Promise<void> {
    await this.unloadAll()
    this.plugins.clear()
    this.broken.clear()
    await this.scanAndLoad()
    this.notify()
  }

  private async unloadAll(): Promise<void> {
    for (const plugin of this.plugins.values()) {
      await this.unloadPlugin(plugin)
    }
  }

  private async scanAndLoad(): Promise<void> {
    let entries
    try {
      entries = await readdir(this.layout.extensionsDir, { withFileTypes: true })
    } catch {
      return
    }
    for (const entry of entries) {
      if (!entry.isDirectory() || entry.name.startsWith('.')) continue
      const dir = join(this.layout.extensionsDir, entry.name)
      await this.loadPluginFromDir(dir)
    }
  }

  private async loadPluginFromDir(dir: string): Promise<void> {
    let manifest: ExternalManifest
    try {
      manifest = await readExternalManifest(dir)
    } catch (error) {
      this.broken.set(basename(dir), { dir, error: (error as Error).message })
      return
    }

    if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === manifest.id)) {
      this.broken.set(manifest.id, { dir, error: '插件 id 与内置插件冲突' })
      return
    }

    const enabled = this.config.isPluginEnabled(manifest.id)
    if (!enabled) {
      this.plugins.set(manifest.id, {
        manifest,
        dir,
        module: null,
        context: null,
        disposers: [],
        error: null,
        items: new Map()
      })
      return
    }

    const loaded: LoadedPlugin = {
      manifest,
      dir,
      module: null,
      context: null,
      disposers: [],
      error: null,
      items: new Map()
    }

    try {
      const module = await loadModule(dir, manifest.entry)
      loaded.module = module
      const defaults = {
        ...manifest.defaults,
        ...(isRecord((module as { configDefaults?: unknown }).configDefaults)
          ? ((module as { configDefaults?: Record<string, unknown> }).configDefaults ?? {})
          : {})
      }
      const context = this.createContext(manifest, defaults, loaded.disposers)
      loaded.context = context
      await module.setup?.(context)
      loaded.error = null
    } catch (error) {
      loaded.error = (error as Error).message
      log.warn(`external plugin ${manifest.id} failed:`, loaded.error)
    }
    this.plugins.set(manifest.id, loaded)
  }

  private createContext(
    manifest: ExternalManifest,
    defaults: Record<string, unknown>,
    disposers: Array<() => void | Promise<void>>
  ): ExternalPluginContext {
    const config = this.config
    const layout = this.layout
    return {
      id: manifest.id,
      env: {
        platform: process.platform,
        version: process.env.npm_package_version ?? '0.1.0',
        workspace: layout.root
      },
      config: {
        get: () => config.getPluginConfig(manifest.id, defaults),
        set: (patch) => config.savePluginConfig(manifest.id, patch, defaults)
      },
      services: {
        openExternal: async (url: string) => {
          const parsed = new URL(url)
          if (!['http:', 'https:', 'mailto:'].includes(parsed.protocol)) {
            throw new Error('只允许打开 http/https/mailto 链接')
          }
          await shell.openExternal(url)
        },
        copyText: async (text: string) => {
          const { clipboard } = await import('electron')
          await clipboard.writeText(String(text).slice(0, 200_000))
        },
        readClipboardText: async () => {
          const { clipboard } = await import('electron')
          return clipboard.readText()
        },
        toast: (message, tone = 'default') => this.toast(String(message), tone)
      },
      logger: {
        info: (...args) => log.info(`[${manifest.id}]`, ...args),
        warn: (...args) => log.warn(`[${manifest.id}]`, ...args),
        error: (...args) => log.error(`[${manifest.id}]`, ...args)
      },
      effect: (disposer) => {
        disposers.push(disposer)
      },
      onDispose: (disposer) => {
        disposers.push(disposer)
      },
      plugin: (nested) => {
        void nested.setup?.(this.createContext(manifest, defaults, disposers))
      }
    }
  }

  async unloadPlugin(plugin: LoadedPlugin): Promise<void> {
    for (const disposer of [...plugin.disposers].reverse()) {
      try {
        await disposer()
      } catch (error) {
        log.warn('plugin disposer failed:', error)
      }
    }
    if (plugin.module?.dispose && plugin.context) {
      try {
        await plugin.module.dispose(plugin.context)
      } catch (error) {
        log.warn('plugin dispose failed:', error)
      }
    }
  }

  getSummaries(): PluginSummary[] {
    const builtins = BUILTIN_MANIFESTS.map((manifest) => {
      const enabled = this.config.isPluginEnabled(manifest.id)
      return {
        ...manifest,
        enabled,
        status: enabled ? 'ok' : 'disabled',
        source: 'builtin'
      } satisfies PluginSummary
    })

    const externals = [...this.plugins.values()].map((plugin) => {
      const enabled = this.config.isPluginEnabled(plugin.manifest.id)
      const meta = this.readSourceMeta(plugin.dir)
      return {
        id: plugin.manifest.id,
        name: plugin.manifest.name,
        description: plugin.manifest.description,
        version: plugin.manifest.version,
        keywords: plugin.manifest.keywords,
        icon: plugin.manifest.icon,
        searchable: plugin.manifest.searchable,
        primaryAction: plugin.manifest.primaryAction,
        builtin: false,
        hasSettings: plugin.manifest.settings.length > 0,
        permissions: plugin.manifest.permissions,
        layout: plugin.manifest.layout,
        enabled,
        status: plugin.error ? 'error' : enabled ? 'ok' : 'disabled',
        error: plugin.error ?? undefined,
        source: meta?.sourceType ?? 'local',
        installSource: meta?.source
      } satisfies PluginSummary
    })

    const broken = [...this.broken.entries()].map(([id, issue]) => ({
      id,
      name: id,
      description: '插件加载失败',
      version: '0.0.0',
      keywords: [],
      searchable: false,
      builtin: false,
      hasSettings: false,
      permissions: [],
      layout: 'compact',
      enabled: false,
      status: 'error',
      error: issue.error,
      source: 'local'
    })) satisfies PluginSummary[]

    return [
      ...builtins,
      ...externals.sort((a, b) => a.name.localeCompare(b.name, 'zh-CN')),
      ...broken
    ]
  }

  getManifest(id: string): ExternalManifest | null {
    return this.plugins.get(id)?.manifest ?? null
  }

  getLoaded(id: string): LoadedPlugin | undefined {
    return this.plugins.get(id)
  }

  async getPage(id: string, query: string): Promise<ExternalPluginPage> {
    const plugin = this.plugins.get(id)
    const base: ExternalPluginPage = {
      kind: 'external',
      pluginId: id,
      title: plugin?.manifest.name ?? id,
      placeholder: '搜索…',
      searchable: plugin?.manifest.searchable ?? true,
      items: [],
      error: null
    }
    if (!plugin) {
      base.error = '插件未加载'
      return base
    }
    if (!this.config.isPluginEnabled(id)) {
      base.error = '插件已停用'
      return base
    }
    if (plugin.error) {
      base.error = plugin.error
      return base
    }
    if (!plugin.module || !plugin.context) {
      base.error = '插件未初始化'
      return base
    }
    if (!plugin.module.search) {
      base.emptyTitle = '插件暂无内容'
      base.emptyDescription = '这个插件没有提供可搜索的列表'
    } else {
      try {
        const items = await plugin.module.search(plugin.context, query)
        plugin.items = new Map()
        base.items = normaliseItems(items).map((item) => {
          plugin.items.set(item.id, item)
          return item
        })
      } catch (error) {
        base.error = `搜索失败：${(error as Error).message}`
      }
    }
    base.title = plugin.manifest.name
    base.placeholder = isCjkName(plugin.manifest.name)
      ? `在${plugin.manifest.name}中搜索…`
      : `在 ${plugin.manifest.name} 中搜索…`
    base.searchable = plugin.manifest.searchable
    return base
  }

  async search(id: string, query: string): Promise<ExternalPluginItem[]> {
    const page = await this.getPage(id, query)
    if (page.error) throw new Error(page.error)
    return page.items
  }

  async activate(
    id: string,
    itemId: string,
    modifiers: { alt: boolean }
  ): Promise<ActionResult> {
    const plugin = this.plugins.get(id)
    if (!plugin?.module?.activate || !plugin.context) {
      return { ok: false, error: '插件不支持执行操作' }
    }
    const item = plugin.items.get(itemId) ?? {
      id: itemId,
      title: itemId
    }
    try {
      const result = await plugin.module.activate(plugin.context, item, modifiers)
      return normaliseActionResult(result)
    } catch (error) {
      return { ok: false, error: (error as Error).message }
    }
  }

  async getSettings(id: string): Promise<PluginSettingsPage> {
    const plugin = this.plugins.get(id)
    if (!plugin) {
      return {
        kind: 'settings',
        pluginId: id,
        title: '插件设置',
        placeholder: '插件设置…',
        searchable: false,
        fields: [],
        error: '插件未安装',
        notice: null
      }
    }

    const defaults = { ...plugin.manifest.defaults }
    const config = await this.config.getPluginConfig(id, defaults)
    let definitions = plugin.manifest.settings
    if (plugin.module?.settings && plugin.context) {
      try {
        definitions = normaliseSettingDefinitions(await plugin.module.settings(plugin.context))
      } catch (error) {
        return {
          kind: 'settings',
          pluginId: id,
          title: isCjkName(plugin.manifest.name)
            ? `${plugin.manifest.name}设置`
            : `${plugin.manifest.name} 设置`,
          placeholder: '插件设置…',
          searchable: false,
          fields: [],
          error: `读取设置失败：${(error as Error).message}`,
          notice: null
        }
      }
    }

    return {
      kind: 'settings',
      pluginId: id,
      title: isCjkName(plugin.manifest.name)
        ? `${plugin.manifest.name}设置`
        : `${plugin.manifest.name} 设置`,
      placeholder: isCjkName(plugin.manifest.name)
        ? `${plugin.manifest.name}设置…`
        : `${plugin.manifest.name} 设置…`,
      searchable: false,
      fields: definitions.map((definition) => toField(definition, config)),
      error: null,
      notice: plugin.error
    }
  }

  async saveSettings(
    id: string,
    patch: Record<string, unknown>
  ): Promise<{ page: PluginSettingsPage; savedAt: number }> {
    const plugin = this.plugins.get(id)
    if (!plugin) throw new Error('插件未安装')
    const defaults = { ...plugin.manifest.defaults }
    const current = await this.config.getPluginConfig(id, defaults)
    const definitions = plugin.module?.settings && plugin.context
      ? normaliseSettingDefinitions(await plugin.module.settings(plugin.context))
      : plugin.manifest.settings

    const clean: Record<string, unknown> = {}
    for (const definition of definitions) {
      if (!(definition.key in patch)) continue
      clean[definition.key] = coerceValue(definition, patch[definition.key])
    }
    const next = await this.config.savePluginConfig(id, clean, defaults)
    if (plugin.module?.onConfigChange && plugin.context) {
      try {
        await plugin.module.onConfigChange(plugin.context, next)
      } catch (error) {
        log.warn('plugin onConfigChange failed:', error)
      }
    }
    const page = await this.getSettings(id)
    return { page, savedAt: Date.now() }
  }

  async setEnabled(id: string, enabled: boolean): Promise<ActionResult> {
    if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === id)) {
      await this.config.setEnabled(id, enabled)
      this.notify()
      return { ok: true }
    }
    const plugin = this.plugins.get(id)
    if (!plugin) return { ok: false, error: '插件未安装' }

    await this.config.setEnabled(id, enabled)
    if (!enabled) {
      await this.unloadPlugin(plugin)
      plugin.module = null
      plugin.context = null
      plugin.disposers = []
      plugin.error = null
    } else {
      await this.unloadPlugin(plugin)
      this.plugins.delete(id)
      await this.loadPluginFromDir(plugin.dir)
    }
    this.notify()
    return { ok: true }
  }

  async openDirectory(id: string): Promise<void> {
    const dir = this.plugins.get(id)?.dir ?? this.broken.get(id)?.dir
    if (!dir) throw new Error('插件目录不存在')
    const error = await shell.openPath(dir)
    if (error) throw new Error(error)
  }

  async uninstall(id: string): Promise<ActionResult> {
    if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === id)) {
      return { ok: false, error: '内置插件不能卸载' }
    }
    const plugin = this.plugins.get(id)
    const broken = this.broken.get(id)
    const dir = plugin?.dir ?? broken?.dir
    if (!dir) return { ok: false, error: '插件目录不存在' }
    if (plugin) await this.unloadPlugin(plugin)
    await rm(dir, { recursive: true, force: true })
    await rm(this.layout.pluginDir(id), { recursive: true, force: true })
    this.plugins.delete(id)
    this.broken.delete(id)
    await this.config.setEnabled(id, false)
    this.notify()
    return { ok: true }
  }

  async inspect(input: string): Promise<{
    token: string
    manifest: PluginManifest
    sourceType: 'git' | 'local'
    source: string
    permissions: string[]
    entry: string
    fileCount: number
    size: number
    alreadyInstalled: boolean
    installedVersion: string | null
    warning: string | null
  }> {
    const trimmed = input.trim()
    if (!trimmed) throw new Error('请输入插件仓库地址或文件路径')

    const sourceType: 'git' | 'local' = /^(https?:\/\/|git@|ssh:\/\/)/.test(trimmed) || trimmed.endsWith('.git')
      ? 'git'
      : 'local'

    let dir: string
    let cloned = false
    if (sourceType === 'git') {
      const token = randomUUID()
      const cloneDir = join(this.layout.installTempDir, token)
      await mkdir(this.layout.installTempDir, { recursive: true })
      try {
        await execFileAsync(
          'git',
          ['clone', '--depth', '1', '--single-branch', trimmed, cloneDir],
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
      dir = cloneDir
      cloned = true
    } else {
      const expanded = trimmed.startsWith('~')
        ? join(process.env.HOME ?? process.env.USERPROFILE ?? '', trimmed.slice(1))
        : trimmed
      dir = resolve(expanded)
      if (!existsSync(dir)) throw new Error('路径不存在')
    }

    try {
      const manifest = await readExternalManifest(dir)
      if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === manifest.id)) {
        throw new Error(`插件 id「${manifest.id}」与内置插件冲突`)
      }

      const token = randomUUID()
      const safeSource = sanitizeSource(trimmed)
      const stats = await directoryStats(dir)
      if (stats.size > MAX_PLUGIN_BYTES) {
        throw new Error(
          `插件体积 ${formatBytes(stats.size)} 超过 ${formatBytes(MAX_PLUGIN_BYTES)} 上限，已拒绝安装`
        )
      }
      const existing = this.plugins.get(manifest.id)
      const record: InspectionRecord = {
        token,
        manifest,
        source: { sourceType, source: safeSource, installedAt: Date.now() },
        dir,
        fileCount: stats.fileCount,
        size: stats.size,
        expiresAt: Date.now() + 10 * 60 * 1000,
        alreadyInstalled: Boolean(existing),
        installedVersion: existing?.manifest.version ?? null
      }
      this.inspections.set(token, record)

      return {
        token,
        manifest: toPublicManifest(manifest),
        sourceType,
        source: safeSource,
        permissions: manifest.permissions,
        entry: manifest.entry,
        fileCount: stats.fileCount,
        size: stats.size,
        alreadyInstalled: record.alreadyInstalled,
        installedVersion: record.installedVersion,
        warning: manifest.permissions.length
          ? `${PLUGIN_CODE_COMMIT_NOTICE}插件将在本机以当前用户权限运行，申请权限：${manifest.permissions.join('、')}`
          : PLUGIN_CODE_COMMIT_NOTICE
      }
    } catch (error) {
      if (cloned) await rm(dir, { recursive: true, force: true })
      throw error
    }
  }

  async install(token: string): Promise<PluginSummary> {
    const record = this.inspections.get(token)
    if (!record) throw new Error('安装信息已过期，请重新检查')
    if (Date.now() > record.expiresAt) {
      this.inspections.delete(token)
      throw new Error('安装信息已过期，请重新检查')
    }
    if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === record.manifest.id)) {
      throw new Error('不能覆盖内置插件')
    }

    await this.replacePluginDirectory(record.dir, record.manifest, record.source)
    this.inspections.delete(token)
    if (record.dir.startsWith(this.layout.installTempDir)) {
      await rm(record.dir, { recursive: true, force: true })
    }
    await this.config.setEnabled(record.manifest.id, true)
    this.plugins.delete(record.manifest.id)
    this.broken.delete(record.manifest.id)
    await this.loadPluginFromDir(this.layout.extensionDir(record.manifest.id))
    this.notify()
    return this.getSummaries().find((plugin) => plugin.id === record.manifest.id)!
  }

  async update(id: string): Promise<ActionResult> {
    if (BUILTIN_MANIFESTS.some((builtin) => builtin.id === id)) {
      return { ok: false, error: '内置插件随 Flashcast 版本更新' }
    }
    const plugin = this.plugins.get(id)
    const meta = this.readSourceMeta(plugin?.dir ?? this.layout.extensionDir(id))
    if (!plugin || !meta) return { ok: false, error: '没有可用的更新源' }

    try {
      let sourceDir: string
      if (meta.sourceType === 'git') {
        const token = randomUUID()
        const cloneDir = join(this.layout.installTempDir, token)
        await mkdir(this.layout.installTempDir, { recursive: true })
        await execFileAsync('git', ['clone', '--depth', '1', '--single-branch', meta.source, cloneDir], {
          timeout: 90_000,
          env: { ...process.env, GIT_TERMINAL_PROMPT: '0' }
        })
        sourceDir = cloneDir
      } else {
        sourceDir = resolve(meta.source)
        if (!existsSync(sourceDir)) throw new Error('本地插件源目录不存在')
      }
      const manifest = await readExternalManifest(sourceDir)
      if (manifest.id !== id) throw new Error('更新源的插件 id 与已安装插件不一致')
      await this.replacePluginDirectory(sourceDir, manifest, meta)
      if (sourceDir.startsWith(this.layout.installTempDir)) {
        await rm(sourceDir, { recursive: true, force: true })
      }
      this.plugins.delete(id)
      await this.loadPluginFromDir(this.layout.extensionDir(id))
      this.notify()
      return { ok: true, toast: { message: `已更新 ${manifest.name}`, tone: 'success' } }
    } catch (error) {
      return { ok: false, error: (error as Error).message }
    }
  }

  private async replacePluginDirectory(
    sourceDir: string,
    manifest: ExternalManifest,
    source: SourceMeta
  ): Promise<void> {
    const extensions = this.layout.extensionsDir
    await mkdir(extensions, { recursive: true })
    const destination = this.layout.extensionDir(manifest.id)
    const staging = join(extensions, `.staging-${manifest.id}-${Date.now()}`)
    const backup = join(extensions, `.backup-${manifest.id}-${Date.now()}`)

    await cp(sourceDir, staging, {
      recursive: true,
      dereference: false,
      filter: (path) => {
        if (!isCopyablePluginPath(relative(sourceDir, path))) return false
        try {
          return !lstatSync(path).isSymbolicLink()
        } catch {
          return false
        }
      }
    })
    await writeFile(
      join(staging, '.flashcast-source.json'),
      `${JSON.stringify(source, null, 2)}\n`,
      'utf8'
    )

    // Validate the staged copy before replacing the working plugin.
    await readExternalManifest(staging)

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
  }

  private readSourceMeta(dir: string): SourceMeta | null {
    try {
      const raw = JSON.parse(
        readFileSync(join(dir, '.flashcast-source.json'), 'utf8')
      ) as SourceMeta
      if (raw && (raw.sourceType === 'git' || raw.sourceType === 'local')) {
        return { ...raw, source: sanitizeSource(String(raw.source ?? '')) }
      }
    } catch {
      // no metadata
    }
    return null
  }

  private notify(): void {
    this.onChanged(this.getSummaries())
  }

  async refresh(): Promise<PluginSummary[]> {
    await this.reload()
    return this.getSummaries()
  }
}

/* ---------------------------- manifest parsing ---------------------------- */

async function readExternalManifest(dir: string): Promise<ExternalManifest> {
  let raw: Record<string, unknown>
  let entryFromFile: unknown = undefined
  let defaultsFromFile: Record<string, unknown> = {}

  const pluginJson = join(dir, 'plugin.json')
  if (existsSync(pluginJson)) {
    raw = JSON.parse(await readFile(pluginJson, 'utf8')) as Record<string, unknown>
    if (isRecord(raw.configDefaults)) defaultsFromFile = raw.configDefaults
  } else {
    const pkgPath = join(dir, 'package.json')
    if (!existsSync(pkgPath)) throw new Error('缺少 plugin.json 或 package.json')
    const pkg = JSON.parse(await readFile(pkgPath, 'utf8')) as Record<string, unknown>
    if (!isRecord(pkg.flashcast)) throw new Error('package.json 中缺少 flashcast 配置')
    raw = pkg.flashcast
    entryFromFile = pkg.main
    if (isRecord(raw.configDefaults)) defaultsFromFile = raw.configDefaults
  }

  const id = String(raw.id ?? '').trim()
  if (!ID_PATTERN.test(id)) throw new Error('插件 id 非法（仅允许小写字母、数字、-、_、.）')
  const name = String(raw.name ?? id).trim()
  if (!name) throw new Error('插件名称不能为空')
  const version = String(raw.version ?? '0.0.0').trim()
  const entryRaw = String(raw.entry ?? raw.main ?? entryFromFile ?? 'index.js').trim()
  const entryPath = resolve(dir, entryRaw)
  const relativeEntry = relative(dir, entryPath)
  if (!relativeEntry || relativeEntry.startsWith('..') || isAbsolute(relativeEntry)) {
    throw new Error('插件入口必须位于插件目录内')
  }
  if (!existsSync(entryPath)) throw new Error('插件入口文件不存在')

  const iconRaw = typeof raw.icon === 'string' ? raw.icon : ''
  let icon: string | undefined
  let iconFile: string | undefined
  if (iconRaw) {
    const iconPath = isAbsolute(iconRaw) ? iconRaw : resolve(dir, iconRaw)
    if (!relative(dir, iconPath).startsWith('..') && existsSync(iconPath)) {
      const ext = extname(iconPath).toLowerCase()
      if (['.png', '.svg', '.jpg', '.jpeg', '.webp'].includes(ext)) {
        const buffer = await readFile(iconPath)
        if (buffer.byteLength < 512 * 1024) {
          const mime =
            ext === '.svg' ? 'image/svg+xml' : ext === '.png' ? 'image/png' : ext === '.webp' ? 'image/webp' : 'image/jpeg'
          icon = `data:${mime};base64,${buffer.toString('base64')}`
          iconFile = iconPath
        }
      }
    }
  }

  const settings = normaliseSettingDefinitions(raw.settings)

  return {
    id,
    name,
    version,
    description: String(raw.description ?? '').trim(),
    keywords: Array.isArray(raw.keywords)
      ? raw.keywords.map((value) => String(value)).filter(Boolean).slice(0, 20)
      : [],
    icon,
    iconFile,
    entry: relativeEntry,
    permissions: Array.isArray(raw.permissions)
      ? raw.permissions.map((value) => String(value)).filter(Boolean).slice(0, 20)
      : [],
    settings,
    defaults: defaultsFromFile,
    layout: raw.layout === 'wide' ? 'wide' : 'compact',
    searchable: raw.searchable !== false,
    primaryAction: String(raw.primaryAction ?? '打开')
  }
}

function toPublicManifest(manifest: ExternalManifest): PluginManifest {
  return {
    id: manifest.id,
    name: manifest.name,
    description: manifest.description,
    version: manifest.version,
    keywords: manifest.keywords,
    icon: manifest.icon,
    searchable: manifest.searchable,
    primaryAction: manifest.primaryAction,
    builtin: false,
    hasSettings: manifest.settings.length > 0,
    permissions: manifest.permissions,
    layout: manifest.layout
  }
}

function normaliseSettingDefinitions(value: unknown): ExternalSettingDefinition[] {
  if (!Array.isArray(value)) return []
  const definitions: ExternalSettingDefinition[] = []
  for (const item of value) {
    if (!isRecord(item)) continue
    const key = String(item.key ?? '').trim()
    const label = String(item.label ?? key).trim()
    const type = item.type
    if (!key || !label) continue
    if (!['select', 'number', 'switch', 'text'].includes(String(type))) continue
    const definition: ExternalSettingDefinition = {
      key,
      label,
      type: type as ExternalSettingDefinition['type'],
      description: typeof item.description === 'string' ? item.description : undefined
    }
    if (type === 'select') {
      definition.options = Array.isArray(item.options)
        ? item.options
            .filter(isRecord)
            .map((option) => ({
              value: String(option.value ?? ''),
              label: String(option.label ?? option.value ?? '')
            }))
            .filter((option) => option.value)
        : []
    }
    if (type === 'number') {
      definition.min = typeof item.min === 'number' ? item.min : 0
      definition.max = typeof item.max === 'number' ? item.max : 9999
      definition.step = typeof item.step === 'number' ? item.step : 1
      definition.unit = typeof item.unit === 'string' ? item.unit : undefined
    }
    if (type === 'text') {
      definition.placeholder = typeof item.placeholder === 'string' ? item.placeholder : undefined
      definition.mono = item.mono === true
    }
    definitions.push(definition)
  }
  return definitions
}

function toField(definition: ExternalSettingDefinition, config: Record<string, unknown>): SettingsField {
  const value = config[definition.key]
  if (definition.type === 'select') {
    return {
      key: definition.key,
      type: 'select',
      label: definition.label,
      value: typeof value === 'string' ? value : null,
      options: definition.options ?? [],
      description: definition.description
    }
  }
  if (definition.type === 'number') {
    const number = typeof value === 'number' ? value : Number(definition.min ?? 0)
    return {
      key: definition.key,
      type: 'number',
      label: definition.label,
      value: number,
      min: definition.min ?? 0,
      max: definition.max ?? 9999,
      step: definition.step ?? 1,
      unit: definition.unit,
      description: definition.description
    }
  }
  if (definition.type === 'switch') {
    return {
      key: definition.key,
      type: 'switch',
      label: definition.label,
      value: value === true,
      description: definition.description
    }
  }
  return {
    key: definition.key,
    type: 'text',
    label: definition.label,
    value: typeof value === 'string' ? value : '',
    description: definition.description,
    placeholder: definition.placeholder,
    mono: definition.mono
  }
}

function coerceValue(definition: ExternalSettingDefinition, value: unknown): unknown {
  if (definition.type === 'switch') return value === true
  if (definition.type === 'number') {
    const number = typeof value === 'number' ? value : Number(value)
    const safe = Number.isFinite(number) ? number : definition.min ?? 0
    return Math.min(definition.max ?? 9999, Math.max(definition.min ?? 0, safe))
  }
  if (definition.type === 'select') {
    const string = String(value ?? '')
    const allowed = definition.options?.some((option) => option.value === string)
    return allowed ? string : definition.options?.[0]?.value ?? ''
  }
  return String(value ?? '').slice(0, 2000)
}

function normaliseItems(value: unknown): ExternalPluginItem[] {
  if (!Array.isArray(value)) return []
  return value
    .filter(isRecord)
    .map((item): ExternalPluginItem | null => {
      const id = String(item.id ?? '')
      const title = String(item.title ?? '')
      if (!id || !title) return null
      return {
        id: id.slice(0, 200),
        title: title.slice(0, 400),
        subtitle: typeof item.subtitle === 'string' ? item.subtitle.slice(0, 600) : undefined,
        badge: typeof item.badge === 'string' ? item.badge.slice(0, 80) : undefined,
        icon:
          typeof item.icon === 'string' && item.icon.startsWith('data:image/') && item.icon.length < 200_000
            ? item.icon
            : undefined,
        iconKey: typeof item.iconKey === 'string' ? item.iconKey : undefined
      } satisfies ExternalPluginItem
    })
    .filter((item): item is ExternalPluginItem => item !== null)
    .slice(0, 500)
}

function normaliseActionResult(value: unknown): ActionResult {
  if (!isRecord(value)) return { ok: true }
  return {
    ok: value.ok !== false,
    error: typeof value.error === 'string' ? value.error : undefined,
    close: value.close === true,
    paste: value.paste === true,
    toast: isRecord(value.toast)
      ? {
          message: String(value.toast.message ?? ''),
          tone: ['default', 'success', 'warning', 'danger'].includes(String(value.toast.tone))
            ? (value.toast.tone as 'default' | 'success' | 'warning' | 'danger')
            : 'default'
        }
      : undefined
  }
}

async function loadModule(dir: string, entry: string): Promise<PluginModule> {
  const entryPath = join(dir, entry)
  const ext = extname(entryPath)
  const requireFromPlugin = createRequire(join(dir, 'flashcast-loader.cjs'))
  let loaded: unknown

  if (ext === '.mjs') {
    loaded = await nativeImport(`${pathToFileURL(entryPath).href}?v=${Date.now()}`)
  } else {
    try {
      const resolved = requireFromPlugin.resolve(entryPath)
      delete requireFromPlugin.cache[resolved]
      loaded = requireFromPlugin(resolved)
    } catch (error) {
      if (ext === '.cjs') throw error
      loaded = await nativeImport(`${pathToFileURL(entryPath).href}?v=${Date.now()}`)
    }
  }

  const moduleValue = isRecord(loaded)
    ? ((loaded.default as unknown) ?? (loaded.plugin as unknown) ?? loaded)
    : loaded
  if (!isRecord(moduleValue)) throw new Error('插件入口必须导出一个对象')
  const candidate = moduleValue as PluginModule & { activateLifecycle?: unknown }
  if (typeof candidate.setup !== 'function' && typeof candidate.search !== 'function' && typeof candidate.activate !== 'function') {
    throw new Error('插件至少需要导出 setup、search 或 activate 之一')
  }
  return candidate
}

async function directoryStats(dir: string): Promise<{ fileCount: number; size: number }> {
  let fileCount = 0
  let size = 0
  const walk = async (current: string): Promise<void> => {
    let entries
    try {
      entries = await readdir(current, { withFileTypes: true })
    } catch {
      return
    }
    for (const entry of entries) {
      if (!isCopyablePluginPath(entry.name) || entry.isSymbolicLink()) continue
      const full = join(current, entry.name)
      if (entry.isDirectory()) await walk(full)
      else if (entry.isFile()) {
        fileCount += 1
        try {
          size += (await stat(full)).size
        } catch {
          // ignore
        }
      }
    }
  }
  await walk(dir)
  return { fileCount, size }
}

/** Package files only: VCS/dependency/cache directories and logs are never copied. */
function isCopyablePluginPath(path: string): boolean {
  const segments = path.split(/[\\/]/)
  if (segments.some((segment) => EXCLUDED_PLUGIN_SEGMENTS.has(segment))) return false
  const name = segments[segments.length - 1] ?? ''
  return !name.toLowerCase().endsWith('.log')
}

function formatBytes(bytes: number): string {
  if (bytes >= 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)}MB`
  if (bytes >= 1024) return `${Math.round(bytes / 1024)}KB`
  return `${bytes}B`
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

/** CJK plugin names read best without spaces around the verb. */
function isCjkName(name: string): boolean {
  return /[\u3400-\u9fff]/.test(name)
}

/* `require` is available because the main process is bundled as CommonJS. */
declare const require: NodeJS.Require
