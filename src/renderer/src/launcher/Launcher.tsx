import { useEffect, useMemo, useRef, useState } from 'react'
import {
  DESKTOP_WALLPAPER_SCHEME,
  type AppEntry,
  type BootstrapData,
  type EditorWorkspace,
  type ChromeBookmarksPluginPage,
  type ClipboardItem,
  type ExternalPluginItem,
  type ExternalPluginPage,
  type PluginPage,
  type PluginSettingsPage,
  type PluginSummary,
  type PluginTrigger,
  type VscodeLikeWorkspacePluginPage
} from '@shared/types'
import {
  clearAllAccelerator,
  eventMatchesAccelerator,
  keycapParts,
  primaryActionModifier
} from '@shared/shortcuts'
import { api, isMock, previewWallpaper } from '../bridge'
import { SkeletonRows, Toast, useToast } from '../components/ui'
import { useUpdateState } from '../hooks/useUpdateState'
import { applyTheme, themeById, upsertThemeDefinition } from '../theme'
import { BookmarksView } from './BookmarksView'
import { ClipboardView } from './ClipboardView'
import { ExternalPluginView } from './ExternalPluginView'
import { Footer, type FooterHint } from './Footer'
import { GlobalSearchView } from './GlobalSearchView'
import { HelpOverlay } from './HelpOverlay'
import { PluginSettingsView } from './PluginSettingsView'
import { SearchBar } from './SearchBar'
import { VscodeLikeWorkspaceSettingsView } from './VscodeLikeWorkspaceSettingsView'
import { VscodeLikeWorkspaceView } from './VscodeLikeWorkspaceView'
import {
  buildBookmarkRows,
  buildClipboardRows,
  buildEditorRows,
  buildExternalRows,
  buildGlobalSections,
  buildWorkspaceRows,
  flattenSections,
  type BookmarkRowModel,
  type ClipboardRowModel,
  type EditorRowModel,
  type ExternalRowModel,
  type GlobalResult,
  type WorkspaceRowModel
} from './helpers'

const GLOBAL_PLACEHOLDER = '搜索应用、插件或命令'
// Keep in sync with the idle `.fc-panel` height in launcher.css (60px input + borders).
const PANEL_COLLAPSED_HEIGHT = 62
const ANIMATION = {
  openPanel: 470,
  closePanel: 170,
  closeHide: 180
}

interface PageError {
  message: string
}

