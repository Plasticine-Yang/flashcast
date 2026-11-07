import { AlertTriangle, Code2, FolderGit2, RefreshCw, SearchX, Settings } from 'lucide-react'
import { useEffect, useRef } from 'react'
import { relativeTime } from '@shared/time'
import type { EditorVariant, VscodeLikeWorkspacePluginPage } from '@shared/types'
import { Glyph, Highlight, Keycap, SkeletonRows, VirtualList, scrollRowIntoView } from '../components/ui'
import { editorSubtitle, type EditorRowModel, type WorkspaceRowModel } from './helpers'

export function VscodeLikeWorkspaceView({
  page,
  mode,
  editorRows,
  workspaceRows,
  activeVariant,
  query,
  selectedIndex,
  loading,
  canGoBack,
  onHover,
  onActivate,
  onBack,
  onSettings,
  onRetry
}: {
  page: VscodeLikeWorkspacePluginPage | null
  mode: 'picker' | 'list'
  editorRows: EditorRowModel[]
  workspaceRows: WorkspaceRowModel[]
  activeVariant: EditorVariant | null
  query: string
  selectedIndex: number
  loading: boolean
  canGoBack: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
  onBack: () => void
  onSettings: () => void
  onRetry: () => void
}): React.JSX.Element {
  const rowsCount = mode === 'picker' ? editorRows.length : workspaceRows.length
  const activeError = page?.errors.find((error) => error.variantId === activeVariant?.id) ?? null
  const showVirtual = rowsCount > 60
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    scrollRowIntoView(scrollRef.current, selectedIndex)
  }, [selectedIndex, rowsCount])

  const title = mode === 'picker' ? 'VSCode-like Workspace' : activeVariant?.name ?? '工作区'
  const meta =
    mode === 'picker'
      ? page
        ? `${page.variants.length} 个编辑器`
        : null
      : `${workspaceRows.length} 个工作区`

  return (
    <div className="fc-plugin">
      <div className="fc-plugin-head">
        {canGoBack ? (
          <button type="button" className="fc-back-button" onClick={onBack}>
            编辑器
          </button>
        ) : null}
        <h1 className="fc-plugin-title">{title}</h1>
        <span className="fc-plugin-head-meta">{meta}</span>
      </div>
      <div className="fc-plugin-body fc-workspace-body">
        {activeError ? (
          <div className="fc-workspace-notice" role="status">
            <AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" />
            <span>{activeError.message}</span>
          </div>
        ) : null}
        {loading && !page ? (
          <SkeletonRows count={4} />
        ) : page && page.variants.length === 0 && mode === 'picker' ? (
          <div className="fc-empty">
            <div className="fc-empty-icon">
              <Code2 size={26} strokeWidth={1.5} aria-hidden="true" />
            </div>
            <div className="fc-empty-title">没有检测到 VS Code 系编辑器</div>
            <div className="fc-empty-description">
              支持 VS Code、Trae、Cursor、Windsurf、VSCodium 等基于 VS Code 的产品。安装后重新扫描即可。
            </div>
            <div className="fc-empty-actions">
              <button type="button" className="fc-btn fc-btn--primary" onClick={onSettings}>
                打开设置
              </button>
              <button type="button" className="fc-btn" onClick={onRetry}>
                <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />
                重新扫描
              </button>
            </div>
          </div>
        ) : rowsCount === 0 ? (
          query ? (
            <div className="fc-no-results">
              <div className="fc-no-results-icon">
                <SearchX size={26} strokeWidth={1.5} aria-hidden="true" />
              </div>
              <div className="fc-no-results-title">没有匹配的工作区</div>
              <div className="fc-no-results-hint">
                试试文件夹名、完整路径或拼音，例如 <kbd>kuake</kbd>、<kbd>corehr</kbd>
              </div>
            </div>
          ) : (
            <div className="fc-empty">
              <div className="fc-empty-icon">
                <FolderGit2 size={26} strokeWidth={1.5} aria-hidden="true" />
              </div>
              <div className="fc-empty-title">还没有打开过工作区</div>
              <div className="fc-empty-description">
                在 {activeVariant?.name ?? '编辑器'} 中打开一个文件夹或 .code-workspace 文件后，这里会自动出现。
              </div>
              <div className="fc-empty-actions">
                <button type="button" className="fc-btn" onClick={onRetry}>
                  <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />
                  重新扫描
                </button>
                <button type="button" className="fc-btn fc-btn--ghost" onClick={onSettings}>
                  <Settings size={13} strokeWidth={1.75} aria-hidden="true" />
                  设置
                </button>
              </div>
            </div>
          )
        ) : showVirtual && mode === 'picker' ? (
          <VirtualList
            items={editorRows}
            rowHeight={48}
            activeIndex={selectedIndex}
            className="fc-plugin-scroll"
            renderRow={(row, index) => (
              <EditorRow
                key={row.variant.id}
                row={row}
                index={index}
                selected={index === selectedIndex}
                onHover={onHover}
                onActivate={onActivate}
              />
            )}
          />
        ) : showVirtual ? (
          <VirtualList
            items={workspaceRows}
            rowHeight={48}
            activeIndex={selectedIndex}
            className="fc-plugin-scroll"
            renderRow={(row, index) => (
              <WorkspaceRow
                key={row.workspace.id}
                row={row}
                index={index}
                selected={index === selectedIndex}
                onHover={onHover}
                onActivate={onActivate}
              />
            )}
          />
        ) : (
          <div
            className="fc-plugin-scroll fc-scroll"
            ref={scrollRef}
            role="listbox"
            id="fc-results"
            aria-label={title}
          >
            {mode === 'picker'
              ? editorRows.map((row, index) => (
                  <EditorRow
                    key={row.variant.id}
                    row={row}
                    index={index}
                    selected={index === selectedIndex}
                    onHover={onHover}
                    onActivate={onActivate}
                  />
                ))
              : workspaceRows.map((row, index) => (
                  <WorkspaceRow
                    key={row.workspace.id}
                    row={row}
                    index={index}
                    selected={index === selectedIndex}
                    onHover={onHover}
                    onActivate={onActivate}
                  />
                ))}
          </div>
        )}
      </div>
      <div className="sr-only" aria-live="polite">
        {rowsCount} 项结果
      </div>
    </div>
  )
}

