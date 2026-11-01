import { Bookmark, Globe, RefreshCw, SearchX, Settings } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { ChromeBookmarksPluginPage } from '@shared/types'
import { api } from '../bridge'
import { Highlight, IconButton, Keycap, SkeletonRows, ErrorCard } from '../components/ui'
import { VirtualList } from '../components/ui'
import type { BookmarkRowModel } from './helpers'

export function BookmarksView({
  page,
  rows,
  query,
  selectedIndex,
  loading,
  onHover,
  onActivate,
  onSettings,
  onRetry
}: {
  page: ChromeBookmarksPluginPage | null
  rows: BookmarkRowModel[]
  query: string
  selectedIndex: number
  loading: boolean
  onHover: (index: number) => void
  onActivate: (index: number) => void
  onSettings: () => void
  onRetry: () => void
}): React.JSX.Element {
  const [favicons, setFavicons] = useState<Record<string, string>>({})
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!page) return
    const domains = [...new Set(page.bookmarks.map((bookmark) => bookmark.domain).filter(Boolean))].slice(0, 80)
    if (domains.length === 0) return
    let cancelled = false
    void api
      .getFavicons(domains)
      .then((result) => {
        if (!cancelled) setFavicons(result)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [page])

  useEffect(() => {
    const node = scrollRef.current?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`)
    if (node) node.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [selectedIndex, rows.length])

  const showVirtual = rows.length > 60
  const profile = page?.profiles.find((item) => item.dir === page.selectedProfile)

  return (
    <div className="fc-plugin">
      <div className="fc-plugin-head">
        <h1 className="fc-plugin-title">Chrome 书签</h1>
        <span className="fc-plugin-head-meta">
          {page ? `${page.bookmarks.length} 个书签` : null}
          {page?.bookmarkSource === 'account' ? ' · 账户书签' : ''}
          {profile ? ` · ${profile.name}` : ''}
        </span>
        <div className="fc-plugin-head-actions">
          <IconButton label="Chrome 书签设置" onClick={onSettings}>
            <Settings size={16} strokeWidth={1.5} aria-hidden="true" />
          </IconButton>
        </div>
      </div>
      <div className="fc-plugin-body">
        {loading ? (
          <SkeletonRows count={4} />
        ) : page?.error ? (
          <ErrorCard
            title={page.error === '未找到 Chrome Profile' ? '未找到 Chrome Profile' : page.error}
            description={
              page.error === '未找到 Chrome Profile'
                ? '请先安装 Google Chrome 并至少打开一次，或在插件设置中选择已存在的 Profile。'
                : '可能是书签文件被占用或格式异常，请稍后重试。'
            }
          >
            {page.error === '未找到 Chrome Profile' ? (
              <button type="button" className="fc-btn fc-btn--primary" onClick={onSettings}>
                选择 Profile
              </button>
            ) : null}
            <button type="button" className="fc-btn" onClick={onRetry}>
              <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />
              重试
            </button>
          </ErrorCard>
        ) : rows.length === 0 ? (
          query ? (
            <div className="fc-no-results">
              <div className="fc-no-results-icon">
                <SearchX size={26} strokeWidth={1.5} aria-hidden="true" />
              </div>
              <div className="fc-no-results-title">没有匹配的书签</div>
              <div className="fc-no-results-hint">试试标题、网址或文件夹名，例如「github」「工作」「docs」</div>
            </div>
          ) : (
            <div className="fc-empty">
              <div className="fc-empty-icon">
                <Bookmark size={26} strokeWidth={1.5} aria-hidden="true" />
              </div>
              <div className="fc-empty-title">这个 Profile 还没有书签</div>
              <div className="fc-empty-description">在 Chrome 中添加书签后，会按书签栏顺序出现在这里。</div>
              <button type="button" className="fc-btn" onClick={onRetry} style={{ marginTop: 8 }}>
                重新加载
              </button>
            </div>
          )
        ) : showVirtual ? (
          <VirtualList
            items={rows}
            rowHeight={48}
            activeIndex={selectedIndex}
            className="fc-plugin-scroll"
            renderRow={(row, index) => (
              <BookmarkRow
                key={row.bookmark.id}
                row={row}
                index={index}
                selected={index === selectedIndex}
                favicon={favicons[row.bookmark.domain]}
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
            aria-label="Chrome 书签"
          >
            {rows.map((row, index) => (
              <BookmarkRow
                key={row.bookmark.id}
                row={row}
                index={index}
                selected={index === selectedIndex}
                favicon={favicons[row.bookmark.domain]}
                onHover={onHover}
                onActivate={onActivate}
              />
            ))}
          </div>
        )}
      </div>
      <div className="sr-only" aria-live="polite">
        {rows.length} 个书签结果
      </div>
    </div>
  )
}

function BookmarkRow({
  row,
  index,
  selected,
  favicon,
  onHover,
  onActivate
}: {
  row: BookmarkRowModel
  index: number
  selected: boolean
  favicon?: string
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  const folder = row.bookmark.folder.join(' / ')
  return (
    <div
      id={`fc-result-${index}`}
      role="option"
      aria-selected={selected}
      className="fc-row fc-bookmark-row"
      data-index={index}
      data-flip-id={row.bookmark.id}
      onMouseEnter={() => onHover(index)}
      onMouseDown={(event) => event.preventDefault()}
      onClick={() => onActivate(index)}
    >
      <span className="fc-bookmark-favicon">
        {favicon ? (
          <img src={favicon} alt="" draggable={false} />
        ) : (
          <Globe size={16} strokeWidth={1.5} aria-hidden="true" />
        )}
      </span>
      <span className="fc-row-body">
        <span className="fc-row-title">
          <Highlight text={row.bookmark.title} indices={row.titleIndices} />
        </span>
        <span className="fc-bookmark-url">
          <Highlight text={row.displayUrl} indices={row.urlIndices} />
        </span>
      </span>
      {folder ? (
        <span className="fc-chip fc-bookmark-folder">
          <Highlight text={folder} indices={row.folderIndices} />
        </span>
      ) : null}
      <span className="fc-row-tail fc-bookmark-enter">
        <Keycap>↵</Keycap>
      </span>
    </div>
  )
}
