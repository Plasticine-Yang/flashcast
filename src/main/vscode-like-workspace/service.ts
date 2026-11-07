import { homedir } from 'node:os'
import { join } from 'node:path'
import type {
  ActionResult,
  EditorCleanupResult,
  EditorVariant,
  EditorVariantSettings,
  EditorWorkspace,
  EditorWorkspaceSnapshot,
  EditorWorkspaceStats,
  PlatformId,
  PluginTrigger,
  VscodeLikeWorkspaceOptions,
  VscodeLikeWorkspacePluginPage,
  VscodeLikeWorkspaceSettingsPage
} from '@shared/types'
import {
  normalizeTrigger,
  TRIGGER_PATTERN,
  validateTriggers,
  type TriggerDraft
} from '@shared/editor-triggers'
import type { ConfigStore } from '../config-store'
import {
  VSCODE_LIKE_DEFAULTS,
  VSCODE_LIKE_WORKSPACE_MANIFEST,
  type VscodeLikeConfig,
  type VscodeLikeVariantConfig
} from '../builtins/manifests'
import { detectVariants, type DetectOptions, type VariantRecord } from './detect'
import { indexVariant } from './indexer'
import { launchWorkspace, revealWorkspace } from './open'
import { pruneWorkspaces, runningVariants } from './prune'

const PLUGIN_ID = VSCODE_LIKE_WORKSPACE_MANIFEST.id
const VARIANT_CACHE_MS = 5_000
const INDEX_CACHE_MS = 5_000

interface IndexCacheEntry {
  at: number
  workspaces: EditorWorkspace[]
  error: string | null
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === 'object' && !Array.isArray(value)
}

function normalizeConfig(raw: VscodeLikeConfig | null | undefined): VscodeLikeConfig {
  const variants: Record<string, VscodeLikeVariantConfig> = {}
  for (const [id, value] of Object.entries(raw?.variants ?? {})) {
    if (!isRecord(value)) continue
    variants[id] = {
      enabled: value.enabled !== false,
      trigger: typeof value.trigger === 'string' ? normalizeTrigger(value.trigger) : ''
    }
  }
  return {
    variants,
    includeRemote: raw?.includeRemote !== false,
    hideMissing: raw?.hideMissing === true,
    maxItems:
      typeof raw?.maxItems === 'number' && Number.isFinite(raw.maxItems)
        ? Math.min(5000, Math.max(100, Math.round(raw.maxItems)))
        : 1000
  }
}

/**
 * Owns editor discovery, workspace indexing and the user-configured
 * editor→trigger map for the `vscode-like-workspace` built-in plugin.
 */
export class VscodeLikeWorkspaceService {
  private variantCache: VariantRecord[] = []
  private variantCacheAt = 0
  private configCache: VscodeLikeConfig = normalizeConfig(VSCODE_LIKE_DEFAULTS)
  private triggers: PluginTrigger[] = []
  private indexCache = new Map<string, IndexCacheEntry>()
  private workspaceMap = new Map<string, EditorWorkspace>()
  private workspaceStats: Record<string, EditorWorkspaceStats> = {}
  private variantMap = new Map<string, VariantRecord>()

  constructor(
    private readonly config: ConfigStore,
    private readonly platform: PlatformId,
    /** Injection point for tests: limits detection to fixture roots. */
    private readonly detectOptions: DetectOptions = {}
  ) {}

  async init(): Promise<void> {
    await this.reloadConfig()
    await this.refreshVariants(true)
  }

  async reloadConfig(): Promise<void> {
    const stored = await this.config.getPluginConfig<VscodeLikeConfig>(
      PLUGIN_ID,
      VSCODE_LIKE_DEFAULTS
    )
    this.configCache = normalizeConfig(stored)
    this.rebuildTriggers()
  }

  async refreshVariants(force = false): Promise<VariantRecord[]> {
    if (
      !force &&
      this.variantCacheAt > 0 &&
      Date.now() - this.variantCacheAt < VARIANT_CACHE_MS
    ) {
      return this.variantCache
    }
    const detected = await detectVariants({ platform: this.platform, ...this.detectOptions })
    this.variantCache = detected
    this.variantMap = new Map(detected.map((variant) => [variant.id, variant]))
    this.variantCacheAt = Date.now()
    this.rebuildTriggers()
    return detected
  }

