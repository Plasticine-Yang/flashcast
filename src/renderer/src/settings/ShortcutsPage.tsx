import { AlertTriangle, Keyboard, RotateCcw } from 'lucide-react'
import { useEffect, useState } from 'react'
import type { BootstrapData, ShortcutError, ShortcutInfo } from '@shared/types'
import {
  acceleratorFromEvent,
  clearAllAccelerator,
  formatAccelerator,
  keycapParts
} from '@shared/shortcuts'
import { api } from '../bridge'
import { AcceleratorKeys, Keycaps } from '../components/ui'
import { PageHeader, Section, Modal, InlineSpinner } from './components'

type ToastTone = 'default' | 'success' | 'warning' | 'danger'

/** `system` 诊断描述的是环境问题（如 Wayland portal 身份缺失），不是本次保存失败。 */
function registrationErrors(errors: ShortcutError[]): ShortcutError[] {
  return errors.filter((error) => error.id !== 'system')
}

export function ShortcutsPage({
  data,
  onData,
  onToast
}: {
  data: BootstrapData
  onData: (data: BootstrapData) => void
  onToast: (message: string, tone?: ToastTone) => void
}): React.JSX.Element {
  const [recording, setRecording] = useState<ShortcutInfo | null>(null)
  const [candidate, setCandidate] = useState<string | null>(null)
  const [conflict, setConflict] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [resetting, setResetting] = useState(false)
  const platform = data.platform
  const metaKeyLabel = platform === 'darwin' ? '⌘' : platform === 'win32' ? 'Win' : 'Super'

  useEffect(() => {
    if (!recording) return
    const handler = (event: KeyboardEvent): void => {
      if (event.key === 'Escape') {
        event.preventDefault()
        setRecording(null)
        return
      }
      event.preventDefault()
      const accelerator = acceleratorFromEvent(event)
      if (accelerator) {
        const other = data.shortcuts.find(
          (shortcut) => shortcut.id !== recording.id && shortcut.accelerator === accelerator
        )
        if (other) {
          setCandidate(accelerator)
          setConflict(`与「${other.label}」冲突，请换一个组合`)
          return
        }
        setCandidate(accelerator)
        setConflict(null)
        return
      }
      if (event.key === 'Enter' && candidate && !conflict) {
        void save(candidate)
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [recording, candidate, conflict, data.shortcuts])

  const save = async (accelerator: string): Promise<void> => {
    if (!recording || !accelerator) return
    setSaving(true)
    try {
      const result = await api.setShortcuts({ [recording.id]: accelerator })
      const failures = registrationErrors(result.errors)
      if (failures.length > 0) {
        setConflict(failures[0].message)
        return
      }
      onData({ ...data, shortcuts: result.shortcuts, shortcutErrors: result.errors })
      onToast('快捷键已更新', 'success')
      setRecording(null)
      setCandidate(null)
      setConflict(null)
    } catch (error) {
      setConflict((error as Error).message)
    } finally {
      setSaving(false)
    }
  }

  const reset = async (): Promise<void> => {
    setResetting(true)
    try {
      const result = await api.resetShortcuts()
      const failed = registrationErrors(result.errors).length > 0
      onData({ ...data, shortcuts: result.shortcuts, shortcutErrors: result.errors })
      onToast(failed ? '已恢复默认，但部分快捷键注册失败' : '已恢复默认快捷键', failed ? 'warning' : 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setResetting(false)
    }
  }

  const startRecording = (shortcut: ShortcutInfo): void => {
    setRecording(shortcut)
    setCandidate(null)
    setConflict(null)
  }

  return (
    <>
      <PageHeader
        title="快捷键"
        description="全局快捷键在任何应用中都生效；导航快捷键与选中移动在启动器内始终可用。"
        actions={
          <button type="button" className="settings-ghost-button" onClick={() => void reset()} disabled={resetting}>
            {resetting ? <span className="fc-spinner" aria-hidden="true" /> : <RotateCcw size={13} strokeWidth={1.75} aria-hidden="true" />}
            恢复默认
          </button>
        }
      />
      <div className="settings-stack">
        {data.shortcutErrors.length > 0 ? (
          <div className="settings-error-banner" role="alert">
            <AlertTriangle size={15} strokeWidth={1.75} aria-hidden="true" />
            <div>
              <strong>全局快捷键未能生效</strong>
              <ul>
                {data.shortcutErrors.map((error) => (
                  <li key={`${error.id}-${error.accelerator}`}>
                    {error.accelerator}：{error.message}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        ) : null}

        <Section title="全局快捷键" description={`点击「录制」后按下新的组合键，至少包含 Ctrl / Alt / ${metaKeyLabel} 中一个修饰键。`}>
          <div className="settings-shortcut-list">
            {data.shortcuts.map((shortcut) => {
              const rowError = data.shortcutErrors.find((error) => error.id === shortcut.id)
              return (
                <div className="settings-shortcut-row" key={shortcut.id}>
                  <div className="settings-row-copy">
                    <div className="settings-row-label">{shortcut.label}</div>
                    <div className="settings-row-description">{shortcut.description}</div>
                    {rowError ? <div className="settings-plugin-error">{rowError.message}</div> : null}
                  </div>
                  <div className="settings-shortcut-actions">
                    <AcceleratorKeys accelerator={shortcut.accelerator} platform={platform} />
                    {shortcut.accelerator !== shortcut.defaultAccelerator ? (
                      <button
                        type="button"
                        className="settings-link-button"
                        onClick={async () => {
                          setSaving(true)
                          try {
                            const result = await api.setShortcuts({
                              [shortcut.id]: shortcut.defaultAccelerator
                            })
                            const failures = registrationErrors(result.errors)
                            if (failures.length > 0) {
                              onToast(failures[0].message, 'danger')
                            } else {
                              onData({ ...data, shortcuts: result.shortcuts, shortcutErrors: result.errors })
                              onToast('已恢复默认组合', 'success')
                            }
                          } catch (error) {
                            onToast((error as Error).message, 'danger')
                          } finally {
                            setSaving(false)
                          }
                        }}
                      >
                        默认
                      </button>
                    ) : null}
                    <button
                      type="button"
                      className="fc-btn"
                      onClick={() => startRecording(shortcut)}
                    >
                      <Keyboard size={13} strokeWidth={1.75} aria-hidden="true" />
                      录制
                    </button>
                  </div>
                </div>
              )
            })}
          </div>
        </Section>

        <Section title="启动器内快捷键" description="这一组快捷键为固定映射，避免与系统全局快捷键冲突。">
          <div className="settings-fixed-list">
            <FixedRow label="向下移动光标" keys={vimKeys(platform, 'j')} />
            <FixedRow label="向上移动光标" keys={vimKeys(platform, 'k')} />
            <FixedRow label="设置页：下一个字段" keys={vimKeys(platform, 'l')} />
            <FixedRow label="设置页：回到输入框" keys={vimKeys(platform, 'h')} />
            <FixedRow label="打开选中项" keys={['↵']} />
            <FixedRow label="关闭窗口 / 返回上一层" keys={['esc']} />
            <FixedRow label="快捷键帮助" keys={['?']} />
            <FixedRow label="剪切板：置顶并关闭" keys={['↵']} />
            <FixedRow label="剪切板：置顶、关闭并粘贴" keys={keycapParts('Alt+Enter', platform)} />
            <FixedRow label="剪切板：删除选中项" keys={keycapParts('Control+Backspace', platform)} />
            <FixedRow label="剪切板：清空全部（二次确认）" keys={keycapParts(clearAllAccelerator(platform), platform)} />
          </div>
        </Section>
      </div>

      <Modal
        open={recording !== null}
        title="录制快捷键"
        description={recording ? `${recording.label} · 按下新的组合键` : undefined}
        onClose={() => setRecording(null)}
        width={460}
        footer={
          <>
            <button type="button" className="fc-btn" onClick={() => setRecording(null)}>
              取消
            </button>
            <button
              type="button"
              className="fc-btn fc-btn--primary"
              disabled={!candidate || Boolean(conflict) || saving}
              onClick={() => candidate && void save(candidate)}
            >
              {saving ? <span className="fc-spinner" aria-hidden="true" /> : null}
              保存
            </button>
          </>
        }
      >
        <div className="settings-recorder">
          <div className={`settings-recorder-box${candidate ? ' has-value' : ''}`}>
            {candidate ? (
              <Keycaps parts={keycapParts(candidate, platform)} />
            ) : (
              <span>按下新的组合键…</span>
            )}
          </div>
          <div className="settings-recorder-hint">
            当前：<AcceleratorKeys accelerator={recording?.accelerator ?? ''} platform={platform} />
            {candidate ? (
              <span className="settings-recorder-preview">
                新组合：{formatAccelerator(candidate, platform)}
              </span>
            ) : null}
          </div>
          {conflict ? (
            <div className="settings-form-error" role="alert">
              <AlertTriangle size={13} strokeWidth={2} aria-hidden="true" />
              {conflict}
            </div>
          ) : null}
          {saving ? <InlineSpinner label="正在注册快捷键…" /> : null}
        </div>
      </Modal>
    </>
  )
}

function vimKeys(platform: BootstrapData['platform'], letter: string): string[] {
  return [platform === 'darwin' ? '⌃' : 'Ctrl', letter.toUpperCase()]
}

function FixedRow({ label, keys }: { label: string; keys: string[] }): React.JSX.Element {
  return (
    <div className="settings-fixed-row">
      <span>{label}</span>
      <Keycaps parts={keys} />
    </div>
  )
}
