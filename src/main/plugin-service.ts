import type {
  ActionResult,
  ChromeBookmarksPluginPage,
  ChromeProfile,
  ClipboardPluginPage,
  CommandEntry,
  PluginPage,
  PluginSettingsPage,
  PluginSummary,
  SettingsField
} from '@shared/types'
import type { ConfigStore } from './config-store'
import type { ClipboardService } from './clipboard-service'
import type { ExternalPluginManager } from './external-plugins'
import type { PlatformId } from '@shared/types'
import { CHROME_BOOKMARKS_MANIFEST, CHROME_DEFAULTS, CLIPBOARD_DEFAULTS, CLIPBOARD_MANIFEST } from './builtins/manifests'
import { discoverChromeProfiles, readChromeBookmarks } from './chrome-bookmarks'

export class PluginService {
  constructor(
    private readonly config: ConfigStore,
    private readonly clipboard: ClipboardService,
    private readonly external: ExternalPluginManager,
    private readonly platform: PlatformId
  ) {}

  getSummaries(): PluginSummary[] {
    return this.external.getSummaries()
  }

  async getPage(pluginId: string): Promise<PluginPage> {
    if (pluginId === CLIPBOARD_MANIFEST.id) return this.clipboardPage()
    if (pluginId === CHROME_BOOKMARKS_MANIFEST.id) return this.chromePage()
    return this.external.getPage(pluginId, '')
  }

  async searchExternal(pluginId: string, query: string) {
    return this.external.search(pluginId, query)
  }

  async activateExternal(pluginId: string, itemId: string, alt: boolean): Promise<ActionResult> {
    return this.external.activate(pluginId, itemId, { alt })
  }

  private async clipboardPage(): Promise<ClipboardPluginPage> {
    return {
      kind: 'clipboard',
      pluginId: CLIPBOARD_MANIFEST.id,
      title: CLIPBOARD_MANIFEST.name,
      placeholder: '在剪切板中搜索…',
      searchable: true,
      items: this.clipboard.getItems(),
      maxItems: this.clipboard.getMaxItems()
    }
  }

  private async chromePage(): Promise<ChromeBookmarksPluginPage> {
    const config = await this.config.getPluginConfig('chrome-bookmarks', CHROME_DEFAULTS)
    const result = await readChromeBookmarks(this.platform, config.profile)
    return {
      kind: 'bookmarks',
      pluginId: CHROME_BOOKMARKS_MANIFEST.id,
      title: CHROME_BOOKMARKS_MANIFEST.name,
      placeholder: '搜索书签…',
      searchable: true,
      profiles: result.profiles,
      selectedProfile: result.selectedProfile,
      userDataDir: result.userDataDir,
      bookmarkFile: result.bookmarkFile,
      bookmarks: result.bookmarks,
      error: result.error
    }
  }

  async getSettings(pluginId: string): Promise<PluginSettingsPage> {
    if (pluginId === CLIPBOARD_MANIFEST.id) return this.clipboardSettings()
    if (pluginId === CHROME_BOOKMARKS_MANIFEST.id) return this.chromeSettings()
    return this.external.getSettings(pluginId)
  }

  private async clipboardSettings(): Promise<PluginSettingsPage> {
    const config = await this.config.getPluginConfig('clipboard', CLIPBOARD_DEFAULTS)
    return {
      kind: 'settings',
      pluginId: CLIPBOARD_MANIFEST.id,
      title: '剪切板设置',
      placeholder: '剪切板设置…',
      searchable: false,
      fields: [
        {
          key: 'maxItems',
          type: 'number',
          label: '最大记录容量',
          value: this.clipboard.getMaxItems(),
          min: 1,
          max: 50,
          step: 1,
          unit: '条',
          description: '默认 10 条，超出的最早记录会被自动丢弃'
        }
      ],
      error: null,
      notice: null,
      savedAt: 0
    }
  }

