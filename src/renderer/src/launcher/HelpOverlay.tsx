import { X } from 'lucide-react'
import { useEffect, useRef } from 'react'
import type { PlatformId } from '@shared/types'
import { clearAllAccelerator, keycapParts, vimKeycap } from '@shared/shortcuts'
import { AcceleratorKeys, IconButton, Keycaps } from '../components/ui'

interface ShortcutRow {
  label: string
  keys?: string[]
  accelerator?: string
}

function shortcutRow(label: string, accelerator: string): ShortcutRow {
  return { label, accelerator }
}

export function HelpOverlay({
  open,
  onClose,
  platform,
  launcherAccelerator,
  clipboardAccelerator,
  activePluginId,
  settingsActive
}: {
  open: boolean
  onClose: () => void
  platform: PlatformId
  launcherAccelerator: string
  clipboardAccelerator: string
  activePluginId: string | null
  settingsActive: boolean
}): React.JSX.Element | null {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (!open) return
    const timer = window.setTimeout(() => panelRef.current?.focus(), 40)
    return () => window.clearTimeout(timer)
  }, [open])

  if (!open) return null

  const globalRows: ShortcutRow[] = [
    shortcutRow('打开 / 关闭 Flashcast', launcherAccelerator),
    shortcutRow('直接进入剪切板', clipboardAccelerator),
    { label: '关闭窗口', keys: ['esc'] },
    { label: '移动选中项', keys: [...vimKeycap('j', platform), '/', ...vimKeycap('k', platform)] },
    { label: '打开选中项', keys: ['↵'] },
    { label: '快捷键帮助', keys: ['?'] }
  ]

  const navRows: ShortcutRow[] = [
    { label: '向下移动', keys: vimKeycap('j', platform) },
    { label: '向上移动', keys: vimKeycap('k', platform) },
    { label: '向右 / 展开预览', keys: vimKeycap('l', platform) },
    { label: '向左 / 收起预览', keys: vimKeycap('h', platform) },
    { label: '方向键辅助', keys: ['↑', '↓'] },
    { label: '切换焦点', keys: ['Tab'] }
  ]

  const clipboardRows: ShortcutRow[] = [
    { label: '置顶并关闭', keys: ['↵'] },
    { label: '置顶、关闭并粘贴', keys: [keycapParts('Alt+Enter', platform)[0], '↵'] },
    { label: '删除选中项', keys: [keycapParts('Control+Backspace', platform)[0], '⌫'] },
    { label: '清空全部（二次确认）', keys: keycapParts(clearAllAccelerator(platform), platform) },
    { label: '立即展开预览', keys: vimKeycap('l', platform) },
    { label: '收起预览', keys: vimKeycap('h', platform) }
  ]

  const bookmarkRows: ShortcutRow[] = [
    { label: '在 Chrome 中打开', keys: ['↵'] },
    { label: '移动选中项', keys: [...vimKeycap('j', platform), '/', ...vimKeycap('k', platform)] },
    { label: '退出插件', keys: ['esc'] }
  ]

  const settingsRows: ShortcutRow[] = [
    { label: '返回插件主页', keys: ['esc'] },
    { label: '切换到下一个字段', keys: ['Tab'] },
    { label: '在字段间移动', keys: [...vimKeycap('j', platform), '/', ...vimKeycap('k', platform)] }
  ]

  return (
    <div className="fc-help-layer" role="dialog" aria-modal="true" aria-label="键盘快捷键">
      <div className="fc-help-backdrop" onClick={onClose} aria-hidden="true" />
      <div className="fc-help-panel" ref={panelRef} tabIndex={-1}>
        <div className="fc-help-head">
          <div className="fc-help-title">键盘快捷键</div>
          <span className="fc-help-subtitle">任意时刻按 esc 关闭</span>
          <IconButton label="关闭快捷键帮助" onClick={onClose}>
            <X size={15} strokeWidth={1.75} aria-hidden="true" />
          </IconButton>
        </div>
        <div className="fc-help-body fc-scroll">
          <HelpSection title="全局" rows={globalRows} platform={platform} />
          <HelpSection title="光标与焦点" rows={navRows} platform={platform} />
          {activePluginId === 'clipboard' ? (
            <HelpSection title="剪切板" rows={clipboardRows} platform={platform} />
          ) : null}
          {activePluginId === 'chrome-bookmarks' ? (
            <HelpSection title="Chrome 书签" rows={bookmarkRows} platform={platform} />
          ) : null}
          {activePluginId && settingsActive ? (
            <HelpSection title="插件设置" rows={settingsRows} platform={platform} />
          ) : null}
        </div>
      </div>
    </div>
  )
}

function HelpSection({
  title,
  rows,
  platform
}: {
  title: string
  rows: ShortcutRow[]
  platform: PlatformId
}): React.JSX.Element {
  return (
    <section className="fc-help-section">
      <div className="fc-help-section-title">{title}</div>
      <dl>
        {rows.map((row) => (
          <div className="fc-help-row" key={row.label}>
            <dt>{row.label}</dt>
            <dd>
              {row.accelerator ? (
                <AcceleratorKeys accelerator={row.accelerator} platform={platform} />
              ) : (
                <Keycaps parts={row.keys ?? []} />
              )}
            </dd>
          </div>
        ))}
      </dl>
    </section>
  )
}
