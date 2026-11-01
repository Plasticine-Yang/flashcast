import { AlertTriangle, Check, ChevronDown, X } from 'lucide-react'
import { useEffect, useRef, type ReactNode } from 'react'
import { createPortal } from 'react-dom'
import type { GitSyncState, PlatformId } from '@shared/types'

export function Logo({ size = 28 }: { size?: number }): React.JSX.Element {
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden="true">
      <defs>
        <linearGradient id="fc-logo-bg" x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor="#1b2c4d" />
          <stop offset="1" stopColor="#09101f" />
        </linearGradient>
        <radialGradient id="fc-logo-glow" cx="0.5" cy="0.35" r="0.6">
          <stop offset="0" stopColor="#4c8dff" stopOpacity="0.45" />
          <stop offset="1" stopColor="#4c8dff" stopOpacity="0" />
        </radialGradient>
        <linearGradient id="fc-logo-bolt" x1="0.2" y1="0" x2="0.8" y2="1">
          <stop offset="0" stopColor="#d9ecff" />
          <stop offset="0.5" stopColor="#6fa8ff" />
          <stop offset="1" stopColor="#2563eb" />
        </linearGradient>
      </defs>
      <rect width="64" height="64" rx="16" fill="url(#fc-logo-bg)" />
      <rect width="64" height="64" rx="16" fill="url(#fc-logo-glow)" />
      <rect x="0.75" y="0.75" width="62.5" height="62.5" rx="15.25" fill="none" stroke="#d8eaff" strokeOpacity="0.16" />
      <path d="M36 8 L19 35 H30 L25 56 L46 27 H34 L41 8 Z" fill="url(#fc-logo-bolt)" />
    </svg>
  )
}

export function PageHeader({
  title,
  description,
  actions
}: {
  title: string
  description?: string
  actions?: ReactNode
}): React.JSX.Element {
  return (
    <header className="settings-page-header">
      <div>
        <h1 className="settings-page-title">{title}</h1>
        {description ? <p className="settings-page-description">{description}</p> : null}
      </div>
      {actions ? <div className="settings-page-actions">{actions}</div> : null}
    </header>
  )
}

export function Section({
  title,
  description,
  children,
  actions
}: {
  title?: string
  description?: string
  children: ReactNode
  actions?: ReactNode
}): React.JSX.Element {
  return (
    <section className="settings-section">
      {title ? (
        <div className="settings-section-head">
          <div>
            <h2 className="settings-section-title">{title}</h2>
            {description ? <p className="settings-section-description">{description}</p> : null}
          </div>
          {actions ? <div className="settings-section-actions">{actions}</div> : null}
        </div>
      ) : null}
      {children}
    </section>
  )
}

export function Row({
  label,
  description,
  children,
  vertical = false
}: {
  label: string
  description?: string
  children: ReactNode
  vertical?: boolean
}): React.JSX.Element {
  return (
    <div className={`settings-row${vertical ? ' settings-row--vertical' : ''}`}>
      <div className="settings-row-copy">
        <div className="settings-row-label">{label}</div>
        {description ? <div className="settings-row-description">{description}</div> : null}
      </div>
      <div className="settings-row-control">{children}</div>
    </div>
  )
}

