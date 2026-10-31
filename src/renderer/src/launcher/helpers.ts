import { fuzzyMatch, matchFields } from '@shared/fuzzy'
import { kindLabel, relativeTime } from '@shared/time'
import type {
  AppEntry,
  ChromeBookmark,
  ClipboardItem,
  CommandEntry,
  PluginSummary
} from '@shared/types'

export type GlobalKind = 'plugin' | 'app' | 'command'

export interface GlobalResult {
  id: string
  kind: GlobalKind
  refId: string
  title: string
  subtitle: string
  icon?: string
  iconKey?: string
  titleIndices: number[]
  subtitleIndices: number[]
  score: number
}

export interface GlobalSection {
  key: GlobalKind
  title: string
  items: GlobalResult[]
}

export function buildGlobalSections(
  plugins: PluginSummary[],
  apps: AppEntry[] | null,
  commands: CommandEntry[],
  query: string
): GlobalSection[] {
  const trimmed = query.trim()
  const enabledPlugins = plugins.filter((plugin) => plugin.enabled)

  const pluginItems = enabledPlugins
    .map((plugin): GlobalResult | null => {
      const subtitle = plugin.keywords.slice(0, 5).join(' · ') || plugin.description
      const match = trimmed
        ? matchFields(trimmed, [plugin.name, subtitle, plugin.description])
        : { score: 0, indices: [[], [], []] }
      if (!match) return null
      return {
        id: `plugin:${plugin.id}`,
        kind: 'plugin',
        refId: plugin.id,
        title: plugin.name,
        subtitle,
        icon: plugin.icon,
        iconKey: plugin.iconKey ?? 'puzzle',
        titleIndices: match.indices[0] ?? [],
        subtitleIndices: match.indices[1] ?? [],
        score: match.score
      }
    })
    .filter((item): item is GlobalResult => item !== null)

  const appItems = (apps ?? [])
    .map((app): GlobalResult | null => {
      const keywordText = app.keywords.join(' ')
      const match = trimmed
        ? matchFields(trimmed, [app.name, app.subtitle, keywordText])
        : { score: 0, indices: [[], [], []] }
      if (!match) return null
      return {
        id: `app:${app.id}`,
        kind: 'app',
        refId: app.id,
        title: app.name,
        subtitle: app.subtitle,
        icon: app.icon,
        iconKey: app.iconKey,
        titleIndices: match.indices[0] ?? [],
        subtitleIndices: match.indices[1] ?? [],
        score: match.score
      }
    })
    .filter((item): item is GlobalResult => item !== null)

  const commandItems: GlobalResult[] = !trimmed
    ? []
    : commands
        .map((command): GlobalResult | null => {
          const match = matchFields(trimmed, [command.name, command.subtitle, command.keywords.join(' ')])
          if (!match) return null
          return {
            id: `command:${command.id}`,
            kind: 'command',
            refId: command.id,
            title: command.name,
            subtitle: command.subtitle,
            iconKey: command.iconKey,
            titleIndices: match.indices[0] ?? [],
            subtitleIndices: match.indices[1] ?? [],
            score: match.score
          }
        })
        .filter((item): item is GlobalResult => item !== null)

  if (!trimmed) {
    pluginItems.sort((a, b) => a.title.localeCompare(b.title, 'zh-CN'))
    appItems.sort((a, b) => a.title.localeCompare(b.title, 'zh-CN'))
  } else {
    pluginItems.sort((a, b) => b.score - a.score)
    appItems.sort((a, b) => b.score - a.score)
    commandItems.sort((a, b) => b.score - a.score)
  }

  const allSections: Array<GlobalSection & { best: number }> = [
    { key: 'plugin', title: '插件', items: pluginItems, best: pluginItems[0]?.score ?? -1 },
    { key: 'app', title: '应用', items: appItems, best: appItems[0]?.score ?? -1 },
    { key: 'command', title: '命令', items: commandItems, best: commandItems[0]?.score ?? -1 }
  ]
  const sections = allSections.filter((section) => section.items.length > 0)

  if (trimmed) {
    sections.sort((a, b) => {
      const pluginBias = (section: GlobalSection): number => (section.key === 'plugin' ? 0.5 : 0)
      return b.best + pluginBias(b) - (a.best + pluginBias(a))
    })
  }

  return sections.map(({ key, title, items }) => ({ key, title, items }))
}