  private async chromeSettings(): Promise<PluginSettingsPage> {
    const config = await this.config.getPluginConfig('chrome-bookmarks', CHROME_DEFAULTS)
    const { profiles, userDataDir } = await discoverChromeProfiles(this.platform)
    const selected =
      profiles.find((profile) => profile.dir === config.profile) ??
      profiles.find((profile) => profile.dir === 'Default') ??
      profiles[0] ??
      null

    const fields: SettingsField[] = [
      {
        key: 'profile',
        type: 'select',
        label: 'Chrome Profile',
        value: selected?.dir ?? null,
        options: profiles.map((profile) => ({
          value: profile.dir,
          label: profile.name === profile.dir ? profile.dir : `${profile.name} (${profile.dir})`
        })),
        description: '默认使用 Default Profile；切换后立即生效',
        disabled: profiles.length === 0
      },
      {
        key: 'profilePath',
        type: 'text',
        label: 'Profile 路径',
        value: selected?.bookmarksPath ?? userDataDir,
        mono: true,
        readonly: true,
        description: '书签数据来自该 Chrome 配置文件'
      }
    ]

    return {
      kind: 'settings',
      pluginId: CHROME_BOOKMARKS_MANIFEST.id,
      title: 'Chrome 书签设置',
      placeholder: 'Chrome 书签设置…',
      searchable: false,
      fields,
      error: profiles.length === 0 ? '未找到 Chrome Profile' : null,
      notice:
        profiles.length > 0
          ? null
          : '请先安装 Google Chrome，并至少打开一次以创建 Profile。你也可以设置环境变量 FLASHCAST_CHROME_USER_DATA 指向自定义目录。',
      savedAt: 0
    }
  }

  async saveSettings(
    pluginId: string,
    patch: Record<string, unknown>
  ): Promise<{ page: PluginSettingsPage; savedAt: number }> {
    if (pluginId === CLIPBOARD_MANIFEST.id) {
      const value = Number(patch.maxItems)
      const maxItems = Number.isFinite(value) ? Math.min(50, Math.max(1, Math.round(value))) : 10
      await this.config.savePluginConfig('clipboard', { maxItems }, CLIPBOARD_DEFAULTS)
      await this.clipboard.setMaxItems(maxItems)
      return { page: { ...(await this.clipboardSettings()), savedAt: Date.now() }, savedAt: Date.now() }
    }

    if (pluginId === CHROME_BOOKMARKS_MANIFEST.id) {
      const { profiles } = await discoverChromeProfiles(this.platform)
      const requested = typeof patch.profile === 'string' ? patch.profile : null
      const selected = profiles.find((profile) => profile.dir === requested) ?? profiles[0] ?? null
      await this.config.savePluginConfig(
        'chrome-bookmarks',
        { profile: selected?.dir ?? null },
        CHROME_DEFAULTS
      )
      return { page: { ...(await this.chromeSettings()), savedAt: Date.now() }, savedAt: Date.now() }
    }

    return this.external.saveSettings(pluginId, patch)
  }

  async activateClipboard(id: string, paste: boolean): Promise<ActionResult> {
    const result = await this.clipboard.activate(id, paste)
    return result.ok
      ? { ok: true, close: true, paste }
      : { ok: false, error: result.error }
  }

  async deleteClipboard(id: string): Promise<ActionResult> {
    await this.clipboard.delete(id)
    return { ok: true }
  }

  async clearClipboard(): Promise<ActionResult> {
    await this.clipboard.clear()
    return { ok: true, toast: { message: '已清空剪切板记录', tone: 'success' } }
  }

  getProfiles(): Promise<{ profiles: ChromeProfile[]; userDataDir: string }> {
    return discoverChromeProfiles(this.platform)
  }

  async setPluginEnabled(id: string, enabled: boolean): Promise<ActionResult> {
    return this.external.setEnabled(id, enabled)
  }

  async uninstall(id: string): Promise<ActionResult> {
    return this.external.uninstall(id)
  }

  async update(id: string): Promise<ActionResult> {
    return this.external.update(id)
  }
}

export const COMMANDS: CommandEntry[] = [
  {
    id: 'settings',
    name: '打开设置',
    subtitle: '主题、插件、工作区与快捷键',
    keywords: ['设置', 'settings', 'preferences', 'prefs', '配置'],
    iconKey: 'settings'
  },
  {
    id: 'refresh-apps',
    name: '重新加载应用',
    subtitle: '刷新可搜索的已安装应用',
    keywords: ['刷新', 'reload', 'refresh', '应用'],
    iconKey: 'refresh'
  },
  {
    id: 'quit',
    name: '退出 Flashcast',
    subtitle: '完全退出应用',
    keywords: ['退出', 'quit', 'exit', '关闭'],
    iconKey: 'power'
  }
]
