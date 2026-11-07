import { AlertTriangle, Check, Trash2 } from 'lucide-react'
import { useEffect, useMemo, useRef, useState } from 'react'
import type { EditorWorkspace } from '@shared/types'
import { matchFields } from '@shared/fuzzy'
import { relativeTime } from '@shared/time'
import { api } from '../bridge'
import { Highlight, scrollRowIntoView } from '../components/ui'

type Filter = 'all' | 'missing' | 'remote'

export interface CleanupTarget {
  variantId: string
  name: string
  running: boolean
  workspaces: EditorWorkspace[]
}

/**
 * Batch deletion of one editor's history. Selection is local state; the prune
 * itself happens in the main process, which refuses to write while the editor
 * is running (the editor would just overwrite the files on exit).
 */
export function WorkspaceCleanupView({
  target,
  query,
  onBack,
  onDeleted,
  onError
}: {
  target: CleanupTarget
  query: string
  onBack: () => void
  onDeleted: () => Promise<void>
  onError: (message: string) => void
}): React.JSX.Element {
  const [filter, setFilter] = useState<Filter>('all')
  const [selected, setSelected] = useState<Set<string>>(() => new Set())
  const [cursor, setCursor] = useState(0)
  const [armed, setArmed] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const scrollRef = useRef<HTMLDivElement>(null)

  const workspaces = target.workspaces
  const running = target.running

  useEffect(() => {
    setSelected(new Set())
    setCursor(0)
    setArmed(false)
    setError(null)
  }, [target])

  useEffect(() => {
    if (!armed) return
    const timer = window.setTimeout(() => setArmed(false), 4000)
    return () => window.clearTimeout(timer)
  }, [armed])

  const visible = useMemo(() => {
    const trimmed = query.trim()
    return workspaces.filter((workspace) => {
      if (filter === 'missing' && workspace.exists) return false
      if (filter === 'remote' && !workspace.remote) return false
      if (!trimmed) return true
      return Boolean(matchFields(trimmed, [workspace.name, workspace.displayPath, workspace.path]))
    })
  }, [workspaces, filter, query])

  const selectedCount = selected.size
  const missingCount = workspaces.filter((workspace) => !workspace.exists).length
  const remoteCount = workspaces.filter((workspace) => workspace.remote !== null).length
  const activeIndex = Math.min(cursor, Math.max(0, visible.length - 1))

  useEffect(() => {
    scrollRowIntoView(scrollRef.current, activeIndex)
  }, [activeIndex, visible.length])

  const toggle = (id: string): void => {
    setArmed(false)
    setSelected((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  }

  const remove = async (): Promise<void> => {
    if (busy || selectedCount === 0) return
    if (!armed) {
      setArmed(true)
      return
    }
    setBusy(true)
    setError(null)
    try {
      const result = await api.deleteEditorWorkspaces(target.variantId, [...selected])
      if (!result.ok) {
        const message = result.error ?? '删除失败'
        setError(message)
        onError(message)
        setArmed(false)
        return
      }
      setSelected(new Set())
      setArmed(false)
      await onDeleted()
    } catch (caught) {
      const message = (caught as Error).message
      setError(message)
      onError(message)
    } finally {
      setBusy(false)
    }
  }

  // The launcher handles keys in the bubble phase on window; a capture-phase
  // listener lets Esc peel back to settings instead of leaving them, and keeps
  // Space/arrows available for selection.
  const handlerRef = useRef<(event: KeyboardEvent) => void>(() => {})
  handlerRef.current = (event: KeyboardEvent): void => {
    if (event.isComposing) return
    if (event.ctrlKey || event.metaKey || event.altKey) return
    const key = event.key

    const stop = (): void => {
      event.preventDefault()
      event.stopPropagation()
    }

    if (key === 'Escape') {
      stop()
      if (selectedCount > 0) setSelected(new Set())
      else onBack()
      return
    }
    if (key === 'ArrowDown' || key === 'j') {
      stop()
      setCursor((value) => Math.min(visible.length - 1, value + 1))
      return
    }
    if (key === 'ArrowUp' || key === 'k') {
      stop()
      setCursor((value) => Math.max(0, value - 1))
      return
    }
    if (key === ' ') {
      stop()
      const row = visible[activeIndex]
      if (row) toggle(row.id)
      return
    }
    if (key === 'Enter') {
      stop()
      void remove()
    }
  }

  useEffect(() => {
    const listener = (event: KeyboardEvent): void => handlerRef.current(event)
    window.addEventListener('keydown', listener, true)
    return () => window.removeEventListener('keydown', listener, true)
  }, [])

  return (
    <div className="fc-plugin-settings fc-workspace-cleanup">
      <div className="fc-settings-head">
        <button type="button" className="fc-back-button" onClick={onBack}>
          返回
        </button>
        <h1 className="fc-settings-title">{target.name} · 工作区</h1>
        <div className="fc-plugin-head-actions">
          <button
            type="button"
            className={`fc-btn${armed ? ' fc-btn--danger' : ''}`}
            disabled={busy || selectedCount === 0}
            onClick={() => void remove()}
          >
            {busy ? (
              <span className="fc-spinner" aria-hidden="true" />
            ) : (
              <Trash2 size={13} strokeWidth={1.75} aria-hidden="true" />
            )}
            {armed
              ? `再按一次删除 ${selectedCount} 项`
              : `删除选中${selectedCount > 0 ? ` (${selectedCount})` : ''}`}
          </button>
        </div>
      </div>

      <div className="fc-cleanup-toolbar">
        <button
          type="button"
          className={`fc-chip${filter === 'missing' ? ' is-active' : ''}`}
          aria-pressed={filter === 'missing'}
          onClick={() => {
            setFilter((value) => (value === 'missing' ? 'all' : 'missing'))
            setCursor(0)
          }}
        >
          只看不存在
          {missingCount > 0 ? <span className="fc-chip-count">{missingCount}</span> : null}
        </button>
        <button
          type="button"
          className={`fc-chip${filter === 'remote' ? ' is-active' : ''}`}
          aria-pressed={filter === 'remote'}
          onClick={() => {
            setFilter((value) => (value === 'remote' ? 'all' : 'remote'))
            setCursor(0)
          }}
        >
          只看远程
          {remoteCount > 0 ? <span className="fc-chip-count">{remoteCount}</span> : null}
        </button>
        <span className="fc-cleanup-spacer" />
        <button type="button" className="fc-cleanup-link" onClick={() => setSelected(new Set(visible.map((row) => row.id)))}>
          全选
        </button>
        <button
          type="button"
          className="fc-cleanup-link"
          disabled={selectedCount === 0}
          onClick={() => {
            setArmed(false)
            setSelected(new Set())
          }}
        >
          清空
        </button>
        <button
          type="button"
          className="fc-cleanup-link"
          disabled={missingCount === 0}
          onClick={() =>
            setSelected(
              new Set(workspaces.filter((row) => !row.exists).map((row) => row.id))
            )
          }
        >
          选中全部不存在
        </button>
        <button
          type="button"
          className="fc-cleanup-link"
          disabled={remoteCount === 0}
          onClick={() =>
            setSelected(new Set(workspaces.filter((row) => row.remote).map((row) => row.id)))
          }
        >
          选中全部远程
        </button>
        {running ? (
          <span className="fc-cleanup-warning">
            <AlertTriangle size={12} strokeWidth={1.75} aria-hidden="true" />
            编辑器运行中，退出后才能删除
          </span>
        ) : null}
      </div>

      {error ? (
        <div className="fc-inline-error fc-cleanup-error" role="alert">
          <AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" />
          {error}
        </div>
      ) : null}

      <div
        className="fc-cleanup-list fc-scroll"
        ref={scrollRef}
        role="listbox"
        aria-label={`${target.name} 工作区`}
      >
        {visible.length === 0 ? (
          <div className="fc-empty-inline">
            {filter === 'missing'
              ? '没有路径不存在的工作区'
              : filter === 'remote'
                ? '没有远程工作区'
                : '没有匹配的工作区'}
          </div>
        ) : (
          visible.map((workspace, index) => {
            const checked = selected.has(workspace.id)
            return (
              <div
                key={workspace.id}
                role="option"
                aria-selected={checked}
                data-index={index}
                className={`fc-cleanup-row${checked ? ' is-checked' : ''}${index === activeIndex ? ' is-active' : ''}`}
                onMouseEnter={() => setCursor(index)}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => toggle(workspace.id)}
              >
                <span className={`fc-checkbox${checked ? ' is-checked' : ''}`} aria-hidden="true">
                  {checked ? <Check size={11} strokeWidth={2.5} /> : null}
                </span>
                <span className="fc-cleanup-main">
                  <span className="fc-cleanup-name">
                    <Highlight
                      text={workspace.name}
                      indices={query.trim() ? matchFields(query, [workspace.name])?.indices[0] : []}
                    />
                  </span>
                  <span className="fc-cleanup-path">
                    {workspace.displayPath}
                    {workspace.lastActiveAt > 0 ? ` · ${relativeTime(workspace.lastActiveAt)}` : ''}
                  </span>
                </span>
                {!workspace.exists ? (
                  <span className="fc-status-chip fc-status-chip--warning">路径不存在</span>
                ) : null}
                {workspace.remote ? (
                  <span className="fc-status-chip">{workspace.remote.label}</span>
                ) : null}
              </div>
            )
          })
        )}
      </div>

      <div className="fc-settings-footer fc-cleanup-footer" aria-live="polite">
        <span className="fc-cleanup-summary">
          {selectedCount > 0
            ? `已选 ${selectedCount} 项`
            : visible.length === workspaces.length
              ? `共 ${workspaces.length} 个工作区`
              : `显示 ${visible.length} / 共 ${workspaces.length} 个工作区`}
        </span>
        <span className="fc-cleanup-hint">
          <kbd>空格</kbd> 勾选 · <kbd>↵</kbd> 删除 · <kbd>esc</kbd> 返回
        </span>
      </div>
    </div>
  )
}
