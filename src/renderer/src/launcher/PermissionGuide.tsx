import { ShieldCheck, X } from 'lucide-react'
import { useEffect, useRef } from 'react'
import type { PermissionStatus } from '@shared/types'
import { IconButton } from '../components/ui'

/**
 * Post-update TCC repair. macOS pins ad-hoc builds to their code hash, so an
 * update silently orphans the old Accessibility grant; the switch may still
 * look enabled. This panel opens the exact pane and offers a `tccutil reset`
 * shortcut instead of making the user hunt through System Settings.
 */
export function PermissionGuide({
  open,
  status,
  busy,
  onClose,
  onOpenSettings,
  onReset,
  onRecheck,
  onRestart
}: {
  open: boolean
  status: PermissionStatus
  busy: boolean
  onClose: () => void
  onOpenSettings: () => void
  onReset: () => void
  onRecheck: () => void
  onRestart: () => void
}): React.JSX.Element | null {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const timer = window.setTimeout(() => panelRef.current?.focus(), 40)
    return () => window.clearTimeout(timer)
  }, [open])

  if (!open) return null

  const lead = status.stale
    ? 'Flashcast 更新后，macOS 还记着旧版本的授权。开关看起来可能是开的，但当前版本已经对不上，需要重新登记。'
    : status.previouslyGranted
      ? '自动粘贴的辅助功能授权当前不可用，可能是被手动撤销，或系统重置了授权。'
      : '自动粘贴需要在辅助功能里允许 Flashcast。'

  return (
    <div className="fc-help-layer" role="dialog" aria-modal="true" aria-label="自动粘贴权限">
      <div className="fc-help-backdrop" onClick={onClose} aria-hidden="true" />
      <div className="fc-help-panel fc-permission-panel" ref={panelRef} tabIndex={-1}>
        <div className="fc-help-head">
          <div className="fc-permission-heading">
            <span className="fc-permission-icon" aria-hidden="true">
              <ShieldCheck size={17} strokeWidth={1.6} />
            </span>
            <div>
              <div className="fc-help-title">自动粘贴需要重新授权</div>
              <div className="fc-help-subtitle">只关开开关通常不够，需要移除旧条目</div>
            </div>
          </div>
          <IconButton label="稍后处理" onClick={onClose}>
            <X size={15} strokeWidth={1.75} aria-hidden="true" />
          </IconButton>
        </div>

        <div className="fc-permission-body">
          <p className="fc-permission-lead">{lead}</p>
          <ol className="fc-permission-steps">
            <li>点下方「打开辅助功能设置」，直接进入对应面板。</li>
            <li>在列表里选中 Flashcast，用「−」移除旧条目。</li>
            <li>再点「+」选择 /Applications/Flashcast.app，并打开开关。</li>
            <li>回到 Flashcast 点「重新检查」；如果仍不生效，重启一次应用。</li>
          </ol>

          <div className="fc-permission-status">
            <span className={status.accessibility ? 'is-ok' : 'is-bad'}>
              辅助功能 {status.accessibility ? '已授权' : '未授权'}
            </span>
            <span className={status.postEvent ? 'is-ok' : 'is-bad'}>
              按键权限 {status.postEvent ? '已授权' : '未授权'}
            </span>
            <span className="fc-permission-version">当前版本 {status.version}</span>
          </div>

          <div className="fc-permission-actions">
            <button type="button" className="fc-btn fc-btn--primary" onClick={onOpenSettings}>
              打开辅助功能设置
            </button>
            <button type="button" className="fc-btn" disabled={busy} onClick={onRecheck}>
              重新检查
            </button>
            <button type="button" className="fc-btn" disabled={busy} onClick={onReset}>
              重置旧授权并重新授权
            </button>
            <button type="button" className="fc-btn fc-btn--ghost" disabled={busy} onClick={onRestart}>
              重启 Flashcast
            </button>
          </div>
          <p className="fc-permission-note">
            「重置旧授权」只会清除 Flashcast 自己在这一项上的记录，不会影响其他应用；之后仍然需要你在系统设置里打开开关。
          </p>
        </div>
      </div>
    </div>
  )
}