function EditorRow({
  row,
  index,
  selected,
  onHover,
  onActivate
}: {
  row: EditorRowModel
  index: number
  selected: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  return (
    <div
      id={`fc-result-${index}`}
      role="option"
      aria-selected={selected}
      className="fc-row fc-workspace-row"
      data-index={index}
      onMouseEnter={() => onHover(index)}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onActivate(index)}
    >
      <span className="fc-row-icon fc-row-icon--accent">
        <Glyph name="code" size={17} />
      </span>
      <span className="fc-row-body">
        <span className="fc-row-title">
          <Highlight text={row.variant.name} indices={row.titleIndices} />
        </span>
        <span className="fc-row-subtitle">
          <Highlight text={editorSubtitle(row)} indices={row.subtitleIndices} />
        </span>
      </span>
      <span className="fc-row-tail fc-row-enter">
        <Keycap>↵</Keycap>
      </span>
    </div>
  )
}

function WorkspaceRow({
  row,
  index,
  selected,
  onHover,
  onActivate
}: {
  row: WorkspaceRowModel
  index: number
  selected: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  const workspace = row.workspace
  const timeLabel = workspace.lastActiveAt > 0 ? relativeTime(workspace.lastActiveAt) : '时间未知'
  const subtitle = `${workspace.displayPath} · ${timeLabel}`
  return (
    <div
      id={`fc-result-${index}`}
      role="option"
      aria-selected={selected}
      className="fc-row fc-workspace-row"
      data-index={index}
      data-flip-id={workspace.id}
      onMouseEnter={() => onHover(index)}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onActivate(index)}
    >
      <span className="fc-row-icon fc-row-icon--accent">
        <Glyph name={workspace.kind === 'workspace' ? 'file-code' : 'folder'} size={17} />
      </span>
      <span className="fc-row-body">
        <span className="fc-row-title">
          <Highlight text={workspace.name} indices={row.titleIndices} />
        </span>
        <span className="fc-row-subtitle">
          <Highlight text={subtitle} indices={row.subtitleIndices} />
        </span>
      </span>
      <span className="fc-workspace-badges">
        {workspace.isOpen ? <span className="fc-status-chip fc-status-chip--success">打开中</span> : null}
        {workspace.remote ? <span className="fc-status-chip">{workspace.remote.label}</span> : null}
        {!workspace.exists ? (
          <span className="fc-status-chip fc-status-chip--warning">路径不存在</span>
        ) : null}
      </span>
      <span className="fc-row-tail fc-row-enter">
        <Keycap>↵</Keycap>
      </span>
    </div>
  )
}
