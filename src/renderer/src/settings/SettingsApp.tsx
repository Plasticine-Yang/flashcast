import { FolderGit2, Info, Keyboard, Puzzle, SlidersHorizontal } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { BootstrapData } from '@shared/types'
import { isUpdateActionable } from '@shared/update'
import { api } from '../bridge'
import { Toast, useToast } from '../components/ui'
import { useUpdateState } from '../hooks/useUpdateState'
import { AboutPage } from './AboutPage'
import { GeneralPage } from './GeneralPage'
import { PluginsPage } from './PluginsPage'
import { ShortcutsPage } from './ShortcutsPage'
import { WorkspacePage } from './WorkspacePage'
import { InlineSpinner, Logo, SyncChip } from './components'

type PageId = 'general' | 'plugins' | 'workspace' | 'shortcuts' | 'about'

const NAV: Array<{
  id: PageId
  label: string
  icon: React.JSX.Element
}> = [
  { id: 'general', label: '通用', icon: <SlidersHorizontal size={16} strokeWidth={1.5} aria-hidden="true" /> },
  { id: 'plugins', label: '插件', icon: <Puzzle size={16} strokeWidth={1.5} aria-hidden="true" /> },
  { id: 'workspace', label: '工作区', icon: <FolderGit2 size={16} strokeWidth={1.5} aria-hidden="true" /> },
  { id: 'shortcuts', label: '快捷键', icon: <Keyboard size={16} strokeWidth={1.5} aria-hidden="true" /> },
  { id: 'about', label: '关于', icon: <Info size={16} strokeWidth={1.5} aria-hidden="true" /> }
]