export function Select({
  value,
  onChange,
  options,
  disabled,
  label
}: {
  value: string
  onChange: (value: string) => void
  options: Array<{ value: string; label: string }>
  disabled?: boolean
  label: string
}): React.JSX.Element {
  return (
    <div className="settings-select-wrap">
      <select
        className="settings-select"
        value={value}
        onChange={(event) => onChange(event.target.value)}
        disabled={disabled}
        aria-label={label}
      >
        {options.map((option) => (
          <option value={option.value} key={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
    </div>
  )
}

export function InlineSpinner({ label }: { label?: string }): React.JSX.Element {
  return (
    <span className="settings-inline-status" role="status">
      <span className="fc-spinner" aria-hidden="true" />
      {label}
    </span>
  )
}

export function InlineStatus({
  tone,
  children
}: {
  tone: 'success' | 'warning' | 'danger' | 'default'
  children: ReactNode
}): React.JSX.Element {
  const Icon = tone === 'success' ? Check : tone === 'default' ? null : AlertTriangle
  return (
    <span className={`settings-inline-status settings-inline-status--${tone}`}>
      {Icon ? <Icon size={12} strokeWidth={2} aria-hidden="true" /> : null}
      {children}
    </span>
  )
}

const SYNC_LABELS: Record<GitSyncState, { label: string; tone: string }> = {
  synced: { label: '已同步', tone: 'success' },
  dirty: { label: '有未提交改动', tone: 'warning' },
  conflict: { label: '存在冲突', tone: 'danger' },
  'no-remote': { label: '未配置远程', tone: 'default' },
  error: { label: '同步失败', tone: 'danger' },
  'not-a-repo': { label: '非 Git 仓库', tone: 'danger' }
}

export function SyncChip({ state }: { state: GitSyncState }): React.JSX.Element {
  const info = SYNC_LABELS[state]
  return <span className={`settings-chip settings-chip--${info.tone}`}>{info.label}</span>
}

export function StatusChip({
  tone,
  children
}: {
  tone: 'success' | 'warning' | 'danger' | 'default' | 'accent'
  children: ReactNode
}): React.JSX.Element {
  return <span className={`settings-chip settings-chip--${tone}`}>{children}</span>
}

export function PlatformName({ platform }: { platform: PlatformId }): string {
  if (platform === 'darwin') return 'macOS'
  if (platform === 'win32') return 'Windows'
  return 'Linux'
}

export function Modal({
  open,
  title,
  description,
  onClose,
  children,
  footer,
  width = 520,
  danger = false
}: {
  open: boolean
  title: string
  description?: string
  onClose: () => void
  children: ReactNode
  footer?: ReactNode
  width?: number
  danger?: boolean
}): React.JSX.Element | null {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const timer = window.setTimeout(() => {
      const panel = panelRef.current
      if (!panel) return
      const preferred = panel.querySelector<HTMLElement>(
        'input:not([disabled]), select:not([disabled]), textarea:not([disabled])'
      )
      ;(preferred ?? panel).focus()
    }, 20)
    const handler = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
        return
      }
      if (event.key === 'Tab' && panelRef.current) {
        const panel = panelRef.current
        const nodes = Array.from(
          panel.querySelectorAll<HTMLElement>(
            'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'
          )
        ).filter((node) => node.offsetParent !== null)
        if (nodes.length === 0) {
          event.preventDefault()
          panel.focus()
          return
        }
        const first = nodes[0]
        const last = nodes[nodes.length - 1]
        if (
          event.shiftKey &&
          (document.activeElement === first ||
            document.activeElement === panel ||
            !panel.contains(document.activeElement))
        ) {
          event.preventDefault()
          last.focus()
        } else if (!event.shiftKey && document.activeElement === last) {
          event.preventDefault()
          first.focus()
        }
      }
    }
    window.addEventListener('keydown', handler)
    return () => {
      window.clearTimeout(timer)
      window.removeEventListener('keydown', handler)
    }
  }, [open, onClose])

  if (!open) return null

  return createPortal(
    <div className="settings-modal-layer">
      <div className="settings-modal-backdrop" onClick={onClose} aria-hidden="true" />
      <div
        className={`settings-modal${danger ? ' settings-modal--danger' : ''}`}
        style={{ width }}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        tabIndex={-1}
        ref={panelRef}
      >
        <div className="settings-modal-head">
          <div>
            <h2 className="settings-modal-title">{title}</h2>
            {description ? <p className="settings-modal-description">{description}</p> : null}
          </div>
          <button
            type="button"
            className="settings-icon-button"
            aria-label="关闭"
            onClick={onClose}
          >
            <X size={15} strokeWidth={1.75} aria-hidden="true" />
          </button>
        </div>
        <div className="settings-modal-body">{children}</div>
        {footer ? <div className="settings-modal-footer">{footer}</div> : null}
      </div>
    </div>,
    document.body
  )
}
