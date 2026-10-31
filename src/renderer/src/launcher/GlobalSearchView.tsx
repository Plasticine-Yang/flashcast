import { SearchX } from 'lucide-react'
import { useEffect, useRef, type CSSProperties } from 'react'
import { Glyph, Highlight, Keycap, SkeletonRows, useDelayedFlag } from '../components/ui'
import type { GlobalResult, GlobalSection } from './helpers'

export function GlobalSearchView({
  sections,
  items,
  selectedIndex,
  appsLoading,
  query,
  showNonce,
  onHover,
  onActivate
}: {
  sections: GlobalSection[]
  items: GlobalResult[]
  selectedIndex: number
  appsLoading: boolean
  query: string
  showNonce: number
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  const containerRef = useRef<HTMLDivElement>(null)
  const showSkeleton = useDelayedFlag(appsLoading, 150)

  useEffect(() => {
    const node = containerRef.current?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`)
    if (node) {
      node.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
    }
  }, [selectedIndex])

  let offset = 0

  return (
    <div className="fc-global-scroll fc-scroll" ref={containerRef} id="fc-results" role="listbox" aria-label="搜索结果">
      <div className="fc-stagger fc-enter" key={showNonce}>
        {sections.map((section) => {
          const sectionOffset = offset
          offset += section.items.length
          return (
            <section className="fc-group" key={section.key} aria-labelledby={`fc-group-${section.key}`}>
              <div className="fc-group-title" id={`fc-group-${section.key}`}>
                {section.title}
              </div>
              {section.items.map((item, index) => {
                const flatIndex = sectionOffset + index
                return (
                  <ResultRow
                    key={item.id}
                    item={item}
                    index={flatIndex}
                    selected={flatIndex === selectedIndex}
                    onHover={onHover}
                    onActivate={onActivate}
                  />
                )
              })}
            </section>
          )
        })}
        {showSkeleton ? (
          <section className="fc-group" aria-hidden="true">
            <div className="fc-group-title">应用</div>
            <SkeletonRows count={3} />
          </section>
        ) : null}
        {items.length === 0 && !appsLoading ? (
          <div className="fc-no-results">
            <div className="fc-no-results-icon">
              <SearchX size={26} strokeWidth={1.5} aria-hidden="true" />
            </div>
            <div className="fc-no-results-title">没有找到「{query}」</div>
            <div className="fc-no-results-hint">
              试试应用名首字母，或插件关键字，如 <kbd>fbb</kbd>、<kbd>book</kbd>、<kbd>剪贴板</kbd>
            </div>
          </div>
        ) : null}
      </div>
      <div className="sr-only" aria-live="polite">
        {items.length} 个结果
      </div>
    </div>
  )
}

function ResultRow({
  item,
  index,
  selected,
  onHover,
  onActivate
}: {
  item: GlobalResult
  index: number
  selected: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  const letter = item.title.trim().charAt(0).toUpperCase()
  const isLetter = item.kind === 'app' && !item.icon
  const iconClass =
    item.kind === 'plugin' || item.kind === 'command'
      ? 'fc-row-icon fc-row-icon--accent'
      : `fc-row-icon${isLetter ? ' fc-row-icon--letter' : ''}`
  return (
    <div
      role="option"
      id={`fc-result-${index}`}
      aria-selected={selected}
      className="fc-row"
      data-index={index}
      style={{ '--row-index': index } as CSSProperties}
      onMouseEnter={() => onHover(index)}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onActivate(index)}
    >
      <span className={iconClass}>
        {item.icon ? (
          <img src={item.icon} alt="" draggable={false} />
        ) : isLetter ? (
          letter
        ) : (
          <Glyph name={item.iconKey} size={17} />
        )}
      </span>
      <span className="fc-row-body">
        <span className="fc-row-title">
          <Highlight text={item.title} indices={item.titleIndices} />
        </span>
        <span className="fc-row-subtitle">
          <Highlight text={item.subtitle} indices={item.subtitleIndices} />
        </span>
      </span>
      <span className="fc-row-tail fc-row-enter">
        <Keycap>↵</Keycap>
      </span>
    </div>
  )
}
