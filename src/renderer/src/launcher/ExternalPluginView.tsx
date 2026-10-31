import { Puzzle, SearchX, Settings } from 'lucide-react'
import type { ExternalPluginPage } from '@shared/types'
import { ErrorCard, Glyph, Highlight, IconButton, Keycap, SkeletonRows, VirtualList } from '../components/ui'
import type { ExternalRowModel } from './helpers'

export function ExternalPluginView({
  page,
  rows,
  query,
  selectedIndex,
  loading,
  onHover,
  onActivate,
  onSettings
}: {
  page: ExternalPluginPage
  rows: ExternalRowModel[]
  query: string
  selectedIndex: number
  loading: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
  onSettings: () => void
}): React.JSX.Element {
  const showVirtual = rows.length > 60
  return (
    <div className="fc-plugin">
      <div className="fc-plugin-head">
        <h1 className="fc-plugin-title">{page.title}</h1>
        <span className="fc-plugin-head-meta">{rows.length > 0 ? `${rows.length} 项` : ''}</span>
        <div className="fc-plugin-head-actions">
          <IconButton label={`${page.title}设置`} onClick={onSettings}>
            <Settings size={16} strokeWidth={1.5} aria-hidden="true" />
          </IconButton>
        </div>
      </div>
      <div className="fc-plugin-body">
        {loading ? (
          <SkeletonRows count={4} />
        ) : page.error ? (
          <ErrorCard title={page.error} />
        ) : rows.length === 0 ? (
          <div className="fc-empty">
            <div className="fc-empty-icon">
              {query ? (
                <SearchX size={26} strokeWidth={1.5} aria-hidden="true" />
              ) : (
                <Puzzle size={26} strokeWidth={1.5} aria-hidden="true" />
              )}
            </div>
            <div className="fc-empty-title">
              {query ? '没有匹配的结果' : page.emptyTitle ?? '插件还没有内容'}
            </div>
            <div className="fc-empty-description">
              {query ? '换一个关键词试试' : page.emptyDescription ?? '这个插件没有返回任何条目'}
            </div>
          </div>
        ) : showVirtual ? (
          <VirtualList
            items={rows}
            rowHeight={48}
            activeIndex={selectedIndex}
            className="fc-plugin-scroll"
            renderRow={(row, index) => (
              <ExternalRow
                key={row.id}
                row={row}
                index={index}
                selected={index === selectedIndex}
                onHover={onHover}
                onActivate={onActivate}
              />
            )}
          />
        ) : (
          <div className="fc-plugin-scroll fc-scroll" role="listbox" id="fc-results" aria-label={page.title}>
            {rows.map((row, index) => (
              <ExternalRow
                key={row.id}
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
        {rows.length} 项结果
      </div>
    </div>
  )
}

function ExternalRow({
  row,
  index,
  selected,
  onHover,
  onActivate
}: {
  row: ExternalRowModel
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
      className="fc-row fc-external-row"
      data-index={index}
      data-flip-id={row.id}
      onMouseEnter={() => onHover(index)}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onActivate(index)}
    >
      <span className="fc-row-icon fc-row-icon--accent">
        <Glyph name={row.iconKey ?? 'puzzle'} src={row.icon} size={17} />
      </span>
      <span className="fc-row-body">
        <span className="fc-row-title">
          <Highlight text={row.title} indices={row.titleIndices} />
        </span>
        {row.subtitle ? (
          <span className="fc-row-subtitle">
            <Highlight text={row.subtitle} indices={row.subtitleIndices} />
          </span>
        ) : null}
      </span>
      {row.badge ? <span className="fc-chip fc-external-badge">{row.badge}</span> : null}
      <span className="fc-row-tail fc-row-enter">
        <Keycap>↵</Keycap>
      </span>
    </div>
  )
}