export function Launcher(): React.JSX.Element {
  const params = useMemo(() => new URLSearchParams(window.location.search), [])
  const initialQuery = params.get('q') ?? ''
  const [bootstrap, setBootstrap] = useState<BootstrapData | null>(null)
  const [bootError, setBootError] = useState<string | null>(null)
  const [apps, setApps] = useState<AppEntry[] | null>(null)
  const [query, setQuery] = useState(initialQuery)
  const [selection, setSelection] = useState(0)
  const [activePluginId, setActivePluginId] = useState<string | null>(null)
  const [selectedEditorId, setSelectedEditorId] = useState<string | null>(null)
  const [activeScope, setActiveScope] = useState<{ editor: string; label?: string } | null>(null)
  const [pluginPage, setPluginPage] = useState<PluginPage | null>(null)
  const [pluginError, setPluginError] = useState<PageError | null>(null)
  const [pageLoading, setPageLoading] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [settingsPage, setSettingsPage] = useState<PluginSettingsPage | null>(null)
  const [settingsLoading, setSettingsLoading] = useState(false)
  const [settingsError, setSettingsError] = useState<string | null>(null)
  const [pluginLeaving, setPluginLeaving] = useState(false)
  const [externalItems, setExternalItems] = useState<ExternalPluginItem[] | null>(null)
  const [clipboardItems, setClipboardItems] = useState<ClipboardItem[]>([])
  const [clipboardMax, setClipboardMax] = useState(10)
  const [helpOpen, setHelpOpen] = useState(false)
  const [closing, setClosing] = useState(false)
  const [showNonce, setShowNonce] = useState(0)
  const [clearArmed, setClearArmed] = useState(false)
  const [deletingId, setDeletingId] = useState<string | null>(null)
  const [clipboardScrollNonce, setClipboardScrollNonce] = useState(0)
  const [loadedWallpaper, setLoadedWallpaper] = useState<string | null>(null)
  const [failedWallpaper, setFailedWallpaper] = useState<string | null>(null)
  const { toast, showToast, dismissToast } = useToast()
  const { update: updateState } = useUpdateState()

  const inputRef = useRef<HTMLInputElement>(null)
  const panelRef = useRef<HTMLElement>(null)
  const composingRef = useRef(false)
  const closingRef = useRef(false)
  const closeTokenRef = useRef(0)
  const activePluginIdRef = useRef<string | null>(null)
  const bootstrapRef = useRef<BootstrapData | null>(null)
  const clearTimerRef = useRef<number | null>(null)
  const pageRequestRef = useRef(0)
  const settingsRequestRef = useRef(0)
  const directClipboardRef = useRef(false)
  const pendingOpenRef = useRef<string | null>(null)
  const pendingDirectRef = useRef(false)
  const autoScenarioRef = useRef(false)
  const autoSettingsRef = useRef(false)
  const lastEntranceRef = useRef(0)

  activePluginIdRef.current = activePluginId
  bootstrapRef.current = bootstrap

  const platform = bootstrap?.platform ?? ((params.get('platform') as BootstrapData['platform']) || 'linux')
  const activePlugin: PluginSummary | null =
    bootstrap?.plugins.find((plugin) => plugin.id === activePluginId) ?? null

  const isClipboard = activePluginId === 'clipboard'
  const isBookmarks = activePluginId === 'chrome-bookmarks'
  const isVscodeLike = activePluginId === 'vscode-like-workspace'
  const isExternal =
    activePluginId !== null && !isClipboard && !isBookmarks && !isVscodeLike

  const isPluginView = activePluginId !== null
  const globalSections = useMemo(
    () =>
      buildGlobalSections(
        bootstrap?.plugins ?? [],
        apps,
        bootstrap?.commands ?? [],
        query
      ),
    [bootstrap?.plugins, bootstrap?.commands, apps, query]
  )
  const globalItems = useMemo(() => flattenSections(globalSections), [globalSections])

  const clipboardRows = useMemo(
    () => (isClipboard ? buildClipboardRows(clipboardItems, query) : []),
    [isClipboard, clipboardItems, query]
  )
  const bookmarkPage = pluginPage?.kind === 'bookmarks' ? pluginPage : null
  const bookmarkRows = useMemo(
    () => (isBookmarks && bookmarkPage ? buildBookmarkRows(bookmarkPage.bookmarks, query) : []),
    [isBookmarks, bookmarkPage, query]
  )
  const externalPage = pluginPage?.kind === 'external' ? pluginPage : null
  const externalRows = useMemo(
    () =>
      isExternal
        ? buildExternalRows(externalItems ?? externalPage?.items ?? [], query)
        : [],
    [isExternal, externalItems, externalPage, query]
  )

  const vscodePage: VscodeLikeWorkspacePluginPage | null =
    pluginPage?.kind === 'vscode-like-workspace' ? pluginPage : null
  const scopeEditorId = activeScope?.editor ?? selectedEditorId
  const scopeValid = scopeEditorId
    ? vscodePage
      ? vscodePage.variants.some((variant) => variant.id === scopeEditorId)
      : true
    : false
  const vscodeMode: 'picker' | 'list' = scopeEditorId && scopeValid ? 'list' : 'picker'
  const activeEditorId = vscodeMode === 'list' ? scopeEditorId : null
  const activeEditor =
    vscodePage?.variants.find((variant) => variant.id === activeEditorId) ?? null
  const workspaceRows = useMemo(
    () =>
      isVscodeLike && vscodePage && vscodeMode === 'list'
        ? buildWorkspaceRows(vscodePage.workspaces, query, activeEditorId)
        : [],
    [isVscodeLike, vscodePage, vscodeMode, query, activeEditorId]
  )
  const editorRows = useMemo(
    () =>
      isVscodeLike && vscodePage && vscodeMode === 'picker'
        ? buildEditorRows(vscodePage.variants, vscodePage.workspaces, query)
        : [],
    [isVscodeLike, vscodePage, vscodeMode, query]
  )

  const currentCount = settingsOpen
    ? 0
    : !isPluginView
      ? globalItems.length
      : isClipboard
        ? clipboardRows.length
        : isBookmarks
          ? bookmarkRows.length
          : isVscodeLike
            ? vscodeMode === 'picker'
              ? editorRows.length
              : workspaceRows.length
            : externalRows.length

  const currentIndex = Math.min(selection, Math.max(0, currentCount - 1))
  const selectedGlobal: GlobalResult | null = !isPluginView ? globalItems[currentIndex] ?? null : null
  const selectedClipboard: ClipboardRowModel | null = isClipboard ? clipboardRows[currentIndex] ?? null : null
  const selectedBookmark: BookmarkRowModel | null = isBookmarks ? bookmarkRows[currentIndex] ?? null : null
  const selectedEditor: EditorRowModel | null =
    isVscodeLike && vscodeMode === 'picker' ? editorRows[currentIndex] ?? null : null
  const selectedWorkspace: WorkspaceRowModel | null =
    isVscodeLike && vscodeMode === 'list' ? workspaceRows[currentIndex] ?? null : null
  const selectedExternal: ExternalRowModel | null = isExternal ? externalRows[currentIndex] ?? null : null

  const searchPlaceholder = (() => {
    if (closing) return GLOBAL_PLACEHOLDER
    if (settingsOpen) return settingsPage?.placeholder ?? '插件设置…'
    if (isVscodeLike && !pluginLeaving) {
      if (vscodeMode === 'picker') return '选择编辑器…'
      return activeEditor ? `在 ${activeEditor.name} 中搜索工作区…` : '搜索工作区…'
    }
    if (isPluginView && !pluginLeaving) {
      return pluginPage?.placeholder ?? (activePlugin ? `在${activePlugin.name}中搜索…` : '搜索…')
    }
    return GLOBAL_PLACEHOLDER
  })()

  const searchable = settingsOpen
    ? false
    : !isPluginView || pluginLeaving
      ? true
      : pluginPage
        ? pluginPage.searchable
        : activePlugin?.searchable ?? true

  const readOnly = helpOpen || closing || settingsOpen || (isPluginView && !searchable)

  const previewItem = selectedClipboard?.item ?? null

  /* ------------------------------ wallpaper ------------------------------ */

  // A transparent Electron window cannot sample the desktop behind it, so the
  // frosted backdrop is the current wallpaper: blurred and color-blended in CSS.
  const wallpaperTheme: 'dark' | 'light' = bootstrap?.theme === 'light' ? 'light' : 'dark'
  const wallpaperUrl = isMock
    ? previewWallpaper(wallpaperTheme)
    : `${DESKTOP_WALLPAPER_SCHEME}://current?theme=${wallpaperTheme}`


  /* ------------------------------- helpers ------------------------------- */

  const focusInput = (): void => {
    window.setTimeout(() => inputRef.current?.focus(), 0)
  }

  const moveSelection = (delta: number): void => {
    if (currentCount === 0) return
    setSelection((value) => {
      const safe = Math.min(value, currentCount - 1)
      return Math.min(currentCount - 1, Math.max(0, safe + delta))
    })
    setClipboardScrollNonce((value) => value + 1)
  }

  const setCurrentSelection = (index: number): void => {
    if (index >= 0 && index < currentCount) setSelection(index)
  }

  /* ------------------------------- loading ------------------------------- */

  const loadPluginPage = async (pluginId: string): Promise<void> => {
    const request = ++pageRequestRef.current
    setPageLoading(true)
    setPluginError(null)
    try {
      const page = await api.getPluginPage(pluginId)
      if (request !== pageRequestRef.current) return
      setPluginPage(page)
      setExternalItems(page.kind === 'external' ? page.items : null)
      if (page.kind === 'clipboard') {
        setClipboardItems(page.items)
        setClipboardMax(page.maxItems)
      }
    } catch (error) {
      if (request !== pageRequestRef.current) return
      setPluginError({ message: (error as Error).message })
    } finally {
      if (request === pageRequestRef.current) setPageLoading(false)
    }
  }

  const openPlugin = (
    pluginId: string,
    options?: {
      direct?: boolean
      scope?: { editor: string; label?: string } | null
      initialQuery?: string
    }
  ): void => {
    const data = bootstrapRef.current
    const plugin = data?.plugins.find((entry) => entry.id === pluginId)
    if (!data || !plugin || !plugin.enabled) {
      pendingOpenRef.current = pluginId
      pendingDirectRef.current = options?.direct === true
      return
    }
    if (pluginLeaving) return
    pageRequestRef.current += 1
    directClipboardRef.current = options?.direct === true && pluginId === 'clipboard'
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    setSettingsOpen(false)
    setSettingsPage(null)
    setPluginLeaving(false)
    setQuery(options?.initialQuery ?? '')
    setActiveScope(options?.scope ?? null)
    setSelectedEditorId(options?.scope?.editor ?? null)
    setSelection(0)
    setActivePluginId(pluginId)
    activePluginIdRef.current = pluginId
    setPluginPage(null)
    setPluginError(null)
    setExternalItems(null)
    void loadPluginPage(pluginId)
    focusInput()
  }

  const openPluginRef = useRef(openPlugin)
  openPluginRef.current = openPlugin

  const triggerIndex = useMemo(() => {
    const index = new Map<string, { pluginId: string; trigger: PluginTrigger }>()
    for (const plugin of bootstrap?.plugins ?? []) {
      if (!plugin.enabled) continue
      for (const trigger of plugin.triggers ?? []) {
        const keyword = trigger.keyword.toLowerCase()
        if (keyword && !index.has(keyword)) index.set(keyword, { pluginId: plugin.id, trigger })
      }
    }
    return index
  }, [bootstrap?.plugins])

  const handleQueryChange = (value: string): void => {
    if (!isPluginView && !settingsOpen && !composingRef.current) {
      const match = /^(\S+)\s+([\s\S]*)$/.exec(value)
      if (match) {
        const entry = triggerIndex.get(match[1].toLowerCase())
        if (entry) {
          const editor = entry.trigger.args?.editor
          openPluginRef.current(entry.pluginId, {
            scope: editor ? { editor, label: entry.trigger.label } : null,
            initialQuery: match[2]
          })
          return
        }
      }
    }
    setQuery(value)
    setSelection(0)
  }

  const exitPlugin = (): void => {
    if (!activePluginId || pluginLeaving) return
    pageRequestRef.current += 1
    settingsRequestRef.current += 1
    directClipboardRef.current = false
    setSettingsOpen(false)
    setSettingsPage(null)
    setSettingsError(null)
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    setQuery('')
    setActiveScope(null)
    setSelectedEditorId(null)
    setPluginLeaving(true)
    const leavingId = activePluginId
    window.setTimeout(() => {
      if (activePluginIdRef.current !== leavingId) return
      setActivePluginId(null)
      activePluginIdRef.current = null
      setPluginPage(null)
      setPluginError(null)
      setExternalItems(null)
      setPluginLeaving(false)
      setSelection(0)
      setActiveScope(null)
      setSelectedEditorId(null)
      focusInput()
    }, 240)
  }

  const exitPluginRef = useRef(exitPlugin)
  exitPluginRef.current = exitPlugin

  const openPluginSettings = async (): Promise<void> => {
    if (!activePluginId) return
    const request = ++settingsRequestRef.current
    setQuery('')
    setSelection(0)
    setSettingsError(null)
    setSettingsPage(null)
    setSettingsOpen(true)
    setSettingsLoading(true)
    focusInput()
    try {
      const page = await api.getPluginSettings(activePluginId)
      if (request !== settingsRequestRef.current) return
      setSettingsPage(page)
    } catch (error) {
      if (request !== settingsRequestRef.current) return
      setSettingsError((error as Error).message)
    } finally {
      if (request === settingsRequestRef.current) setSettingsLoading(false)
    }
  }

  const backToPlugin = (): void => {
    if (!activePluginId) return
    settingsRequestRef.current += 1
    setSettingsOpen(false)
    setSettingsPage(null)
    setSettingsError(null)
    setSelection(0)
    setQuery('')
    void loadPluginPage(activePluginId)
    focusInput()
  }

  const togglePluginSettings = (): void => {
    if (settingsOpen) backToPlugin()
    else void openPluginSettings()
  }

  const retryPlugin = (): void => {
    if (!activePluginId) return
    void loadPluginPage(activePluginId)
  }

  /* ------------------------------- actions ------------------------------- */

  const runClipboardActivation = async (itemId: string, paste: boolean): Promise<void> => {
    const result = await api.activateClipboard(itemId, { paste })
    if (!result.ok) {
      showToast({ message: result.error ?? '无法活跃剪切板项', tone: 'danger' })
      return
    }
    await closeLauncher(paste)
  }

  const runBookmarkActivation = async (row: BookmarkRowModel): Promise<void> => {
    const result = await api.openChromeBookmark({
      id: row.bookmark.id,
      url: row.bookmark.url,
      title: row.bookmark.title
    })
    if (!result.ok) {
      showToast({ message: result.error ?? '无法打开书签', tone: 'danger' })
      return
    }
    await closeLauncher(false)
  }

  const runWorkspaceActivation = async (
    workspace: EditorWorkspace,
    newWindow: boolean
  ): Promise<void> => {
    const result = await api.openEditorWorkspace(workspace.id, { newWindow })
    if (!result.ok) {
      showToast({ message: result.error ?? '无法打开工作区', tone: 'danger' })
      return
    }
    if (result.toast) showToast(result.toast)
    await closeLauncher(false)
  }

  const runWorkspaceReveal = async (workspace: EditorWorkspace): Promise<void> => {
    const result = await api.revealEditorWorkspace(workspace.id)
    if (!result.ok) {
      showToast({ message: result.error ?? '无法定位工作区', tone: 'danger' })
      return
    }
    if (result.toast) showToast(result.toast)
  }

  const runExternalActivation = async (row: ExternalRowModel, alt: boolean): Promise<void> => {
    if (!activePluginId) return
    const result = await api.activateExternal(activePluginId, row.id, { alt })
    if (!result.ok) {
      showToast({ message: result.error ?? '插件操作失败', tone: 'danger' })
      return
    }
    if (result.toast) showToast(result.toast)
    if (result.close) await closeLauncher(result.paste === true)
  }

  const activateCurrent = (alt: boolean): void => {
    if (settingsOpen) return
    if (!isPluginView) {
      const item = selectedGlobal
      if (!item) return
      if (item.kind === 'plugin') {
        const editor = item.scope?.editor
        openPlugin(item.refId, {
          scope: editor ? { editor, label: item.scopeLabel } : null
        })
        return
      }
      if (item.kind === 'app') {
        void api.launchApp(item.refId).then(async (result) => {
          if (!result.ok) showToast({ message: result.error ?? '启动应用失败', tone: 'danger' })
          else if (result.toast) showToast(result.toast)
          else await closeLauncher(false)
        })
        return
      }
      void api.runCommand(item.refId as 'settings' | 'check-updates' | 'refresh-apps' | 'quit').then(async (result) => {
        if (!result.ok) showToast({ message: result.error ?? '命令执行失败', tone: 'danger' })
        else if (result.toast) showToast(result.toast)
        else if (item.refId !== 'settings' && item.refId !== 'check-updates') {
          await closeLauncher(false)
        }
      })
      return
    }

    if (isVscodeLike) {
      if (vscodeMode === 'picker') {
        if (selectedEditor) {
          setSelectedEditorId(selectedEditor.variant.id)
          setQuery('')
          setSelection(0)
          focusInput()
        }
        return
      }
      if (selectedWorkspace) {
        void runWorkspaceActivation(selectedWorkspace.workspace, alt)
      }
      return
    }
    if (isClipboard) {
      if (selectedClipboard) void runClipboardActivation(selectedClipboard.item.id, alt)
      return
    }
    if (isBookmarks) {
      if (selectedBookmark) void runBookmarkActivation(selectedBookmark)
      return
    }
    if (selectedExternal) void runExternalActivation(selectedExternal, alt)
  }

  const deleteSelectedClipboard = (): void => {
    if (!isClipboard || !selectedClipboard) return
    const id = selectedClipboard.item.id
    setDeletingId(id)
    window.setTimeout(async () => {
      const result = await api.deleteClipboard(id)
      setDeletingId(null)
      if (!result.ok) showToast({ message: result.error ?? '删除失败', tone: 'danger' })
    }, 120)
  }

  const clearClipboard = (): void => {
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    void api.clearClipboard().then((result) => {
      if (!result.ok) showToast({ message: result.error ?? '清空失败', tone: 'danger' })
      else {
        setClipboardItems([])
        if (result.toast) showToast(result.toast)
      }
    })
  }

  const toggleClear = (): void => {
    if (!isClipboard || clipboardItems.length === 0) {
      if (isClipboard) showToast({ message: '剪切板还没有记录', tone: 'default' })
      return
    }
    if (clearArmed) {
      clearClipboard()
      return
    }
    setClearArmed(true)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    clearTimerRef.current = window.setTimeout(() => setClearArmed(false), 3000)
  }

  const disarmClear = (): void => {
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    clearTimerRef.current = null
  }

  const openHelp = (): void => {
    setHelpOpen(true)
  }

  const closeHelp = (): void => {
    setHelpOpen(false)
    focusInput()
  }

  /* ------------------------------- window ------------------------------- */

  const setIdlePanelSurface = (): void => {
    const panel = panelRef.current
    if (!panel) return
    for (const animation of panel.getAnimations()) animation.cancel()
    panel.style.height = `${PANEL_COLLAPSED_HEIGHT}px`
    panel.style.opacity = '0'
  }

  const playEntrance = (): void => {
    const panel = panelRef.current
    if (!panel) return
    const now = Date.now()
    if (now - lastEntranceRef.current < 180) return
    lastEntranceRef.current = now
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    for (const animation of panel.getAnimations()) animation.cancel()
    panel.style.opacity = '0'
    panel.style.height = reduced ? '100%' : `${PANEL_COLLAPSED_HEIGHT}px`

    if (reduced) {
      const entrance = panel.animate([{ opacity: 0 }, { opacity: 1 }], {
        duration: 120,
        easing: 'ease-out',
        fill: 'both'
      })
      entrance.onfinish = () => {
        panel.style.height = '100%'
        panel.style.opacity = '1'
        entrance.cancel()
      }
      setShowNonce((value) => value + 1)
      return
    }

    const target = panel.parentElement?.clientHeight ?? 480
    const overshoot = target > 200 ? 5 : 2

    // Phase 1 (0–18%): only the 60px search bar exists, in place.
    // Phase 2 (18–100%): the shell grows downward like liquid leaving the
    // input; the inner layout is already final height, so no content reflows.
    // The panel settles with a ≤5px overshoot before resting at 100%.
    const entrance = panel.animate(
      [
        { opacity: 0, height: `${PANEL_COLLAPSED_HEIGHT}px`, offset: 0 },
        {
          opacity: 1,
          height: `${PANEL_COLLAPSED_HEIGHT}px`,
          offset: 0.18,
          easing: 'cubic-bezier(0.32, 0, 0.22, 1)'
        },
        {
          opacity: 1,
          height: `${target + overshoot}px`,
          offset: 0.74,
          easing: 'cubic-bezier(0.4, 0, 0.2, 1)'
        },
        {
          opacity: 1,
          height: `${target - 1}px`,
          offset: 0.9,
          easing: 'ease-out'
        },
        { opacity: 1, height: `${target}px`, offset: 1 }
      ],
      {
        duration: ANIMATION.openPanel,
        easing: 'linear',
        fill: 'both'
      }
    )
    entrance.onfinish = () => {
      panel.style.height = '100%'
      panel.style.opacity = '1'
      entrance.cancel()
    }
    setShowNonce((value) => value + 1)
  }

  const resetForShow = (): void => {
    closingRef.current = false
    closeTokenRef.current += 1
    setClosing(false)
    pageRequestRef.current += 1
    settingsRequestRef.current += 1
    setPageLoading(false)
    setSettingsLoading(false)
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    directClipboardRef.current = false
    lastEntranceRef.current = 0
    setIdlePanelSurface()
    setSettingsOpen(false)
    setSettingsPage(null)
    setSettingsError(null)
    setPluginPage(null)
    setPluginError(null)
    setPluginLeaving(false)
    setActivePluginId(null)
    activePluginIdRef.current = null
    setExternalItems(null)
    setQuery('')
    setSelection(0)
    window.setTimeout(() => {
      playEntrance()
      focusInput()
    }, 20)
  }

  const closeLauncher = async (paste = false): Promise<void> => {
    if (closingRef.current) return
    closingRef.current = true
    const closeToken = ++closeTokenRef.current
    setClosing(true)
    const panel = panelRef.current
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (panel) {
      const computed = window.getComputedStyle(panel)
      const startHeight = computed.height
      const startOpacity = computed.opacity
      for (const animation of panel.getAnimations()) animation.cancel()
      if (reduced) {
        panel.animate([{ opacity: Number(startOpacity) }, { opacity: 0 }], {
          duration: 120,
          fill: 'both'
        })
      } else {
        // Retract the body into the search bar, then fade the bar out. Reverse
        // of the entrance so closing still reads as the same body of liquid.
        panel.animate(
          [
            { opacity: startOpacity, height: startHeight, offset: 0 },
            {
              opacity: startOpacity,
              height: startHeight,
              offset: 0.16,
              easing: 'cubic-bezier(0.4, 0, 1, 1)'
            },
            {
              opacity: startOpacity,
              height: `${PANEL_COLLAPSED_HEIGHT}px`,
              offset: 0.72,
              easing: 'cubic-bezier(0.4, 0, 1, 1)'
            },
            { opacity: 0, height: `${PANEL_COLLAPSED_HEIGHT}px`, offset: 1 }
          ],
          {
            duration: ANIMATION.closePanel,
            easing: 'linear',
            fill: 'both'
          }
        )
      }
    }
    window.setTimeout(() => {
      if (closeToken !== closeTokenRef.current) return
      setIdlePanelSurface()
      api.hideLauncher({ paste })
      closingRef.current = false
      setClosing(false)
    }, reduced ? 120 : ANIMATION.closeHide)
  }

  const closeLauncherRef = useRef(closeLauncher)
  closeLauncherRef.current = closeLauncher

  const settingsShortcut = platform === 'darwin' ? '⌘,' : 'Ctrl+,'

  // Open the settings window from inside the launcher: retract the panel first
  // so the always-on-top launcher never covers the window that takes focus.
  const openSettingsWindow = (): void => {
    void closeLauncher(false)
    window.setTimeout(() => {
      void api.openSettings()
    }, 140)
  }

  const runUpdateAction = (): void => {
    const state = updateState
    if (!state) return
    if (state.phase === 'available') {
      void api
        .downloadUpdate()
        .catch((error: Error) => showToast({ message: error.message, tone: 'danger' }))
      return
    }
    if (state.phase === 'ready') {
      void api.installUpdate().then((result) => {
        if (!result.ok) showToast({ message: result.error ?? '安装更新失败', tone: 'danger' })
      })
    }
  }

  /* ------------------------------- effects ------------------------------- */

  useEffect(() => {
    let cancelled = false
    void (async () => {
      try {
        const data = await api.getBootstrap()
        if (cancelled) return
        setBootstrap(data)
        setClipboardMax((current) => (current > 0 ? current : 10))
        applyTheme(themeById(data.themes, data.themeId), data.theme, data.platform)
        document.documentElement.dataset.preview = isMock ? 'true' : 'false'
        if (data.role === 'settings') return
        const preview = params.get('preview')
        if (isMock && preview && !autoScenarioRef.current) {
          autoScenarioRef.current = true
          if (preview === 'help') {
            window.setTimeout(() => setHelpOpen(true), 260)
          } else if (preview.startsWith('clipboard') || preview.startsWith('bookmarks')) {
            const pluginId = preview.startsWith('clipboard') ? 'clipboard' : 'chrome-bookmarks'
            autoSettingsRef.current = preview.endsWith('-settings')
            window.setTimeout(() => openPluginRef.current(pluginId), 160)
          }
        }
        if (isMock && params.get('preview') === 'empty') {
          setQuery(params.get('q') ?? 'fbb zzz')
        }
      } catch (error) {
        if (!cancelled) setBootError((error as Error).message)
      }
    })()
    void api
      .getApps()
      .then((entries) => {
        if (!cancelled) setApps(entries)
      })
      .catch(() => {
        if (!cancelled) setApps([])
      })
    const entranceTimer = window.setTimeout(() => {
      if (isMock || document.visibilityState === 'visible') {
        playEntrance()
        focusInput()
      }
    }, 30)
    if (isMock) {
      return () => {
        cancelled = true
        window.clearTimeout(entranceTimer)
      }
    }
    return () => {
      cancelled = true
      window.clearTimeout(entranceTimer)
    }
  }, [])

  useEffect(() => {
    if (!bootstrap) return
    bootstrapRef.current = bootstrap
    applyTheme(themeById(bootstrap.themes, bootstrap.themeId), bootstrap.theme, bootstrap.platform)
    if (pendingOpenRef.current) {
      const id = pendingOpenRef.current
      const direct = pendingDirectRef.current
      pendingOpenRef.current = null
      pendingDirectRef.current = false
      openPluginRef.current(id, { direct })
    }
  }, [bootstrap])

  useEffect(() => {
    if (!autoSettingsRef.current) return
    if (!activePluginId || !pluginPage) return
    autoSettingsRef.current = false
    void openPluginSettings()
  }, [activePluginId, pluginPage])

  useEffect(() => {
    if (!isExternal || !activePluginId || !externalPage || !externalPage.searchable) return
    let cancelled = false
    const timer = window.setTimeout(() => {
      void api
        .searchExternal(activePluginId, query)
        .then((items) => {
          if (!cancelled) setExternalItems(items)
        })
        .catch((error: Error) => {
          if (!cancelled) showToast({ message: error.message, tone: 'danger' })
        })
    }, 80)
    return () => {
      cancelled = true
      window.clearTimeout(timer)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isExternal, activePluginId, externalPage, query])

  useEffect(() => {
    setSelection(0)
  }, [query, activePluginId, settingsOpen])

  useEffect(() => {
    const offs: Array<() => void> = []
    offs.push(
      api.on('launcher:shown', () => {
        resetForShow()
      })
    )
    offs.push(api.on('launcher:close-request', () => void closeLauncherRef.current(false)))
    offs.push(
      api.on('launcher:open-plugin', ({ pluginId }) => {
        // Only the global clipboard hotkey enters through this channel. Mark it
        // so esc can close the launcher directly instead of leaving the plugin.
        if (!bootstrapRef.current) {
          pendingOpenRef.current = pluginId
          pendingDirectRef.current = true
          return
        }
        openPluginRef.current(pluginId, { direct: true })
      })
    )
    offs.push(
      api.on('clipboard:changed', (items) => {
        setClipboardItems(items)
      })
    )
    offs.push(
      api.on('plugins:changed', (plugins) => {
        const data = bootstrapRef.current
        if (!data) return
        setBootstrap({ ...data, plugins })
        const active = activePluginIdRef.current
        if (active) {
          const plugin = plugins.find((entry) => entry.id === active)
          if (!plugin || !plugin.enabled) {
            // eslint-disable-next-line no-use-before-define
            exitPluginRef.current()
          }
        }
      })
    )
    offs.push(
      api.on(
        'theme:changed',
        ({
          theme,
          setting,
          themeId,
          locked,
          supportedModes,
          definition,
          requestedThemeId,
          fallbackReason
        }) => {
          const data = bootstrapRef.current
          applyTheme(definition, theme, data?.platform ?? platform)
          if (!data) return
          const next: BootstrapData = {
            ...data,
            theme,
            themeId,
            themeLocked: locked,
            themeSupportedModes: supportedModes,
            themeFallback: fallbackReason
              ? { requestedId: requestedThemeId, reason: fallbackReason }
              : null,
            themes: upsertThemeDefinition(data.themes, definition),
            prefs: { ...data.prefs, theme: setting, themeId: requestedThemeId }
          }
          // Keep the ref in step so a `themes:changed` emitted right after this
          // event re-applies the new theme instead of the previous one.
          bootstrapRef.current = next
          setBootstrap(next)
        }
      )
    )
    offs.push(
      api.on('themes:changed', ({ themes, errors }) => {
        const data = bootstrapRef.current
        if (!data) return
        const next: BootstrapData = { ...data, themes, themeErrors: errors }
        bootstrapRef.current = next
        setBootstrap(next)
        const active = themeById(themes, data.themeId)
        if (active) applyTheme(active, data.theme, data.platform)
      })
    )
    offs.push(
      api.on('prefs:changed', (prefs) => {
        const data = bootstrapRef.current
        if (data) setBootstrap({ ...data, prefs })
      })
    )
    offs.push(
      api.on('workspace:changed', (workspace) => {
        const data = bootstrapRef.current
        if (data) setBootstrap({ ...data, workspace })
      })
    )
    offs.push(
      api.on('shortcuts:changed', ({ shortcuts, errors }) => {
        const data = bootstrapRef.current
        if (data) setBootstrap({ ...data, shortcuts, shortcutErrors: errors })
      })
    )
    offs.push(api.on('toast', (message) => showToast(message)))
    return () => {
      for (const off of offs) off()
    }
  }, [])

  useEffect(() => {
    return () => {
      if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  /* ------------------------------- keyboard ------------------------------- */

  const focusSettingField = (delta: number): void => {
    const nodes = Array.from(
      document.querySelectorAll<HTMLElement>('[data-settings-field]:not([disabled])')
    ).filter((node) => node.offsetParent !== null)
    if (nodes.length === 0) return
    const active = document.activeElement as HTMLElement | null
    const current = active ? nodes.indexOf(active) : -1
    const next = current < 0 ? 0 : (current + delta + nodes.length) % nodes.length
    nodes[next]?.focus()
  }

  const keyHandler = (event: KeyboardEvent): void => {
    if (event.isComposing || composingRef.current) return
    if (closingRef.current) return
    const target = event.target as HTMLElement | null
    const key = event.key
    const tag = target?.tagName
    const inEditable =
      tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || target?.isContentEditable === true
    const inButton = Boolean(target?.closest('button, a, [role="button"]'))

    if (toast) dismissToast()

    if (helpOpen) {
      if (key === 'Escape' || key === '?') {
        event.preventDefault()
        closeHelp()
      } else if (key === 'Tab') {
        event.preventDefault()
      }
      return
    }

    if (clearArmed) {
      if (key === 'Escape') {
        event.preventDefault()
        disarmClear()
        return
      }
      if (eventMatchesAccelerator(clearAllAccelerator(platform), event)) {
        event.preventDefault()
        toggleClear()
        return
      }
      disarmClear()
    }

    const primaryModifier = platform === 'darwin' ? event.metaKey : event.ctrlKey
    const secondaryModifier = platform === 'darwin' ? event.ctrlKey : event.metaKey
    if (
      primaryModifier &&
      !secondaryModifier &&
      !event.altKey &&
      !event.shiftKey &&
      key === ','
    ) {
      event.preventDefault()
      openSettingsWindow()
      return
    }

    if (settingsOpen) {
      if (key === 'Escape') {
        event.preventDefault()
        backToPlugin()
        return
      }
      if (key === '?' && !event.ctrlKey && !event.metaKey && !event.altKey) {
        event.preventDefault()
        openHelp()
        return
      }
      if (event.ctrlKey && !event.altKey && !event.metaKey && (key === 'j' || key === 'k')) {
        event.preventDefault()
        focusSettingField(key === 'j' ? 1 : -1)
        return
      }
      if (event.ctrlKey && !event.altKey && !event.metaKey && (key === 'h' || key === 'l')) {
        event.preventDefault()
        if (key === 'h') focusInput()
        else focusSettingField(1)
        return
      }
      return
    }

    if (key === '?' && !event.ctrlKey && !event.metaKey && !event.altKey) {
      event.preventDefault()
      openHelp()
      return
    }

    if (key === 'Escape') {
      if (isPluginView) {
        if (clearArmed) {
          event.preventDefault()
          disarmClear()
          return
        }
        event.preventDefault()
        if (isVscodeLike && vscodeMode === 'list' && !activeScope) {
          setSelectedEditorId(null)
          setQuery('')
          setSelection(0)
          focusInput()
          return
        }
        // The clipboard hotkey is a direct shortcut into the plugin, so esc
        // should close the launcher instead of peeling back to the global
        // search page first. Entering from the global page still returns there.
        if (isClipboard && directClipboardRef.current) {
          void closeLauncher(false)
          return
        }
        exitPlugin()
        return
      }
      event.preventDefault()
      void closeLauncher(false)
      return
    }

    if (event.ctrlKey && !event.altKey && !event.metaKey && (key === 'j' || key === 'k')) {
      if (!isPluginView || currentCount > 0) {
        event.preventDefault()
        moveSelection(key === 'j' ? 1 : -1)
      }
      return
    }

    if ((key === 'ArrowDown' || key === 'ArrowUp') && !event.metaKey && !event.ctrlKey && !event.altKey) {
      if (!isPluginView || currentCount > 0) {
        event.preventDefault()
        moveSelection(key === 'ArrowDown' ? 1 : -1)
      }
      return
    }

    if (event.ctrlKey && !event.altKey && !event.metaKey && key === 'Backspace' && isClipboard) {
      event.preventDefault()
      deleteSelectedClipboard()
      return
    }

    if (isClipboard && eventMatchesAccelerator(clearAllAccelerator(platform), event)) {
      event.preventDefault()
      toggleClear()
      return
    }

    if (key === 'Enter' && primaryModifier && !event.altKey && !event.shiftKey) {
      if (isVscodeLike && vscodeMode === 'list' && selectedWorkspace) {
        event.preventDefault()
        void runWorkspaceReveal(selectedWorkspace.workspace)
        return
      }
    }

    if (key === 'Enter') {
      if (inButton && !inEditable) return
      if (isPluginView || currentCount > 0) {
        event.preventDefault()
        activateCurrent(event.altKey)
      }
      return
    }

    if (
      !isPluginView &&
      key.length === 1 &&
      !event.ctrlKey &&
      !event.metaKey &&
      !event.altKey &&
      !inEditable
    ) {
      event.preventDefault()
      focusInput()
      setQuery((value) => value + key)
    }
  }

  const keyHandlerRef = useRef(keyHandler)
  keyHandlerRef.current = keyHandler

  useEffect(() => {
    const listener = (event: KeyboardEvent): void => keyHandlerRef.current(event)
    window.addEventListener('keydown', listener)
    return () => window.removeEventListener('keydown', listener)
  }, [])

  /* ------------------------------- rendering ------------------------------- */

  const footerHints: FooterHint[] = (() => {
    if (settingsOpen) {
      return [
        { keys: ['esc'], label: '返回' },
        { keys: ['Tab'], label: '切换字段' },
        { keys: [platform === 'darwin' ? '⌃' : 'Ctrl', 'J/K'], label: '移动字段' }
      ]
    }
    if (isPluginView && !pluginLeaving) {
      if (isVscodeLike) {
        if (vscodeMode === 'picker') {
          return [
            { keys: ['↵'], label: '选择编辑器' },
            { keys: ['esc'], label: '退出' }
          ]
        }
        return [
          { keys: ['↵'], label: '打开' },
          { keys: [platform === 'darwin' ? '⌥' : 'Alt', '↵'], label: '新窗口' },
          { keys: [primaryActionModifier(platform), '↵'], label: '显示位置' },
          { keys: ['esc'], label: activeScope ? '退出' : '返回编辑器' }
        ]
      }
      if (isClipboard) {
        return [
          { keys: ['↵'], label: '置顶并关闭' },
          { keys: [primaryActionModifier(platform), '↵'], label: '粘贴' },
          { keys: keycapParts('Control+Backspace', platform), label: '删除' },
          { keys: ['esc'], label: directClipboardRef.current ? '关闭' : '退出' }
        ]
      }
      if (isBookmarks) {
        return [
          { keys: ['↵'], label: '在 Chrome 打开' },
          { keys: [platform === 'darwin' ? '⌃' : 'Ctrl', 'J/K'], label: '移动' },
          { keys: ['esc'], label: '退出' }
        ]
      }
      return [
        { keys: ['↵'], label: activePlugin?.primaryAction ?? '打开' },
        { keys: [platform === 'darwin' ? '⌃' : 'Ctrl', 'J/K'], label: '移动' },
        { keys: ['esc'], label: '退出' }
      ]
    }
    return [
      { keys: ['↵'], label: '打开' },
      { keys: ['esc'], label: '关闭' },
      { keys: [platform === 'darwin' ? '⌃' : 'Ctrl', 'J/K'], label: '移动' }
    ]
  })()

  const showPluginSkeleton = isPluginView && !settingsOpen && (pageLoading || !pluginPage)
  const pluginTitle = activePlugin?.name ?? '插件'
  const settingsTitle = settingsPage?.title ?? `${activePlugin?.name ?? '插件'}设置`

  const renderPluginContent = (): React.JSX.Element => {
    if (settingsOpen) {
      if (settingsLoading || (!settingsPage && !settingsError)) {
        return (
          <div className="fc-plugin">
            <div className="fc-plugin-head">
              <h1 className="fc-plugin-title">{settingsTitle}</h1>
            </div>
            <div className="fc-plugin-body">
              <SkeletonRows count={3} />
            </div>
          </div>
        )
      }
      if (settingsError) {
        return (
          <div className="fc-plugin">
            <div className="fc-plugin-head">
              <button type="button" className="fc-back-button" onClick={backToPlugin}>
                返回
              </button>
              <h1 className="fc-plugin-title">{settingsTitle}</h1>
            </div>
            <div className="fc-plugin-body">
              <div className="fc-error-card" role="alert">
                <div className="fc-error-heading">设置读取失败</div>
                <div className="fc-error-description">{settingsError}</div>
                <div className="fc-error-actions">
                  <button type="button" className="fc-btn" onClick={() => void openPluginSettings()}>
                    重试
                  </button>
                  <button type="button" className="fc-btn fc-btn--ghost" onClick={backToPlugin}>
                    返回插件
                  </button>
                </div>
              </div>
            </div>
          </div>
        )
      }
      if (!settingsPage) return <div />
      if (settingsPage.kind === 'vscode-like-workspace-settings') {
        return (
          <VscodeLikeWorkspaceSettingsView
            key={settingsPage.pluginId}
            page={settingsPage}
            query={query}
            onBack={backToPlugin}
            onSaved={(page) => setSettingsPage(page)}
            onError={(message) => showToast({ message, tone: 'danger' })}
          />
        )
      }
      return (
        <PluginSettingsView
          key={settingsPage.pluginId}
          page={settingsPage}
          onBack={backToPlugin}
          onSaved={(page) => setSettingsPage(page)}
          onError={(message) => showToast({ message, tone: 'danger' })}
        />
      )
    }

    if (pluginError) {
      return (
        <div className="fc-plugin">
          <div className="fc-plugin-head">
            <h1 className="fc-plugin-title">{pluginTitle}</h1>
          </div>
          <div className="fc-plugin-body">
            <div className="fc-error-card" role="alert">
              <div className="fc-error-heading">插件加载失败</div>
              <div className="fc-error-description">{pluginError.message}</div>
              <div className="fc-error-actions">
                <button type="button" className="fc-btn" onClick={retryPlugin}>
                  重试
                </button>
              </div>
            </div>
          </div>
        </div>
      )
    }

    if (showPluginSkeleton) {
      return (
        <div className="fc-plugin">
          <div className="fc-plugin-head">
            <h1 className="fc-plugin-title">{pluginTitle}</h1>
          </div>
          <div className="fc-plugin-body">
            <SkeletonRows count={4} />
          </div>
        </div>
      )
    }

    if (isClipboard && pluginPage?.kind === 'clipboard') {
      return (
        <ClipboardView
          rows={clipboardRows}
          totalCount={clipboardItems.length}
          maxItems={clipboardMax}
          query={query}
          selectedIndex={currentIndex}
          scrollNonce={clipboardScrollNonce}
          deletingId={deletingId}
          previewItem={previewItem}
          onHover={setCurrentSelection}
          onActivate={(index) => {
            const row = clipboardRows[index]
            if (row) void runClipboardActivation(row.item.id, false)
          }}
        />
      )
    }

    if (isBookmarks) {
      return (
        <BookmarksView
          page={bookmarkPage}
          rows={bookmarkRows}
          query={query}
          selectedIndex={currentIndex}
          loading={pageLoading}
          onHover={setCurrentSelection}
          onActivate={(index) => {
            const row = bookmarkRows[index]
            if (row) void runBookmarkActivation(row)
          }}
          onSettings={() => void openPluginSettings()}
          onRetry={retryPlugin}
        />
      )
    }

    if (isVscodeLike) {
      return (
        <VscodeLikeWorkspaceView
          page={vscodePage}
          mode={vscodeMode}
          editorRows={editorRows}
          workspaceRows={workspaceRows}
          activeVariant={activeEditor}
          query={query}
          selectedIndex={currentIndex}
          loading={pageLoading}
          canGoBack={vscodeMode === 'list' && !activeScope}
          onHover={setCurrentSelection}
          onActivate={(index) => {
            if (vscodeMode === 'picker') {
              const row = editorRows[index]
              if (!row) return
              setSelectedEditorId(row.variant.id)
              setQuery('')
              setSelection(0)
              focusInput()
              return
            }
            const row = workspaceRows[index]
            if (row) void runWorkspaceActivation(row.workspace, false)
          }}
          onBack={() => {
            setSelectedEditorId(null)
            setQuery('')
            setSelection(0)
            focusInput()
          }}
          onSettings={() => void openPluginSettings()}
          onRetry={retryPlugin}
        />
      )
    }

    if (externalPage) {
      return (
        <ExternalPluginView
          page={externalPage}
          rows={externalRows}
          query={query}
          selectedIndex={currentIndex}
          loading={pageLoading}
          onHover={setCurrentSelection}
          onActivate={(index) => {
            const row = externalRows[index]
            if (row) void runExternalActivation(row, false)
          }}
        />
      )
    }

    return (
      <div className="fc-plugin">
        <div className="fc-plugin-head">
          <h1 className="fc-plugin-title">{pluginTitle}</h1>
        </div>
        <div className="fc-plugin-body">
          <SkeletonRows count={3} />
        </div>
      </div>
    )
  }

  return (
    <div
      className="fc-root"
      data-platform={platform}
      data-closing={closing || undefined}
      aria-busy={!bootstrap}
    >
      <div
        className="fc-dismiss-layer"
        onClick={() => void closeLauncher(false)}
        aria-hidden="true"
      />
      <div className="fc-panel-anchor">
        <section
          className="fc-panel"
          ref={panelRef}
          role="dialog"
          aria-modal="true"
          aria-label="Flashcast"
        >
          {failedWallpaper !== wallpaperUrl ? (
            <img
              key={wallpaperUrl}
              className={`fc-wallpaper${loadedWallpaper === wallpaperUrl ? ' is-ready' : ''}`}
              src={wallpaperUrl}
              alt=""
              aria-hidden="true"
              draggable={false}
              onLoad={() => setLoadedWallpaper(wallpaperUrl)}
              onError={() => setFailedWallpaper(wallpaperUrl)}
            />
          ) : null}
          <div className="fc-panel-inner">
            <SearchBar
              inputRef={inputRef}
              value={query}
              placeholder={searchPlaceholder}
              activePlugin={activePlugin}
              settingsActive={settingsOpen}
              searchable={searchable}
              readOnly={readOnly}
              update={updateState}
              settingsShortcut={settingsShortcut}
              onChange={handleQueryChange}
              onClear={() => {
                setQuery('')
                setSelection(0)
                focusInput()
              }}
              onPluginClick={togglePluginSettings}
              onOpenSettings={openSettingsWindow}
              onUpdateAction={runUpdateAction}
              onCompositionStart={() => {
                composingRef.current = true
              }}
              onCompositionEnd={() => {
                composingRef.current = false
              }}
              activeDescendant={
                !settingsOpen && currentCount > 0 ? `fc-result-${currentIndex}` : undefined
              }
            />

            <div className="fc-views">
              <div className={`fc-global-view${isPluginView && !pluginLeaving ? ' is-obscured' : ''}`}>
                {bootError ? (
                  <div className="fc-error-card" role="alert">
                    <div className="fc-error-heading">Flashcast 无法初始化</div>
                    <div className="fc-error-description">{bootError}</div>
                    <div className="fc-error-actions">
                      <button type="button" className="fc-btn" onClick={() => window.location.reload()}>
                        重试
                      </button>
                    </div>
                  </div>
                ) : (
                  <GlobalSearchView
                    sections={globalSections}
                    items={globalItems}
                    selectedIndex={currentIndex}
                    appsLoading={apps === null}
                    query={query}
                    showNonce={showNonce}
                    onHover={setCurrentSelection}
                    onActivate={(index) => {
                      setSelection(index)
                      const item = globalItems[index]
                      if (!item) return
                      if (item.kind === 'plugin') {
                        const editor = item.scope?.editor
                        openPlugin(item.refId, {
                          scope: editor ? { editor, label: item.scopeLabel } : null
                        })
                      }
                      else if (item.kind === 'app') {
                        void api.launchApp(item.refId).then(async (result) => {
                          if (!result.ok) showToast({ message: result.error ?? '启动应用失败', tone: 'danger' })
                          else await closeLauncher(false)
                        })
                      } else {
                        void api.runCommand(item.refId as 'settings' | 'check-updates' | 'refresh-apps' | 'quit').then(async (result) => {
                          if (!result.ok) showToast({ message: result.error ?? '命令执行失败', tone: 'danger' })
                          else if (result.toast) showToast(result.toast)
                        })
                      }
                    }}
                  />
                )}
              </div>
              {isPluginView ? (
                <div className={`fc-plugin-view${pluginLeaving ? ' is-leaving' : ''}`} key={`${activePluginId}-${settingsOpen ? 'settings' : 'main'}`}>
                  {renderPluginContent()}
                </div>
              ) : null}
            </div>

            <Footer
              hints={footerHints}
              clearCount={clearArmed ? clipboardItems.length : null}
              clearKeys={keycapParts(clearAllAccelerator(platform), platform)}
              onHelp={openHelp}
            />

            <HelpOverlay
              open={helpOpen}
              onClose={closeHelp}
              platform={platform}
              launcherAccelerator={bootstrap?.prefs.hotkeys.launcher ?? 'Alt+Space'}
              clipboardAccelerator={bootstrap?.prefs.hotkeys.openClipboard ?? 'Control+Alt+C'}
              activePluginId={activePluginId}
              settingsActive={settingsOpen}
            />

            <Toast toast={toast} />
          </div>
        </section>
      </div>
    </div>
  )
}
