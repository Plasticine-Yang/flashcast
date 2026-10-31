import { useEffect, useMemo, useRef, useState, type SyntheticEvent } from 'react'
import type {
  AppEntry,
  BootstrapData,
  ChromeBookmarksPluginPage,
  ClipboardItem,
  ExternalPluginItem,
  ExternalPluginPage,
  PluginPage,
  PluginSettingsPage,
  PluginSummary
} from '@shared/types'
import {
  clearAllAccelerator,
  eventMatchesAccelerator,
  keycapParts,
  primaryActionModifier
} from '@shared/shortcuts'
import { api, isMock } from '../bridge'
import { SkeletonRows, Toast, useToast } from '../components/ui'
import { BookmarksView } from './BookmarksView'
import { ClipboardView, type PreviewState } from './ClipboardView'
import { ExternalPluginView } from './ExternalPluginView'
import { Footer, type FooterHint } from './Footer'
import { GlobalSearchView } from './GlobalSearchView'
import { HelpOverlay } from './HelpOverlay'
import { PluginSettingsView } from './PluginSettingsView'
import { SearchBar } from './SearchBar'
import {
  buildBookmarkRows,
  buildClipboardRows,
  buildExternalRows,
  buildGlobalSections,
  flattenSections,
  type BookmarkRowModel,
  type ClipboardRowModel,
  type ExternalRowModel,
  type GlobalResult
} from './helpers'

