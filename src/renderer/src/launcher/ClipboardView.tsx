import { Clipboard as ClipboardIcon, Image as ImageIcon, Music2, Play, SearchX, Settings, X } from 'lucide-react'
import { useEffect, useLayoutEffect, useRef } from 'react'
import type { ClipboardItem } from '@shared/types'
import { kindLabel, relativeTime } from '@shared/time'
import { Highlight, IconButton, Keycap } from '../components/ui'
import { useFlipAnimation } from '../components/ui'
import { mediaUrl, type ClipboardRowModel } from './helpers'

export interface PreviewState {
  mounted: boolean
  open: boolean
  pinned: boolean
  item: ClipboardItem | null
}

export function ClipboardView({
  rows,
  totalCount,
  maxItems,
  query,
  selectedIndex,
  deletingId,
  preview,
  onHover,
  onActivate,
  onSettings,
  onPreviewClose,
  onActivity
}: {
  rows: ClipboardRowModel[]
  totalCount: number
  maxItems: number
  query: string
  selectedIndex: number
  deletingId: string | null
  preview: PreviewState
  onHover: (index: number) => void
  onActivate: (index: number) => void
  onSettings: () => void
  onPreviewClose: () => void
  onActivity: (event?: React.SyntheticEvent) => void
}): React.JSX.Element {
  const { containerRef: scrollRef } = useFlipAnimation(rows, (row) => row.item.id)
  const previewRef = useRef<HTMLElement>(null)
  const previewContentRef = useRef<HTMLDivElement>(null)
  const previewWasOpenRef = useRef(false)

  useEffect(() => {
    const node = scrollRef.current?.querySelector<HTMLElement>(`[data-index="${selectedIndex}"]`)
    if (node) node.scrollIntoView({ block: 'nearest', behavior: 'smooth' })
  }, [selectedIndex])

  // Content is updated in place (no remount / opacity restart) so fast j/k
  // does not blink; only reset its scroll offset when the item changes.
  useLayoutEffect(() => {
    if (previewContentRef.current) previewContentRef.current.scrollTop = 0
  }, [preview.item?.id, preview.open])

  // The preview unfolds from a tiny bead aligned with the selected row, then
  // expands into the 300px panel like liquid. Keeping this in WAAPI lets us
  // measure the row at open time; the old translate+clip transition always
  // started from the panel's top-right and read as a generic slide-in.
  useLayoutEffect(() => {
    if (!preview.mounted) {
      previewWasOpenRef.current = false
      return
    }
    const node = previewRef.current
    if (!node) return
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches

    if (preview.open) {
      if (previewWasOpenRef.current) return
      previewWasOpenRef.current = true
      for (const animation of node.getAnimations()) animation.cancel()
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
      return
    }

    if (!previewWasOpenRef.current) return
    previewWasOpenRef.current = false
    const computed = window.getComputedStyle(node)
    const startOpacity = computed.opacity
    const startTransform = computed.transform
    const startClip = computed.clipPath
    for (const animation of node.getAnimations()) animation.cancel()
    node.animate(
      [
        { opacity: startOpacity, transform: startTransform, clipPath: startClip },
        {
          opacity: 0,
          transform: 'translateX(16px)',
          clipPath: 'inset(0px 0px 0px 24px round 14px)'
        }
      ],
      {
        duration: reduced ? 120 : 180,
        easing: 'cubic-bezier(0.4, 0, 1, 1)',
        fill: 'both'
      }
    )
  }, [preview.open, preview.mounted, selectedIndex, scrollRef])

  return (
    <div className="fc-plugin">
      <div className="fc-plugin-head">
        <h1 className="fc-plugin-title">剪切板</h1>
        <span className="fc-plugin-head-meta tnum">
          {totalCount} / {maxItems}
        </span>
        <div className="fc-plugin-head-actions">
          <IconButton label="剪切板设置" onClick={onSettings}>
            <Settings size={16} strokeWidth={1.5} aria-hidden="true" />
          </IconButton>
        </div>
      </div>
      <div className="fc-plugin-body fc-clipboard-body">
        <div className={`fc-clipboard-list-wrap${preview.open ? ' is-preview-open' : ''}`}>
          {rows.length === 0 ? (
            <ClipboardEmpty query={query} totalCount={totalCount} />
          ) : (
            <div
              className="fc-clipboard-scroll fc-scroll"
              ref={scrollRef}
              role="listbox"
              id="fc-results"
              aria-label="剪切板记录"
              onScroll={(event) => onActivity(event)}
              onMouseMove={(event) => onActivity(event)}
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
        {preview.mounted && preview.item ? (
          <aside
            className="fc-preview"
            ref={previewRef}
            data-open={preview.open}
            aria-label="剪切板预览"
            aria-hidden={!preview.open}
          >
            <div className="fc-preview-head">
              <span>{kindLabel(preview.item.kind)}</span>
              {preview.pinned ? <span className="fc-chip fc-chip--accent">已固定</span> : null}
              <IconButton label="收起预览" onClick={onPreviewClose}>
                <X size={13} strokeWidth={1.75} aria-hidden="true" />
              </IconButton>
            </div>
            <div className="fc-preview-divider" />
            <div className="fc-preview-content" ref={previewContentRef}>
              <PreviewBody item={preview.item} />
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