export function flattenSections(sections: GlobalSection[]): GlobalResult[] {
  return sections.flatMap((section) => section.items)
}

export interface ClipboardRowModel {
  item: ClipboardItem
  titleText: string
  titleIndices: number[]
  subtitle: string
  subtitleIndices: number[]
  meta: string
  score: number
}

export function buildClipboardRows(items: ClipboardItem[], query: string): ClipboardRowModel[] {
  const trimmed = query.trim()
  const rows: ClipboardRowModel[] = []

  for (const item of items) {
    const label = kindLabel(item.kind)
    const titleText =
      item.kind === 'text' ? (item.text ?? item.title) : item.kind === 'video' ? '视频' : item.kind === 'image' ? '图片' : '音乐'
    const subtitle = item.fileName ?? ''
    const meta = `${label} · ${relativeTime(item.createdAt)}`

    let titleIndices: number[] = []
    let subtitleIndices: number[] = []
    let score = 0
    if (trimmed) {
      const match = matchFields(trimmed, [titleText, subtitle, label, item.fileName ?? ''])
      if (!match) continue
      titleIndices = match.indices[0] ?? []
      subtitleIndices = match.indices[1] ?? []
      score = match.score
    }

    rows.push({ item, titleText, titleIndices, subtitle, subtitleIndices, meta, score })
  }

  if (trimmed) rows.sort((a, b) => b.score - a.score)
  return rows
}

export interface BookmarkRowModel {
  bookmark: ChromeBookmark
  displayUrl: string
  titleIndices: number[]
  urlIndices: number[]
  folderIndices: number[]
  score: number
}

export function buildBookmarkRows(bookmarks: ChromeBookmark[], query: string): BookmarkRowModel[] {
  const trimmed = query.trim()
  const rows: BookmarkRowModel[] = []

  for (const bookmark of bookmarks) {
    const displayUrl = bookmark.url.replace(/^https?:\/\//, '').replace(/\/$/, '')
    const folderText = bookmark.folder.join(' / ')
    let titleIndices: number[] = []
    let urlIndices: number[] = []
    let folderIndices: number[] = []
    let score = 0

    if (trimmed) {
      const match = matchFields(trimmed, [bookmark.title, bookmark.url, folderText])
      if (!match) continue
      titleIndices = match.indices[0] ?? []
      urlIndices = match.indices[1] ?? []
      folderIndices = match.indices[2] ?? []
      score = match.score
    }

    rows.push({
      bookmark,
      displayUrl,
      titleIndices,
      urlIndices,
      folderIndices,
      score
    })
  }

  if (trimmed) rows.sort((a, b) => b.score - a.score)
  return rows
}

export interface ExternalRowModel {
  id: string
  title: string
  subtitle?: string
  badge?: string
  icon?: string
  iconKey?: string
  titleIndices: number[]
  subtitleIndices: number[]
}

export function buildExternalRows(
  items: Array<{
    id: string
    title: string
    subtitle?: string
    badge?: string
    icon?: string
    iconKey?: string
  }>,
  query: string
): ExternalRowModel[] {
  return items.map((item) => {
    const match = query.trim()
      ? matchFields(query, [item.title, item.subtitle ?? '', item.badge ?? ''])
      : null
    return {
      ...item,
      titleIndices: match?.indices[0] ?? [],
      subtitleIndices: match?.indices[1] ?? []
    }
  })
}

export function mediaUrl(path: string | undefined): string | undefined {
  if (!path) return undefined
  if (path.startsWith('data:') || path.startsWith('blob:') || path.startsWith('http')) return path
  return `flashcast-media://media/${encodeURIComponent(path)}`
}

export function simpleMatchIndices(query: string, text: string): number[] {
  return fuzzyMatch(query, text)?.indices ?? []
}