const GLOBAL_PLACEHOLDER = '搜索应用、插件或命令'
const ANIMATION = {
  openPanel: 300,
  openScrim: 180,
  closePanel: 180,
  closeScrim: 120,
  previewHold: 1500
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
  const [previewMounted, setPreviewMounted] = useState(false)
  const [previewOpen, setPreviewOpen] = useState(false)
  const [previewPinned, setPreviewPinned] = useState(false)
  const [holdVisible, setHoldVisible] = useState(false)
  const [holdNonce, setHoldNonce] = useState(0)
  const { toast, showToast, dismissToast } = useToast()

  const inputRef = useRef<HTMLInputElement>(null)
  const panelRef = useRef<HTMLElement>(null)
  const scrimRef = useRef<HTMLDivElement>(null)
  const composingRef = useRef(false)
  const closingRef = useRef(false)
  const activePluginIdRef = useRef<string | null>(null)
  const bootstrapRef = useRef<BootstrapData | null>(null)
  const previewTimerRef = useRef<number | null>(null)
  const previewUnmountRef = useRef<number | null>(null)
  const clearTimerRef = useRef<number | null>(null)
  const pageRequestRef = useRef(0)
  const settingsRequestRef = useRef(0)
  const previewSuppressedRef = useRef(false)
  const pendingOpenRef = useRef<string | null>(null)
  const autoScenarioRef = useRef(false)
  const autoSettingsRef = useRef(false)
  const lastEntranceRef = useRef(0)
  const popupTimerRef = useRef<number | null>(null)
  const lastMouseRef = useRef<{ x: number; y: number } | null>(null)

  activePluginIdRef.current = activePluginId
  bootstrapRef.current = bootstrap

  const platform = bootstrap?.platform ?? ((params.get('platform') as BootstrapData['platform']) || 'linux')
  const activePlugin: PluginSummary | null =
    bootstrap?.plugins.find((plugin) => plugin.id === activePluginId) ?? null

  const isClipboard = activePluginId === 'clipboard'
  const isBookmarks = activePluginId === 'chrome-bookmarks'
  const isExternal =
    activePluginId !== null && !isClipboard && !isBookmarks

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

  const currentCount = settingsOpen
    ? 0
    : !isPluginView
      ? globalItems.length
      : isClipboard
        ? clipboardRows.length
        : isBookmarks
          ? bookmarkRows.length
          : externalRows.length

  const currentIndex = Math.min(selection, Math.max(0, currentCount - 1))
  const selectedGlobal: GlobalResult | null = !isPluginView ? globalItems[currentIndex] ?? null : null
  const selectedClipboard: ClipboardRowModel | null = isClipboard ? clipboardRows[currentIndex] ?? null : null
  const selectedBookmark: BookmarkRowModel | null = isBookmarks ? bookmarkRows[currentIndex] ?? null : null
  const selectedExternal: ExternalRowModel | null = isExternal ? externalRows[currentIndex] ?? null : null

  const searchPlaceholder = (() => {
    if (closing) return GLOBAL_PLACEHOLDER
    if (settingsOpen) return settingsPage?.placeholder ?? '插件设置…'
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
  const preview: PreviewState = {
    mounted: previewMounted,
    open: previewOpen,
    pinned: previewPinned,
    item: previewItem
  }

  /* ------------------------------- helpers ------------------------------- */

  const focusInput = (): void => {
    window.setTimeout(() => inputRef.current?.focus(), 0)
  }

  const applyTheme = (theme: 'dark' | 'light', platformId = platform): void => {
    document.documentElement.dataset.theme = theme
    document.documentElement.dataset.platform = platformId
  }

  const clearPreviewTimers = (): void => {
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current)
    previewTimerRef.current = null
    if (previewUnmountRef.current) window.clearTimeout(previewUnmountRef.current)
    previewUnmountRef.current = null
    if (popupTimerRef.current) window.clearTimeout(popupTimerRef.current)
    popupTimerRef.current = null
  }

  const closePreviewVisual = (): void => {
    setPreviewOpen(false)
    setHoldVisible(false)
    if (previewUnmountRef.current) window.clearTimeout(previewUnmountRef.current)
    previewUnmountRef.current = window.setTimeout(() => setPreviewMounted(false), 340)
  }

  const closePreview = (suppress = true): void => {
    previewSuppressedRef.current = suppress
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current)
    previewTimerRef.current = null
    setPreviewPinned(false)
    closePreviewVisual()
  }

  const armPreviewTimer = (): void => {
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current)
    setHoldNonce((value) => value + 1)
    setHoldVisible(true)
    previewTimerRef.current = window.setTimeout(() => {
      setPreviewMounted(true)
      setPreviewPinned(false)
      setHoldVisible(false)
      popupTimerRef.current = window.setTimeout(() => setPreviewOpen(true), 20)
    }, ANIMATION.previewHold)
  }

  const handleClipboardActivity = (event?: SyntheticEvent): void => {
    if (!isClipboard || settingsOpen || helpOpen) return
    if (previewPinned) return
    if (event && event.type === 'mousemove') {
      const native = event.nativeEvent as MouseEvent
      const last = lastMouseRef.current
      if (last && Math.hypot(native.clientX - last.x, native.clientY - last.y) < 8) return
      lastMouseRef.current = { x: native.clientX, y: native.clientY }
    }
    previewSuppressedRef.current = false
    closePreviewVisual()
    armPreviewTimer()
  }

  const moveSelection = (delta: number): void => {
    if (currentCount === 0) return
    setSelection((value) => {
      const safe = Math.min(value, currentCount - 1)
      return Math.min(currentCount - 1, Math.max(0, safe + delta))
    })
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

  const openPlugin = (pluginId: string): void => {
    const data = bootstrapRef.current
    const plugin = data?.plugins.find((entry) => entry.id === pluginId)
    if (!data || !plugin || !plugin.enabled) {
      pendingOpenRef.current = pluginId
      return
    }
    if (pluginLeaving) return
    pageRequestRef.current += 1
    previewSuppressedRef.current = false
    clearPreviewTimers()
    setPreviewMounted(false)
    setPreviewOpen(false)
    setPreviewPinned(false)
    setHoldVisible(false)
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    setSettingsOpen(false)
    setSettingsPage(null)
    setPluginLeaving(false)
    setQuery('')
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

  const exitPlugin = (): void => {
    if (!activePluginId || pluginLeaving) return
    pageRequestRef.current += 1
    settingsRequestRef.current += 1
    setSettingsOpen(false)
    setSettingsPage(null)
    setSettingsError(null)
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    closePreview(false)
    setQuery('')
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
    previewSuppressedRef.current = true
    clearPreviewTimers()
    setPreviewOpen(false)
    setPreviewPinned(false)
    setPreviewMounted(false)
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
    previewSuppressedRef.current = false
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
        openPlugin(item.refId)
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
      void api.runCommand(item.refId as 'settings' | 'refresh-apps' | 'quit').then(async (result) => {
        if (!result.ok) showToast({ message: result.error ?? '命令执行失败', tone: 'danger' })
        else if (result.toast) showToast(result.toast)
        else if (item.refId !== 'settings') await closeLauncher(false)
      })
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
    previewSuppressedRef.current = true
    closePreview(true)
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
        closePreview(true)
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
    previewSuppressedRef.current = true
    setHelpOpen(true)
  }

  const closeHelp = (): void => {
    setHelpOpen(false)
    focusInput()
  }

  /* ------------------------------- window ------------------------------- */

  const playEntrance = (): void => {
    const panel = panelRef.current
    const scrim = scrimRef.current
    if (!panel || !scrim) return
    const now = Date.now()
    if (now - lastEntranceRef.current < 180) return
    lastEntranceRef.current = now
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (reduced) {
      panel.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 120, fill: 'both' })
      scrim.animate([{ opacity: 0 }, { opacity: 1 }], { duration: 120, fill: 'both' })
      return
    }
    scrim.animate([{ opacity: 0 }, { opacity: 1 }], {
      duration: ANIMATION.openScrim,
      easing: 'cubic-bezier(0.16, 1, 0.3, 1)',
      fill: 'both'
    })
    panel.animate(
      [
        { opacity: 0, transform: 'translateY(-8px) scale(0.96)', filter: 'blur(8px)' },
        { opacity: 1, transform: 'translateY(0) scale(1)', filter: 'blur(0px)' }
      ],
      {
        duration: ANIMATION.openPanel,
        easing: 'cubic-bezier(0.16, 1, 0.3, 1)',
        fill: 'both'
      }
    )
    setShowNonce((value) => value + 1)
  }

  const resetForShow = (): void => {
    closingRef.current = false
    setClosing(false)
    pageRequestRef.current += 1
    settingsRequestRef.current += 1
    setPageLoading(false)
    setSettingsLoading(false)
    setHelpOpen(false)
    setClearArmed(false)
    if (clearTimerRef.current) window.clearTimeout(clearTimerRef.current)
    clearPreviewTimers()
    setPreviewMounted(false)
    setPreviewOpen(false)
    setPreviewPinned(false)
    setHoldVisible(false)
    previewSuppressedRef.current = false
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
    setShowNonce((value) => value + 1)
    window.setTimeout(() => {
      playEntrance()
      focusInput()
    }, 20)
  }

  const closeLauncher = async (paste = false): Promise<void> => {
    if (closingRef.current) return
    closingRef.current = true
    setClosing(true)
    const panel = panelRef.current
    const scrim = scrimRef.current
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    if (panel && scrim) {
      if (reduced) {
        panel.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 80, fill: 'both' })
        scrim.animate([{ opacity: 1 }, { opacity: 0 }], { duration: 80, fill: 'both' })
      } else {
        panel.animate(
          [
            { opacity: 1, transform: 'translateY(0) scale(1)' },
            { opacity: 0, transform: 'translateY(-6px) scale(0.98)' }
          ],
          {
            duration: ANIMATION.closePanel,
            easing: 'cubic-bezier(0.4, 0, 1, 1)',
            fill: 'both'
          }
        )
        scrim.animate([{ opacity: 1 }, { opacity: 0 }], {
          duration: ANIMATION.closeScrim,
          fill: 'both'
        })
      }
    }
    window.setTimeout(() => {
      api.hideLauncher({ paste })
      closingRef.current = false
      setClosing(false)
    }, reduced ? 90 : 170)
  }

  const closeLauncherRef = useRef(closeLauncher)
  closeLauncherRef.current = closeLauncher

  /* ------------------------------- effects ------------------------------- */

  useEffect(() => {
    let cancelled = false
    applyTheme(isMock ? ((params.get('theme') as 'dark' | 'light') || 'dark') : 'dark')
    void (async () => {
      try {
        const data = await api.getBootstrap()
        if (cancelled) return
        setBootstrap(data)
        setClipboardMax((current) => (current > 0 ? current : 10))
        applyTheme(data.theme, data.platform)
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
    applyTheme(bootstrap.theme, bootstrap.platform)
    if (pendingOpenRef.current) {
      const id = pendingOpenRef.current
      pendingOpenRef.current = null
      openPluginRef.current(id)
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
        if (!bootstrapRef.current) {
          pendingOpenRef.current = pluginId
          return
        }
        openPluginRef.current(pluginId)
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
      api.on('theme:changed', ({ theme, setting }) => {
        applyTheme(theme, bootstrapRef.current?.platform ?? platform)
        const data = bootstrapRef.current
        if (data) setBootstrap({ ...data, theme, prefs: { ...data.prefs, theme: setting } })
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

  // Timer-driven hover preview. Selection changes, filtering, and leaving the
  // plugin reset the 1.5s dwell timer; a pinned preview survives selection moves.
  useEffect(() => {
    if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current)
    previewTimerRef.current = null
    if (!isClipboard || settingsOpen || helpOpen || !selectedClipboard) {
      setHoldVisible(false)
      if (previewOpen) closePreviewVisual()
      return
    }
    if (previewPinned) {
      setHoldVisible(false)
      return
    }
    closePreviewVisual()
    if (previewSuppressedRef.current) {
      setHoldVisible(false)
      return
    }
    armPreviewTimer()
    return () => {
      if (previewTimerRef.current) window.clearTimeout(previewTimerRef.current)
      previewTimerRef.current = null
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isClipboard, settingsOpen, helpOpen, previewPinned, selectedClipboard?.item.id, query])

  useEffect(() => {
    return () => {
      clearPreviewTimers()
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
        if (previewOpen || previewMounted) {
          event.preventDefault()
          closePreview(true)
          return
        }
        if (clearArmed) {
          event.preventDefault()
          disarmClear()
          return
        }
        event.preventDefault()
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

    if (event.ctrlKey && !event.altKey && !event.metaKey && (key === 'h' || key === 'l')) {
      if (isClipboard) {
        event.preventDefault()
        if (key === 'l' && selectedClipboard) {
          clearPreviewTimers()
          previewSuppressedRef.current = false
          setPreviewPinned(true)
          setPreviewMounted(true)
          setHoldVisible(false)
          popupTimerRef.current = window.setTimeout(() => setPreviewOpen(true), 20)
        } else {
          closePreview(true)
        }
      } else if (!isPluginView && key === 'l' && inEditable) {
        event.preventDefault()
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
      if (isClipboard) {
        if (previewOpen) {
          return [
            { keys: keycapParts('Control+H', platform), label: '收起预览' },
            { keys: ['↵'], label: '置顶并关闭' },
            { keys: [primaryActionModifier(platform), '↵'], label: '粘贴' }
          ]
        }
        return [
          { keys: ['↵'], label: '置顶并关闭' },
          { keys: [primaryActionModifier(platform), '↵'], label: '粘贴' },
          { keys: keycapParts('Control+Backspace', platform), label: '删除' }
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
          deletingId={deletingId}
          preview={preview}
          holdVisible={holdVisible && !previewOpen && !previewPinned}
          holdKey={holdNonce}
          onHover={setCurrentSelection}
          onActivate={(index) => {
            const row = clipboardRows[index]
            if (row) void runClipboardActivation(row.item.id, false)
          }}
          onSettings={() => void openPluginSettings()}
          onPreviewClose={() => closePreview(true)}
          onActivity={handleClipboardActivity}
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
          onSettings={() => void openPluginSettings()}
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
        className="fc-scrim"
        ref={scrimRef}
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
          <SearchBar
            inputRef={inputRef}
            value={query}
            placeholder={searchPlaceholder}
            activePlugin={activePlugin}
            settingsActive={settingsOpen}
            searchable={searchable}
            readOnly={readOnly}
            onChange={(value) => {
              setQuery(value)
              setSelection(0)
              previewSuppressedRef.current = false
            }}
            onClear={() => {
              setQuery('')
              setSelection(0)
              focusInput()
            }}
            onPluginClick={togglePluginSettings}
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
                    if (item.kind === 'plugin') openPlugin(item.refId)
                    else if (item.kind === 'app') {
                      void api.launchApp(item.refId).then(async (result) => {
                        if (!result.ok) showToast({ message: result.error ?? '启动应用失败', tone: 'danger' })
                        else await closeLauncher(false)
                      })
                    } else {
                      void api.runCommand(item.refId as 'settings' | 'refresh-apps' | 'quit').then(async (result) => {
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
        </section>
      </div>
    </div>
  )
}
