import type { PluginManifest } from '@shared/types'

export const CLIPBOARD_DEFAULTS = { maxItems: 10 }
export const CHROME_DEFAULTS: { profile: string | null } = { profile: null }

export const CLIPBOARD_MANIFEST: PluginManifest = {
  id: 'clipboard',
  name: '剪切板',
  description: '最近复制的内容，随时取回',
  version: '1.0.0',
  keywords: ['剪切板', '剪贴板', 'clipboard', '粘贴板', '复制历史'],
  iconKey: 'clipboard',
  searchable: true,
  primaryAction: '置顶并关闭',
  builtin: true,
  hasSettings: true,
  permissions: [],
  layout: 'compact'
}

export const CHROME_BOOKMARKS_MANIFEST: PluginManifest = {
  id: 'chrome-bookmarks',
  name: 'Chrome 书签',
  description: '搜索并打开 Chrome 书签',
  version: '1.0.0',
  keywords: ['chrome-bookmarks', '浏览器书签', '书签', 'bookmarks', 'chrome', '浏览器'],
  iconKey: 'bookmark',
  searchable: true,
  primaryAction: '在 Chrome 打开',
  builtin: true,
  hasSettings: true,
  permissions: [],
  layout: 'compact'
}

export const BUILTIN_MANIFESTS = [CLIPBOARD_MANIFEST, CHROME_BOOKMARKS_MANIFEST]