  /** Synchronous snapshot used by `PluginSummary` when the launcher bootstraps. */
  getTriggersSync(): PluginTrigger[] {
    return this.triggers.map((trigger) => ({ ...trigger, args: { ...trigger.args } }))
  }

  getVariantsSync(): VariantRecord[] {
    return this.variantCache
  }

  invalidate(): void {
    this.variantCacheAt = 0
    this.indexCache.clear()
    this.workspaceMap.clear()
  }

  private rebuildTriggers(): void {
    const triggers: PluginTrigger[] = []
    const used = new Set<string>()
    for (const variant of this.variantCache) {
      const variantConfig = this.variantConfig(variant.id)
      if (!variantConfig.enabled) continue
      const keyword = variantConfig.trigger
      if (!keyword || used.has(keyword) || !TRIGGER_PATTERN.test(keyword)) continue
      used.add(keyword)
      triggers.push({
        keyword,
        label: `${variant.name} 工作区`,
        args: { editor: variant.id }
      })
    }
    this.triggers = triggers
  }

  private variantConfig(id: string): VscodeLikeVariantConfig {
    const stored = this.configCache.variants[id]
    return {
      enabled: stored?.enabled !== false,
      trigger: stored?.trigger ? normalizeTrigger(stored.trigger) : ''
    }
  }

  private enabledVariants(): VariantRecord[] {
    return this.variantCache.filter((variant) => this.variantConfig(variant.id).enabled)
  }

  private async index(
    variant: VariantRecord,
    overrides: { includeRemote?: boolean; hideMissing?: boolean } = {},
    force = false
  ): Promise<IndexCacheEntry> {
    const entry = this.indexCache.get(variant.id)
    if (!force && entry && Date.now() - entry.at <= INDEX_CACHE_MS) return entry

    const result = await indexVariant(variant, {
      platform: this.platform,
      home: homedir(),
      includeRemote: overrides.includeRemote ?? this.configCache.includeRemote,
      hideMissing: overrides.hideMissing ?? this.configCache.hideMissing,
      maxItems: this.configCache.maxItems
    })
    const next: IndexCacheEntry = { at: Date.now(), ...result }
    this.indexCache.set(variant.id, next)
    return next
  }

  async getPage(): Promise<VscodeLikeWorkspacePluginPage> {
    await this.refreshVariants(false)
    const variants = this.enabledVariants()
    const workspaces: EditorWorkspace[] = []
    const errors: Array<{ variantId: string; message: string }> = []

    for (const variant of variants) {
      const entry = await this.index(variant)
      if (entry.error) errors.push({ variantId: variant.id, message: entry.error })
      workspaces.push(...entry.workspaces)
    }

    this.workspaceMap = new Map(workspaces.map((workspace) => [workspace.id, workspace]))
    this.variantMap = new Map(this.variantCache.map((variant) => [variant.id, variant]))

    const publicVariants: EditorVariant[] = variants.map((variant) => ({
      id: variant.id,
      name: variant.name,
      version: variant.version,
      dataDir: variant.dataDir,
      installed: variant.installed,
      error: variant.error
    }))

    return {
      kind: 'vscode-like-workspace',
      pluginId: PLUGIN_ID,
      title: 'VSCode-like Workspace',
      placeholder: '搜索工作区…',
      searchable: true,
      variants: publicVariants,
      workspaces,
      errors,
      scannedAt: Date.now()
    }
  }

  async getSettings(refresh = false): Promise<VscodeLikeWorkspaceSettingsPage> {
    await this.reloadConfig()
    if (refresh) {
      this.invalidate()
      await this.refreshVariants(true)
    } else {
      await this.refreshVariants(false)
    }
    this.workspaceStats = await this.collectStats()
    return this.buildSettingsPage()
  }

