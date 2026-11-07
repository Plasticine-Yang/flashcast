import {
  AlertTriangle,
  AppWindow,
  ArrowDownToLine,
  Bookmark,
  Check,
  Clipboard,
  Code2,
  FileCode2,
  FileText,
  FolderGit2,
  Globe,
  Image as ImageIcon,
  Info,
  Music2,
  Play,
  Power,
  Puzzle,
  RefreshCw,
  Settings,
  Terminal,
  type LucideIcon
} from 'lucide-react'
import { Fragment, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { highlightSegments } from '@shared/fuzzy'
import type { ClipboardKind, PlatformId } from '@shared/types'
import { keycapParts } from '@shared/shortcuts'

export function Keycap({ children }: { children: ReactNode }): React.JSX.Element {
  return <kbd className="fc-keycap">{children}</kbd>
}

export function Keycaps({ parts }: { parts: string[] }): React.JSX.Element {
  return (
    <span className="fc-keycaps">
      {parts.map((part, index) => (
        <Keycap key={`${part}-${index}`}>{part}</Keycap>
      ))}
    </span>
  )
}

export function AcceleratorKeys({
  accelerator,
  platform
}: {
  accelerator: string
  platform: PlatformId
}): React.JSX.Element {
  return <Keycaps parts={keycapParts(accelerator, platform)} />
}

export function Hint({
  keys,
  label,
  tone
}: {
  keys: string[]
  label: string
  tone?: 'danger'
}): React.JSX.Element {
  return (
    <span className={`fc-hint${tone === 'danger' ? ' fc-hint--danger' : ''}`}>
      <Keycaps parts={keys} />
      <span>{label}</span>
    </span>
  )
}

export function Highlight({
  text,
  indices
}: {
  text: string
  indices?: number[]
}): React.JSX.Element {
  const segments = highlightSegments(text, indices)
  return (
    <>
      {segments.map((segment, index) =>
        segment.match ? (
          <mark className="fc-match" key={index}>
            {segment.text}
          </mark>
        ) : (
          <Fragment key={index}>{segment.text}</Fragment>
        )
      )}
    </>
  )
}

const ICONS: Record<string, LucideIcon> = {
  clipboard: Clipboard,
  bookmark: Bookmark,
  settings: Settings,
  update: ArrowDownToLine,
  refresh: RefreshCw,
  power: Power,
  app: AppWindow,
  puzzle: Puzzle,
  terminal: Terminal,
  globe: Globe,
  text: FileText,
  image: ImageIcon,
  video: Play,
  music: Music2,
  code: Code2,
  'file-code': FileCode2,
  folder: FolderGit2
}

/**
 * Scrolls one virtualised row into view without touching ancestor scroll
 * containers. `scrollIntoView()` bubbles up and shifts the launcher's view
 * shell, which detaches the search bar from the panel.
 */
export function scrollRowIntoView(container: HTMLElement | null, index: number): void {
  if (!container) return
  const row = container.querySelector<HTMLElement>(`[data-index="${index}"]`)
  if (!row) return
  const rowRect = row.getBoundingClientRect()
  const boxRect = container.getBoundingClientRect()
  const delta = rowRect.top - boxRect.top
  if (delta < 0) {
    container.scrollTop += delta
    return
  }
  const overflow = delta + rowRect.height - container.clientHeight
  if (overflow > 0) container.scrollTop += overflow
}

export function Glyph({
  name,
  src,
  size = 18
}: {
  name?: string
  src?: string
  size?: number
}): React.JSX.Element {
  if (src) return <img src={src} alt="" width={size} height={size} draggable={false} />
  const Icon = (name && ICONS[name]) || Puzzle
  return <Icon size={size} strokeWidth={1.5} aria-hidden="true" />
}

export function PluginIcon({
  icon,
  iconKey,
  size = 18
}: {
  icon?: string
  iconKey?: string
  size?: number
}): React.JSX.Element {
  return <Glyph name={iconKey ?? 'puzzle'} src={icon} size={size} />
}

export function KindGlyph({ kind, size = 16 }: { kind: ClipboardKind; size?: number }): React.JSX.Element {
  if (kind === 'text') return <FileText size={size} strokeWidth={1.5} aria-hidden="true" />
  if (kind === 'image') return <ImageIcon size={size} strokeWidth={1.5} aria-hidden="true" />
  if (kind === 'video') return <Play size={size} strokeWidth={1.5} aria-hidden="true" />
  return <Music2 size={size} strokeWidth={1.5} aria-hidden="true" />
}

export function Switch({
  checked,
  onChange,
  label
}: {
  checked: boolean
  onChange: (next: boolean) => void
  label: string
}): React.JSX.Element {
  return (
    <button
      type="button"
      className="fc-switch"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
    />
  )
}

export function IconButton({
  label,
  onClick,
  children,
  className = ''
}: {
  label: string
  onClick?: () => void
  children: ReactNode
  className?: string
}): React.JSX.Element {
  return (
    <button
      type="button"
      className={`fc-icon-button ${className}`}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      {children}
    </button>
  )
}

export function SkeletonRows({ count = 3 }: { count?: number }): React.JSX.Element {
  return (
    <div className="fc-loading-rows" aria-hidden="true">
      {Array.from({ length: count }, (_, index) => (
        <div className="fc-skeleton-row" key={index}>
          <div className="fc-skeleton" />
          <div className="fc-skeleton" />
        </div>
      ))}
    </div>
  )
}

export function useDelayedFlag(active: boolean, delay = 150): boolean {
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    if (!active) {
      setVisible(false)
      return
    }
    const timer = window.setTimeout(() => setVisible(true), delay)
    return () => window.clearTimeout(timer)
  }, [active, delay])
  return visible
}

