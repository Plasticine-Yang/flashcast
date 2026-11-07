import { Clipboard as ClipboardIcon, Image as ImageIcon, Music2, Play, SearchX } from 'lucide-react'
import { useEffect, useLayoutEffect, useRef } from 'react'
import type { ClipboardItem } from '@shared/types'
import { kindLabel, relativeTime } from '@shared/time'
import { Highlight, Keycap } from '../components/ui'
import { useFlipAnimation } from '../components/ui'
import { mediaUrl, type ClipboardRowModel } from './helpers'

export function ClipboardView({
  rows,
  totalCount,
  maxItems,
  query,
  selectedIndex,
  scrollNonce,
  deletingId,
  previewItem,
  onHover,
  onActivate
}: {
  rows: ClipboardRowModel[]
  totalCount: number
  maxItems: number
  query: string
  selectedIndex: number
  scrollNonce: number
  deletingId: string | null
  previewItem: ClipboardItem | null
  onHover: (index: number) => void
  onActivate: (index: number) => void
}): React.JSX.Element {
  const { containerRef: scrollRef } = useFlipAnimation(rows)
  const previewRef = useRef<HTMLElement>(null)
  const previewContentRef = useRef<HTMLDivElement>(null)
  const previewWasOpenRef = useRef(false)

  const selectedIndexRef = useRef(selectedIndex)
  selectedIndexRef.current = selectedIndex

  // Pointer hover must not scroll: moving the list under the cursor changes
  // the hovered row again and produces a scroll feedback loop. Only keyboard
  // navigation (scrollNonce) brings the selection into view.
  useEffect(() => {
    if (scrollNonce === 0) return
    const node = scrollRef.current?.querySelector<HTMLElement>(
      `[data-index="${selectedIndexRef.current}"]`
    )
    if (node) node.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [scrollNonce, scrollRef])

  useEffect(() => {
    scrollRef.current?.scrollTo({ top: 0 })
  }, [query, scrollRef])

  // Content is updated in place (no remount / opacity restart) so fast j/k
  // does not blink; only reset its scroll offset when the item changes.
  useLayoutEffect(() => {
    if (previewContentRef.current) previewContentRef.current.scrollTop = 0
  }, [previewItem?.id])

  // The preview unfolds from a tiny bead aligned with the selected row, then
  // expands into the 300px panel like liquid. It is part of the clipboard
  // layout: once open it stays open while the selection changes, and the
  // launcher's close animation handles the exit.
  useLayoutEffect(() => {
    if (!previewItem) {
      previewWasOpenRef.current = false
      return
    }
    const node = previewRef.current
    if (!node || previewWasOpenRef.current) return
    previewWasOpenRef.current = true
    for (const animation of node.getAnimations()) animation.cancel()
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    if (reduced) {
      node.style.transform = 'translateX(0px)'
      node.style.clipPath = 'inset(0px round 14px)'
      const fade = node.animate([{ opacity: 0 }, { opacity: 1 }], {
        duration: 120,
        easing: 'ease-out',
        fill: 'both'
      })
      fade.onfinish = () => {
        node.style.opacity = '1'
        fade.cancel()
      }
      return
    }

    const row = scrollRef.current?.querySelector<HTMLElement>(
      `[data-index="${selectedIndex}"]`
    )
    const body = node.offsetParent as HTMLElement | null
    const rowRect = row?.getBoundingClientRect()
    const bodyRect = body?.getBoundingClientRect()
    const height = node.offsetHeight
    const width = node.offsetWidth
    let center = height / 2
    if (rowRect && bodyRect) {
      center = rowRect.top + rowRect.height / 2 - bodyRect.top - node.offsetTop
    }
    const beadHalf = 4
    const top = Math.min(Math.max(center - beadHalf, 0), Math.max(0, height - beadHalf * 2))
    const bottom = Math.max(0, height - top - beadHalf * 2)
    const right = Math.max(0, width - 12)
    const bead = `inset(${top}px ${right}px ${bottom}px 0px round 6px)`

    const openAnimation = node.animate(
      [
        { opacity: 0, transform: 'translateX(22px)', clipPath: bead, offset: 0 },
        {
          opacity: 1,
          transform: 'translateX(16px)',
          clipPath: bead,
          offset: 0.16,
          easing: 'cubic-bezier(0.16, 1, 0.3, 1)'
        },
        { opacity: 1, transform: 'translateX(0px)', clipPath: 'inset(0px round 14px)', offset: 1 }
      ],
      { duration: 420, easing: 'linear', fill: 'both' }
    )
    openAnimation.onfinish = () => {
      node.style.opacity = '1'
      node.style.transform = 'translateX(0px)'
      node.style.clipPath = 'inset(0px round 14px)'
      openAnimation.cancel()
    }
  }, [previewItem, selectedIndex, scrollRef])

  return (
    <div className="fc-plugin">
      <div className="fc-plugin-head">
        <h1 className="fc-plugin-title">剪切板</h1>
        <span className="fc-plugin-head-meta tnum">
          {totalCount} / {maxItems}
        </span>
      </div>
      <div className="fc-plugin-body fc-clipboard-body">
        <div className={`fc-clipboard-list-wrap${previewItem ? ' is-preview-open' : ''}`}>
          {rows.length === 0 ? (
            <ClipboardEmpty query={query} totalCount={totalCount} />
          ) : (
            <div
              className="fc-clipboard-scroll fc-scroll"
              ref={scrollRef}
              role="listbox"
              id="fc-results"
              aria-label="剪切板记录"
            >
              {rows.map((row, index) => {
                const selected = index === selectedIndex
                const item = row.item
                const thumb = mediaUrl(item.thumbPath ?? item.mediaPath)
                const isImage = item.kind === 'image'
                const isVideo = item.kind === 'video'
                return (
                  <div
                    key={item.id}
                    id={`fc-result-${index}`}
                    role="option"
                    aria-selected={selected}
                    className={`fc-row fc-clipboard-row${deletingId === item.id ? ' is-deleting' : ''}`}
                    data-index={index}
                    data-flip-id={item.id}
                    onMouseEnter={() => onHover(index)}
                    onMouseDown={(event) => event.preventDefault()}
                    onClick={() => onActivate(index)}
                  >
                    <span
                      className={`fc-clip-block${item.kind === 'music' || item.kind === 'text' ? ' fc-clip-block--accent' : ''}`}
                    >
                      {isImage && thumb ? (
                        <img src={thumb} alt="" draggable={false} />
                      ) : isVideo && thumb ? (
                        <>
                          <img src={thumb} alt="" draggable={false} />
                          <span className="fc-clip-play">
                            <Play size={8} strokeWidth={2.5} fill="currentColor" aria-hidden="true" />
                          </span>
                        </>
                      ) : item.kind === 'music' ? (
                        <Music2 size={17} strokeWidth={1.5} aria-hidden="true" />
                      ) : item.kind === 'video' ? (
                        <Play size={16} strokeWidth={1.5} aria-hidden="true" />
                      ) : item.kind === 'image' ? (
                        <ImageIcon size={16} strokeWidth={1.5} aria-hidden="true" />
                      ) : (
                        <ClipboardIcon size={16} strokeWidth={1.5} aria-hidden="true" />
                      )}
                    </span>
                    <span className="fc-clip-main">
                      <span className={`fc-clip-title${item.kind === 'text' ? ' fc-clip-title--two-line' : ' fc-clip-title--single'}`}>
                        <Highlight text={row.titleText} indices={row.titleIndices} />
                      </span>
                      {row.subtitle ? (
                        <span className="fc-clip-subtitle">
                          <Highlight text={row.subtitle} indices={row.subtitleIndices} />
                        </span>
                      ) : item.kind === 'image' && item.width ? (
                        <span className="fc-clip-subtitle">
                          {item.width} × {item.height}
                        </span>
                      ) : null}
                    </span>
                    <span className="fc-clip-meta">
                      <span>{row.meta}</span>
                      <Keycap>↵</Keycap>
                    </span>
                  </div>
                )
              })}
            </div>
          )}
        </div>
        {previewItem ? (
          <aside className="fc-preview" ref={previewRef} aria-label="剪切板预览">
            <div className="fc-preview-head">
              <span>{kindLabel(previewItem.kind)}</span>
            </div>
            <div className="fc-preview-divider" />
            <div className="fc-preview-content" ref={previewContentRef}>
              <PreviewBody item={previewItem} />
            </div>
          </aside>
        ) : null}
      </div>
      <div className="sr-only" aria-live="polite">
        {rows.length} 条剪切板记录
      </div>
    </div>
  )
}

function ClipboardEmpty({ query, totalCount }: { query: string; totalCount: number }): React.JSX.Element {
  if (query && totalCount > 0) {
    return (
      <div className="fc-no-results">
        <div className="fc-no-results-icon">
          <SearchX size={26} strokeWidth={1.5} aria-hidden="true" />
        </div>
        <div className="fc-no-results-title">没有匹配的剪切板项</div>
        <div className="fc-no-results-hint">试试内容里的关键词，或清空搜索重新浏览</div>
      </div>
    )
  }
  return (
    <div className="fc-empty">
      <div className="fc-empty-icon">
        <ClipboardIcon size={26} strokeWidth={1.5} aria-hidden="true" />
      </div>
      <div className="fc-empty-title">还没有复制记录</div>
      <div className="fc-empty-description">
        复制任意文本、图片或文件后，会出现在这里。记录只保存在本地工作区。
      </div>
    </div>
  )
}

function PreviewBody({ item }: { item: ClipboardItem }): React.JSX.Element {
  if (item.kind === 'text') {
    return <pre className="fc-preview-text">{item.text ?? item.title}</pre>
  }

  if (item.kind === 'image') {
    const src = mediaUrl(item.mediaPath)
    return (
      <div className="fc-preview-image-wrap">
        {src ? <img src={src} alt="剪切板图片预览" draggable={false} /> : <ImageIcon size={24} strokeWidth={1.5} />}
      </div>
    )
  }

  if (item.kind === 'video') {
    const src = mediaUrl(item.mediaPath)
    return (
      <div className="fc-preview-video">
        <div className="fc-preview-video-frame">
          {src ? <img src={src} alt="视频封面" draggable={false} /> : <Play size={32} strokeWidth={1.5} />}
        </div>
        <span className="fc-preview-video-label">
          <Play size={11} strokeWidth={2} aria-hidden="true" />
          视频封面 · 不支持播放
        </span>
        {item.fileName ? (
          <div className="fc-preview-info">
            <strong>{item.fileName}</strong>
            <span>视频 · {relativeTime(item.createdAt)}</span>
          </div>
        ) : null}
      </div>
    )
  }

  return (
    <div className="fc-preview-music">
      <div className="fc-preview-music-icon">
        <Music2 size={30} strokeWidth={1.5} aria-hidden="true" />
      </div>
      <div className="fc-preview-music-title">音乐</div>
      <div className="fc-preview-music-meta">
        {item.fileName ?? '音频文件'}
        <br />
        {item.byteSize ? formatBytes(item.byteSize) : null}
        {item.byteSize ? ' · ' : ''}
        {relativeTime(item.createdAt)}
      </div>
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  return `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`
}