  /**
   * Counts for the cleanup section. Uses the *unfiltered* view (missing paths
   * and remote workspaces included) because that is exactly what the user is
   * about to clean up.
   */
  private async collectStats(): Promise<Record<string, EditorWorkspaceStats>> {
    const stats: Record<string, EditorWorkspaceStats> = {}
    const running = await runningVariants(this.variantCache)
    for (const variant of this.variantCache) {
      const entry = await this.index(variant, { includeRemote: true, hideMissing: false }, true)
      stats[variant.id] = {
        total: entry.workspaces.length,
        missing: entry.workspaces.filter((workspace) => !workspace.exists).length,
        running: running.has(variant.id)
      }
    }
    return stats
  }

  private buildSettingsPage(
    variantsOverride?: Record<string, VscodeLikeVariantConfig>,
    optionsOverride?: VscodeLikeWorkspaceOptions,
    triggerErrors: Record<string, string> = {}
  ): VscodeLikeWorkspaceSettingsPage {
    const options = optionsOverride ?? {
      includeRemote: this.configCache.includeRemote,
      hideMissing: this.configCache.hideMissing,
      maxItems: this.configCache.maxItems
    }
    const variants: EditorVariantSettings[] = this.variantCache.map((variant) => {
      const value = variantsOverride?.[variant.id] ?? this.variantConfig(variant.id)
      return {
        id: variant.id,
        name: variant.name,
        version: variant.version,
        dataDir: variant.dataDir,
        trigger: value.trigger,
        enabled: value.enabled
      }
    })

    return {
      kind: 'vscode-like-workspace-settings',
      pluginId: PLUGIN_ID,
      title: 'VSCode-like Workspace 设置',
      placeholder: 'VSCode-like Workspace 设置…',
      searchable: false,
      variants,
      options,
      triggerErrors,
      workspaceStats: this.workspaceStats,
      error: null,
      notice:
        variants.length === 0
          ? '没有检测到 VS Code 系编辑器。安装 VS Code、Trae、Cursor 等后点击「重新扫描」。'
          : null,
      savedAt: Date.now()
    }
  }

  async saveSettings(
    patch: Record<string, unknown>,
    otherTriggers: Set<string>
  ): Promise<{ page: VscodeLikeWorkspaceSettingsPage; savedAt: number }> {
    await this.refreshVariants(false)
    const rawVariants = isRecord(patch.variants) ? patch.variants : {}

    // Drafts as typed in the settings page; `current` is what the workspace has.
    const drafts: Record<string, TriggerDraft> = {}
    const current: Record<string, TriggerDraft> = {}
    for (const variant of this.variantCache) {
      const raw = rawVariants[variant.id]
      const stored = this.variantConfig(variant.id)
      current[variant.id] = { enabled: stored.enabled, trigger: stored.trigger }
      drafts[variant.id] = {
        enabled:
          isRecord(raw) && typeof raw.enabled === 'boolean' ? raw.enabled : stored.enabled,
        trigger:
          isRecord(raw) && typeof raw.trigger === 'string' ? raw.trigger : stored.trigger
      }
    }

    const { accepted, errors } = validateTriggers(
      this.variantCache.map((variant) => variant.id),
      drafts,
      current,
      otherTriggers
    )

    const includeRemote =
      typeof patch.includeRemote === 'boolean'
        ? patch.includeRemote
        : this.configCache.includeRemote
    const hideMissing =
      typeof patch.hideMissing === 'boolean' ? patch.hideMissing : this.configCache.hideMissing
    const maxItems =
      typeof patch.maxItems === 'number' && Number.isFinite(patch.maxItems)
        ? Math.min(5000, Math.max(100, Math.round(patch.maxItems)))
        : this.configCache.maxItems
    const options: VscodeLikeWorkspaceOptions = { includeRemote, hideMissing, maxItems }

    const attempted: Record<string, VscodeLikeVariantConfig> = Object.fromEntries(
      Object.entries(accepted).map(([id, draft]) => [
        id,
        { enabled: draft.enabled, trigger: draft.trigger }
      ])
    )

    if (Object.keys(errors).length > 0) {
      return { page: this.buildSettingsPage(attempted, options, errors), savedAt: 0 }
    }

    const next: VscodeLikeConfig = {
      variants: { ...this.configCache.variants, ...attempted },
      ...options
    }
    await this.config.savePluginConfig(PLUGIN_ID, next, VSCODE_LIKE_DEFAULTS)
    this.configCache = normalizeConfig(next)
    this.indexCache.clear()
    this.rebuildTriggers()

    return { page: this.buildSettingsPage(), savedAt: Date.now() }
  }