export function SettingsApp(): React.JSX.Element {
  const params = new URLSearchParams(window.location.search)
  const initialPage = (params.get('page') as PageId) || 'general'
  const [page, setPage] = useState<PageId>(NAV.some((item) => item.id === initialPage) ? initialPage : 'general')
  const [data, setData] = useState<BootstrapData | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [theme, setTheme] = useState<'dark' | 'light'>('dark')
  const scrollRef = useRef<HTMLDivElement>(null)
  const autoCheckedRef = useRef(false)
  const { toast, showToast, dismissToast } = useToast()
  const { update, loaded: updateLoaded } = useUpdateState()

  const applyTheme = (nextTheme: 'dark' | 'light', platform: BootstrapData['platform']): void => {
    setTheme(nextTheme)
    document.documentElement.dataset.theme = nextTheme
    document.documentElement.dataset.platform = platform
    document.documentElement.dataset.preview = window.flashcast ? 'false' : 'true'
  }

  const load = async (): Promise<void> => {
    try {
      const next = await api.getBootstrap()
      setData(next)
      setError(null)
      applyTheme(next.theme, next.platform)
    } catch (loadError) {
      setError((loadError as Error).message)
    }
  }

  useEffect(() => {
    void load()
    const offs: Array<() => void> = [
      api.on('plugins:changed', (plugins) => {
        setData((current) => (current ? { ...current, plugins } : current))
      }),
      api.on('prefs:changed', (prefs) => {
        setData((current) => (current ? { ...current, prefs } : current))
      }),
      api.on('workspace:changed', (workspace) => {
        setData((current) => (current ? { ...current, workspace } : current))
      }),
      api.on('shortcuts:changed', ({ shortcuts, errors }) => {
        setData((current) => (current ? { ...current, shortcuts, shortcutErrors: errors } : current))
      }),
      api.on('theme:changed', ({ theme: resolved, setting }) => {
        setTheme(resolved)
        document.documentElement.dataset.theme = resolved
        setData((current) => (current ? { ...current, theme: resolved, prefs: { ...current.prefs, theme: setting } } : current))
      }),
      api.on('toast', (message) => showToast(message))
    ]
    return () => {
      for (const off of offs) off()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 })
  }, [page])

  // Opening "关于" is an explicit request for update information: check when the
  // cached state is missing or older than ten minutes, once per visit.
  useEffect(() => {
    if (page !== 'about') {
      autoCheckedRef.current = false
      return
    }
    if (!updateLoaded || autoCheckedRef.current) return
    autoCheckedRef.current = true
    if (update?.phase === 'downloading' || update?.phase === 'ready') return
    if (update?.checkedAt && Date.now() - update.checkedAt < 10 * 60 * 1000) return
    void api.checkForUpdates().catch(() => undefined)
  }, [page, updateLoaded, update])

  useEffect(() => {
    if (!toast) return
    const dismiss = (): void => dismissToast()
    window.addEventListener('keydown', dismiss)
    return () => window.removeEventListener('keydown', dismiss)
  }, [toast, dismissToast])

  const onData = (next: BootstrapData): void => {
    setData(next)
    applyTheme(next.theme, next.platform)
  }

  if (error) {
    return (
      <div className="settings-root settings-root--message">
        <div className="settings-loading-card">
          <Logo size={44} />
          <h1>无法打开设置</h1>
          <p>{error}</p>
          <button type="button" className="fc-btn" onClick={() => void load()}>
            重试
          </button>
        </div>
      </div>
    )
  }

  if (!data) {
    return (
      <div className="settings-root settings-root--message">
        <div className="settings-loading-card">
          <Logo size={44} />
          <InlineSpinner label="正在加载设置…" />
        </div>
      </div>
    )
  }

  return (
    <div className="settings-root" data-platform={data.platform} data-theme={theme}>
      <div className="settings-titlebar" aria-hidden="true" />
      <aside className="settings-sidebar">
        <div className="settings-brand">
          <Logo size={30} />
          <div className="settings-brand-copy">
            <span className="settings-brand-name">Flashcast</span>
            <span className="settings-brand-version">v{data.version}</span>
          </div>
        </div>
        <nav className="settings-nav" aria-label="设置导航">
          {NAV.map((item) => {
            const showUpdateDot = item.id === 'about' && isUpdateActionable(update?.phase ?? 'idle')
            return (
              <button
                key={item.id}
                type="button"
                className={`settings-nav-item${page === item.id ? ' is-active' : ''}`}
                aria-current={page === item.id ? 'page' : undefined}
                aria-label={showUpdateDot ? `${item.label}（有可用更新）` : undefined}
                onClick={() => setPage(item.id)}
              >
                {item.icon}
                <span>{item.label}</span>
                {item.id === 'plugins' ? (
                  <span className="settings-nav-count">{data.plugins.length}</span>
                ) : null}
                {showUpdateDot ? <span className="settings-nav-dot" aria-hidden="true" /> : null}
              </button>
            )
          })}
        </nav>
        <button
          type="button"
          className="settings-sidebar-workspace"
          onClick={() => setPage('workspace')}
          title={data.workspace.path}
        >
          <div className="settings-sidebar-workspace-head">
            <span>工作区</span>
            <SyncChip state={data.workspace.syncState} />
          </div>
          <code>{data.workspace.path}</code>
        </button>
      </aside>

      <main className="settings-main">
        <div className="settings-main-scroll fc-scroll" ref={scrollRef}>
          {page === 'general' ? (
            <GeneralPage
              data={data}
              onData={onData}
              onToast={(message, tone) => showToast({ message, tone })}
              onNavigate={(next) => setPage(next as PageId)}
            />
          ) : null}
          {page === 'plugins' ? (
            <PluginsPage
              data={data}
              onData={onData}
              onToast={(message, tone) => showToast({ message, tone })}
            />
          ) : null}
          {page === 'workspace' ? (
            <WorkspacePage
              data={data}
              onData={onData}
              onToast={(message, tone) => showToast({ message, tone })}
            />
          ) : null}
          {page === 'shortcuts' ? (
            <ShortcutsPage
              data={data}
              onData={onData}
              onToast={(message, tone) => showToast({ message, tone })}
            />
          ) : null}
          {page === 'about' ? (
            <AboutPage
              data={data}
              update={update}
              onToast={(message, tone) => showToast({ message, tone })}
            />
          ) : null}
        </div>
      </main>

      <Toast toast={toast} />
      {toast && <button type="button" className="sr-only" onClick={dismissToast}>关闭提示</button>}
    </div>
  )
}