export function useToast(
  initial: { message: string; tone: 'default' | 'success' | 'warning' | 'danger'; duration?: number } | null = null
): {
  toast: { message: string; tone: 'default' | 'success' | 'warning' | 'danger'; duration?: number } | null
  showToast: (toast: { message: string; tone?: 'default' | 'success' | 'warning' | 'danger'; duration?: number }) => void
  dismissToast: () => void
} {
  const [toast, setToast] = useState(initial)
  const timer = useRef<number | null>(null)

  const dismissToast = (): void => {
    if (timer.current) window.clearTimeout(timer.current)
    timer.current = null
    setToast(null)
  }

  const showToast = (next: {
    message: string
    tone?: 'default' | 'success' | 'warning' | 'danger'
    duration?: number
  }): void => {
    if (timer.current) window.clearTimeout(timer.current)
    const value = { ...next, tone: next.tone ?? 'default' }
    setToast(value)
    timer.current = window.setTimeout(() => {
      setToast(null)
      timer.current = null
    }, next.duration ?? 2400)
  }

  useEffect(() => () => {
    if (timer.current) window.clearTimeout(timer.current)
  }, [])

  return { toast, showToast, dismissToast }
}

export function Toast({
  toast
}: {
  toast: { message: string; tone: 'default' | 'success' | 'warning' | 'danger' } | null
}): React.JSX.Element | null {
  if (!toast) return null
  const Icon = toast.tone === 'success' ? Check : toast.tone === 'danger' ? AlertTriangle : toast.tone === 'warning' ? AlertTriangle : Info
  return (
    <div className="fc-toast" data-tone={toast.tone} role="status" aria-live="polite">
      <span className="fc-toast-icon">
        <Icon size={14} strokeWidth={2} aria-hidden="true" />
      </span>
      <span>{toast.message}</span>
    </div>
  )
}

export function ErrorCard({
  title,
  description,
  children
}: {
  title: string
  description?: string
  children?: ReactNode
}): React.JSX.Element {
  return (
    <div className="fc-error-card" role="alert">
      <div className="fc-error-heading">
        <AlertTriangle size={14} strokeWidth={2} aria-hidden="true" />
        {title}
      </div>
      {description ? <div className="fc-error-description">{description}</div> : null}
      {children ? <div className="fc-error-actions">{children}</div> : null}
    </div>
  )
}

export function useFlipAnimation<T>(items: T[]): {
  containerRef: React.RefObject<HTMLDivElement | null>
} {
  const containerRef = useRef<HTMLDivElement>(null)
  const positions = useRef(new Map<string, number>())

  // Only item membership/order changes should schedule a FLIP. Measuring on
  // every render (e.g. a hover or preview update re-render) makes rows chase
  // their own running transforms and reads as flicker while the pointer moves.
  // Layout offsets are also immune to ancestor entrance transforms and list
  // scrolling, both of which changed getBoundingClientRect() mid-animation.
  useLayoutEffect(() => {
    const container = containerRef.current
    if (!container) return
    const nodes = Array.from(container.querySelectorAll<HTMLElement>('[data-flip-id]'))
    const next = new Map<string, number>()
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    for (const node of nodes) {
      const key = node.dataset.flipId
      if (!key) continue
      const top = node.offsetTop
      const previous = positions.current.get(key)
      if (previous !== undefined && Math.abs(previous - top) > 0.5 && !reduced) {
        node.animate(
          [{ transform: `translateY(${previous - top}px)` }, { transform: 'translateY(0)' }],
          { duration: 220, easing: 'cubic-bezier(0.16, 1, 0.3, 1)' }
        )
      }
      next.set(key, top)
    }
    positions.current = next
  }, [items])

  return { containerRef }
}

export function VirtualList<T>({
  items,
  rowHeight,
  activeIndex,
  renderRow,
  className = ''
}: {
  items: T[]
  rowHeight: number
  activeIndex: number
  renderRow: (item: T, index: number) => ReactNode
  className?: string
}): React.JSX.Element {
  const containerRef = useRef<HTMLDivElement>(null)
  const [scrollTop, setScrollTop] = useState(0)
  const [viewport, setViewport] = useState(320)
  const overscan = 6

  useEffect(() => {
    const node = containerRef.current
    if (!node) return
    const update = (): void => setViewport(node.clientHeight || 320)
    update()
    const observer = new ResizeObserver(update)
    observer.observe(node)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    const node = containerRef.current
    if (!node || activeIndex < 0) return
    const top = activeIndex * rowHeight
    const bottom = top + rowHeight
    if (top < node.scrollTop) {
      node.scrollTop = top
    } else if (bottom > node.scrollTop + node.clientHeight) {
      node.scrollTop = bottom - node.clientHeight
    }
  }, [activeIndex, rowHeight])

  const start = Math.max(0, Math.floor(scrollTop / rowHeight) - overscan)
  const end = Math.min(items.length, Math.ceil((scrollTop + viewport) / rowHeight) + overscan)
  const visible = items.slice(start, end)

  return (
    <div
      ref={containerRef}
      className={`fc-virtual fc-scroll ${className}`}
      id="fc-results"
      role="listbox"
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div className="fc-virtual-canvas" style={{ height: items.length * rowHeight }}>
        <div className="fc-virtual-window" style={{ transform: `translateY(${start * rowHeight}px)` }}>
          {visible.map((item, index) => renderRow(item, start + index))}
        </div>
      </div>
    </div>
  )
}