  /** Full workspace lists for the cleanup view, including disabled editors. */
  async getWorkspaceSnapshots(): Promise<EditorWorkspaceSnapshot[]> {
    await this.refreshVariants(false)
    const running = await runningVariants(this.variantCache)
    const snapshots: EditorWorkspaceSnapshot[] = []

    for (const variant of this.variantCache) {
      const entry = await this.index(variant, { includeRemote: true, hideMissing: false }, true)
      snapshots.push({
        variantId: variant.id,
        name: variant.name,
        running: running.has(variant.id),
        workspaces: entry.workspaces,
        error: entry.error
      })
    }

    this.workspaceMap = new Map(
      snapshots.flatMap((snapshot) =>
        snapshot.workspaces.map((workspace) => [workspace.id, workspace] as const)
      )
    )
    return snapshots
  }

  /**
   * Deletes workspaces from one editor's history. Project folders are never
   * touched; removed editor state is moved into a backup directory.
   */
  async deleteWorkspaces(variantId: string, ids: string[]): Promise<EditorCleanupResult> {
    await this.refreshVariants(false)
    const variant = this.variantMap.get(variantId) ?? this.variantCache.find((entry) => entry.id === variantId)
    if (!variant) {
      return { ok: false, deleted: 0, running: false, backupDir: null, error: '编辑器不可用' }
    }

    const wanted = new Set(ids)
    const snapshot = await this.getWorkspaceSnapshots()
    const group = snapshot.find((entry) => entry.variantId === variantId)
    const targets = (group?.workspaces ?? []).filter((workspace) => wanted.has(workspace.id))

    if (targets.length === 0) {
      return { ok: false, deleted: 0, running: false, backupDir: null, error: '没有可删除的工作区' }
    }
    if (group?.running) {
      return {
        ok: false,
        deleted: 0,
        running: true,
        backupDir: null,
        error: `${variant.name} 正在运行，请先退出再删除`
      }
    }

    const result = await pruneWorkspaces(
      variant,
      targets,
      join(this.config.workspace.stateDir, 'editor-backups'),
      this.platform
    )

    this.invalidate()
    this.workspaceStats = await this.collectStats()

    if (result.running) {
      return {
        ok: false,
        deleted: 0,
        running: true,
        backupDir: null,
        error: `${variant.name} 正在运行，请先退出再删除`
      }
    }

    const failed = result.failures[0]?.message
    return {
      ok: result.removed > 0,
      deleted: result.removed,
      running: false,
      backupDir: result.backupDir,
      error: failed,
      toast:
        result.removed > 0
          ? {
              message: `已从 ${variant.name} 历史中删除 ${targets.length} 个工作区`,
              tone: 'success'
            }
          : undefined
    }
  }

  async open(workspaceId: string, options: { newWindow: boolean }): Promise<ActionResult> {
    let workspace = this.workspaceMap.get(workspaceId)
    if (!workspace) {
      await this.getPage()
      workspace = this.workspaceMap.get(workspaceId)
    }
    if (!workspace) return { ok: false, error: '工作区不存在，请重新加载后再试' }

    const variant = this.variantMap.get(workspace.variantId)
    if (!variant) return { ok: false, error: '对应的编辑器已不可用' }
    return launchWorkspace(variant, workspace, options)
  }

  async reveal(workspaceId: string): Promise<ActionResult> {
    let workspace = this.workspaceMap.get(workspaceId)
    if (!workspace) {
      await this.getPage()
      workspace = this.workspaceMap.get(workspaceId)
    }
    if (!workspace) return { ok: false, error: '工作区不存在，请重新加载后再试' }
    return revealWorkspace(workspace)
  }
}
