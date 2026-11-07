import {
  AlertTriangle,
  Check,
  Download,
  FolderOpen,
  Monitor,
  Moon,
  MoreHorizontal,
  Palette,
  Plus,
  RefreshCw,
  Sun,
  Trash2
} from 'lucide-react'
import { useRef, useState } from 'react'
import type { ThemeDefinition, ThemeMode, ThemePreference } from '@shared/theme'
import { supportedThemeModes } from '@shared/theme'
import type { BootstrapData, ThemeInspection } from '@shared/types'
import { api } from '../bridge'
import { InlineSpinner, Modal, PageHeader, Section, StatusChip } from './components'

type ToastTone = 'default' | 'success' | 'warning' | 'danger'

const MODE_LABELS: Record<ThemePreference, string> = {
  system: '跟随系统',
  light: '浅色',
  dark: '深色'
}

const MODE_OPTIONS: Array<{ value: ThemePreference; label: string; icon: React.JSX.Element }> = [
  {
    value: 'system',
    label: '跟随系统',
    icon: <Monitor size={14} strokeWidth={1.5} aria-hidden="true" />
  },
  { value: 'light', label: '浅色', icon: <Sun size={14} strokeWidth={1.5} aria-hidden="true" /> },
  { value: 'dark', label: '深色', icon: <Moon size={14} strokeWidth={1.5} aria-hidden="true" /> }
]

function modeBadge(theme: ThemeDefinition): string {
  const modes = supportedThemeModes(theme)
  if (modes.length > 1) {
    return modes
      .map(
        (mode) =>
          theme.variants[mode]?.label ?? (mode === 'dark' ? '深色' : '浅色')
      )
      .join(' / ')
  }
  return modes[0] === 'light' ? '仅浅色' : '仅深色'
}

function modeDescription(theme: ThemeDefinition): string {
  const modes = supportedThemeModes(theme)
  if (modes.length > 1) return '深色与浅色'
  return modes[0] === 'light' ? '仅浅色' : '仅深色'
}

/**
 * A miniature of the launcher painted with the theme's real tokens. Two panes
 * appear for dual-mode packs so the dark/light pair is visible at a glance.
 */
function ThemePreview({ theme }: { theme: ThemeDefinition }): React.JSX.Element {
  const modes: ThemeMode[] = []
  if (theme.variants.dark) modes.push('dark')
  if (theme.variants.light) modes.push('light')

  return (
    <span className="settings-theme-card-preview" aria-hidden="true">
      {modes.map((mode) => {
        const variant = theme.variants[mode]
        if (!variant) return null
        return (
          <span
            key={mode}
            className="settings-theme-pane"
            style={{
              background: `${variant.tokens.glassSheen}, ${variant.tokens.window}`,
              borderColor: variant.tokens.glassBorder,
              color: variant.tokens.textPrimary
            }}
          >
            <span
              className="settings-theme-pane-bar"
              style={{
                background: variant.tokens.surface2,
                borderColor: variant.tokens.borderSubtle
              }}
            />
            <span
              className="settings-theme-pane-row"
              style={{
                background: variant.tokens.surface1,
                borderColor: variant.tokens.borderSubtle
              }}
            >
              <span
                className="settings-theme-pane-line"
                style={{ background: variant.tokens.textTertiary }}
              />
            </span>
            <span
              className="settings-theme-pane-row is-selected"
              style={{
                background: variant.tokens.rowSelectedBg,
                borderColor: variant.tokens.rowSelectedRim
              }}
            >
              <span
                className="settings-theme-pane-dot"
                style={{ background: variant.tokens.accent }}
              />
              <span
                className="settings-theme-pane-line"
                style={{ background: variant.tokens.textSecondary }}
              />
            </span>
          </span>
        )
      })}
    </span>
  )
}

export function AppearancePage({
  data,
  onData,
  onToast
}: {
  data: BootstrapData
  onData: (data: BootstrapData) => void
  onToast: (message: string, tone?: ToastTone) => void
}): React.JSX.Element {
  const [busy, setBusy] = useState<string | null>(null)
  const [announcement, setAnnouncement] = useState('')
  const [menuFor, setMenuFor] = useState<string | null>(null)
  const [installOpen, setInstallOpen] = useState(false)
  const [installInput, setInstallInput] = useState('')
  const [inspection, setInspection] = useState<ThemeInspection | null>(null)
  const [installError, setInstallError] = useState<string | null>(null)
  const [uninstallTarget, setUninstallTarget] = useState<ThemeDefinition | null>(null)
  const cardRefs = useRef<Map<string, HTMLButtonElement>>(new Map())
  const modeRefs = useRef<Record<string, HTMLButtonElement | null>>({})

  const themes = data.themes ?? []
  const errors = data.themeErrors ?? []
  const activeTheme = themes.find((theme) => theme.id === data.themeId) ?? null
  const supportedModes =
    data.themeSupportedModes ?? (activeTheme ? supportedThemeModes(activeTheme) : [])
  const lockedMode: ThemeMode | null = supportedModes.length === 1 ? supportedModes[0] : null
  const dualMode = supportedModes.length > 1
  const hasSelection = themes.some((theme) => theme.id === data.prefs.themeId)

  const registerCard = (id: string) => (node: HTMLButtonElement | null): void => {
    if (node) cardRefs.current.set(id, node)
    else cardRefs.current.delete(id)
  }

  const refresh = async (): Promise<void> => {
    try {
      const next = await api.getBootstrap()
      onData(next)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    }
  }

  const updatePrefs = async (
    patch: Parameters<typeof api.updatePrefs>[0],
    message: string
  ): Promise<void> => {
    setBusy('prefs')
    try {
      const next = await api.updatePrefs(patch)
      onData(next)
      setAnnouncement(message)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(null)
    }
  }

  const selectTheme = async (theme: ThemeDefinition): Promise<void> => {
    if (busy !== null || theme.id === data.prefs.themeId) return
    await updatePrefs({ themeId: theme.id }, `主题已切换为 ${theme.name}`)
  }

  const handleGalleryKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (busy !== null || themes.length === 0) return
    const selectedIndex = themes.findIndex((theme) => theme.id === data.prefs.themeId)
    const base = selectedIndex >= 0 ? selectedIndex : -1
    let nextIndex: number
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        nextIndex = Math.min(themes.length - 1, base + 1)
        break
      case 'ArrowLeft':
      case 'ArrowUp':
        nextIndex = Math.max(0, base - 1)
        break
      case 'Home':
        nextIndex = 0
        break
      case 'End':
        nextIndex = themes.length - 1
        break
      default:
        return
    }
    event.preventDefault()
    const next = themes[nextIndex]
    if (!next) return
    cardRefs.current.get(next.id)?.focus()
    void selectTheme(next)
  }

  const handleModeKeyDown = (event: React.KeyboardEvent<HTMLDivElement>): void => {
    if (busy !== null) return
    const selectedIndex = MODE_OPTIONS.findIndex((option) => option.value === data.prefs.theme)
    const base = selectedIndex >= 0 ? selectedIndex : 0
    let nextIndex: number
    switch (event.key) {
      case 'ArrowRight':
      case 'ArrowDown':
        nextIndex = (base + 1) % MODE_OPTIONS.length
        break
      case 'ArrowLeft':
      case 'ArrowUp':
        nextIndex = (base - 1 + MODE_OPTIONS.length) % MODE_OPTIONS.length
        break
      case 'Home':
        nextIndex = 0
        break
      case 'End':
        nextIndex = MODE_OPTIONS.length - 1
        break
      default:
        return
    }
    event.preventDefault()
    const option = MODE_OPTIONS[nextIndex]
    if (!option) return
    modeRefs.current[option.value]?.focus()
    if (option.value !== data.prefs.theme) {
      void updatePrefs({ theme: option.value }, `外观模式已切换为${option.label}`)
    }
  }

  const reloadThemes = async (): Promise<void> => {
    if (busy !== null) return
    setBusy('reload')
    try {
      await api.reloadThemes()
      await refresh()
      onToast('已重新加载工作区主题', 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(null)
    }
  }

  const openThemeDirectory = async (theme: ThemeDefinition): Promise<void> => {
    setMenuFor(null)
    try {
      await api.openThemeDirectory(theme.id)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    }
  }

  const updateTheme = async (theme: ThemeDefinition): Promise<void> => {
    if (busy !== null) return
    setMenuFor(null)
    setBusy(`update:${theme.id}`)
    try {
      const result = await api.updateTheme(theme.id)
      if (!result.ok) {
        onToast(result.error ?? '主题更新失败', 'danger')
        return
      }
      await refresh()
      if (result.toast) onToast(result.toast.message, result.toast.tone)
      else onToast(`${theme.name} 已更新`, 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(null)
    }
  }

  const uninstallTheme = async (): Promise<void> => {
    if (!uninstallTarget || busy !== null) return
    const target = uninstallTarget
    setBusy('uninstall')
    try {
      const result = await api.uninstallTheme(target.id)
      if (!result.ok) {
        onToast(result.error ?? '卸载主题失败', 'danger')
        return
      }
      setUninstallTarget(null)
      await refresh()
      if (result.toast) onToast(result.toast.message, result.toast.tone)
      else onToast(`已卸载 ${target.name}`, 'success')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(null)
    }
  }

  const openInstall = (): void => {
    setInstallOpen(true)
    setInstallInput('')
    setInspection(null)
    setInstallError(null)
  }

  const closeInstall = (): void => {
    if (busy === 'inspect' || busy === 'install') return
    setInstallOpen(false)
    setInstallInput('')
    setInspection(null)
    setInstallError(null)
  }

  const inspectTheme = async (): Promise<void> => {
    const input = installInput.trim()
    if (!input || busy !== null) return
    setBusy('inspect')
    setInstallError(null)
    setInspection(null)
    try {
      const result = await api.inspectTheme(input)
      setInspection(result)
    } catch (error) {
      setInstallError((error as Error).message)
    } finally {
      setBusy(null)
    }
  }

  const installTheme = async (): Promise<void> => {
    if (!inspection || busy !== null) return
    setBusy('install')
    setInstallError(null)
    try {
      const result = await api.installTheme(inspection.token)
      await refresh()
      setInstallOpen(false)
      setInstallInput('')
      setInspection(null)
      onToast(`已安装主题 ${result.theme.name}`, 'success')
    } catch (error) {
      setInstallError((error as Error).message)
    } finally {
      setBusy(null)
    }
  }

  const cardsDisabled = busy !== null

  return (
    <>
      <PageHeader title="外观" description="主题、深浅模式与工作区主题包。" />
      <span className="sr-only" role="status" aria-live="polite">
        {announcement}
      </span>
      <div className="settings-stack">
        {data.themeFallback ? (
          <div className="settings-error-banner" role="alert">
            <AlertTriangle size={14} strokeWidth={1.75} aria-hidden="true" />
            <div>
              <strong>
                当前主题「{data.themeFallback.requestedId}」不可用，已回退 Blue Glass。
              </strong>
              <span>{data.themeFallback.reason}</span>
            </div>
          </div>
        ) : null}

        {errors.length > 0 ? (
          <div className="settings-error-banner" role="alert">
            <AlertTriangle size={14} strokeWidth={1.75} aria-hidden="true" />
            <div>
              <strong>部分工作区主题未能加载</strong>
              <ul>
                {errors.map((error) => (
                  <li key={error.id}>
                    <code>{error.id}</code>：{error.message}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        ) : null}

        <Section
          title="主题"
          description="主题决定启动器与设置窗口的颜色、材质与对比度。"
          actions={
            <div className="settings-theme-actions">
              <button
                type="button"
                className="settings-ghost-button"
                onClick={() => void reloadThemes()}
                disabled={busy !== null}
              >
                {busy === 'reload' ? (
                  <span className="fc-spinner" aria-hidden="true" />
                ) : (
                  <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />
                )}
                重新加载
              </button>
              <button
                type="button"
                className="settings-primary-button"
                onClick={openInstall}
                disabled={busy !== null}
              >
                <Plus size={14} strokeWidth={2} aria-hidden="true" />
                安装主题
              </button>
            </div>
          }
        >
          {themes.length === 0 ? (
            <div className="settings-theme-empty">还没有可用主题。</div>
          ) : (
            <div
              className="settings-theme-gallery"
              role="radiogroup"
              aria-label="主题"
              aria-busy={cardsDisabled}
              onKeyDown={handleGalleryKeyDown}
            >
              {themes.map((theme, index) => {
                const selected = theme.id === data.prefs.themeId
                return (
                  <div className="settings-theme-card-wrap" key={theme.id} role="none">
                    <button
                      ref={registerCard(theme.id)}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      aria-disabled={cardsDisabled}
                      aria-label={`${theme.name}，${modeDescription(theme)}`}
                      tabIndex={selected || (!hasSelection && index === 0) ? 0 : -1}
                      className={`settings-theme-card${selected ? ' is-active' : ''}`}
                      onClick={() => void selectTheme(theme)}
                    >
                      <ThemePreview theme={theme} />
                      <span className="settings-theme-card-meta">
                        <span className="settings-theme-name-line">
                          <span className="settings-theme-name">{theme.name}</span>
                          {selected ? (
                            <Check
                              className="settings-theme-check"
                              size={13}
                              strokeWidth={2.25}
                              aria-hidden="true"
                            />
                          ) : null}
                        </span>
                        <span className="settings-theme-badges">
                          <StatusChip tone={selected ? 'accent' : 'default'}>
                            {modeBadge(theme)}
                          </StatusChip>
                          <StatusChip tone="default">
                            {theme.source === 'builtin' ? '内置' : '工作区'}
                          </StatusChip>
                        </span>
                      </span>
                    </button>
                    {theme.source === 'workspace' ? (
                      <>
                        <button
                          type="button"
                          className="settings-theme-menu-button"
                          aria-label={`${theme.name} 更多操作`}
                          aria-haspopup="menu"
                          aria-expanded={menuFor === theme.id}
                          disabled={cardsDisabled}
                          onClick={() => setMenuFor(menuFor === theme.id ? null : theme.id)}
                        >
                          <MoreHorizontal size={14} strokeWidth={2} aria-hidden="true" />
                        </button>
                        {menuFor === theme.id ? (
                          <>
                            <div
                              className="settings-menu-backdrop"
                              onClick={() => setMenuFor(null)}
                              aria-hidden="true"
                            />
                            <div
                              className="settings-menu settings-theme-menu-pop"
                              role="menu"
                              aria-label={`${theme.name} 操作`}
                            >
                              <button
                                type="button"
                                role="menuitem"
                                disabled={busy === `update:${theme.id}`}
                                onClick={() => void updateTheme(theme)}
                              >
                                {busy === `update:${theme.id}` ? (
                                  <span className="fc-spinner" aria-hidden="true" />
                                ) : (
                                  <RefreshCw size={14} strokeWidth={1.5} aria-hidden="true" />
                                )}
                                检查更新
                              </button>
                              <button
                                type="button"
                                role="menuitem"
                                onClick={() => void openThemeDirectory(theme)}
                              >
                                <FolderOpen size={14} strokeWidth={1.5} aria-hidden="true" />
                                打开目录
                              </button>
                              <button
                                type="button"
                                role="menuitem"
                                className="is-danger"
                                onClick={() => {
                                  setMenuFor(null)
                                  setUninstallTarget(theme)
                                }}
                              >
                                <Trash2 size={14} strokeWidth={1.5} aria-hidden="true" />
                                卸载
                              </button>
                            </div>
                          </>
                        ) : null}
                      </>
                    ) : null}
                  </div>
                )
              })}
            </div>
          )}
          <p className="settings-hint">
            主题包是纯数据：只有 <code>theme.json</code> 与可选的 <code>LICENSE</code>，不会执行任何代码。
          </p>
        </Section>

        <Section title="外观模式" description="选择主题使用深色还是浅色变体。">
          {dualMode ? (
            <>
              <div
                className="settings-mode-picker"
                role="radiogroup"
                aria-label="外观模式"
                aria-busy={cardsDisabled}
                onKeyDown={handleModeKeyDown}
              >
                {MODE_OPTIONS.map((option) => {
                  const selected = data.prefs.theme === option.value
                  return (
                    <button
                      key={option.value}
                      ref={(node) => {
                        modeRefs.current[option.value] = node
                      }}
                      type="button"
                      role="radio"
                      aria-checked={selected}
                      aria-disabled={cardsDisabled}
                      tabIndex={selected ? 0 : -1}
                      className={`settings-mode-option${selected ? ' is-active' : ''}`}
                      onClick={() => {
                        if (selected || busy !== null) return
                        void updatePrefs(
                          { theme: option.value },
                          `外观模式已切换为${option.label}`
                        )
                      }}
                    >
                      {option.icon}
                      {option.label}
                    </button>
                  )
                })}
              </div>
              <p className="settings-hint">
                跟随系统时使用系统当前模式，当前为{data.theme === 'dark' ? '深色' : '浅色'}。
              </p>
            </>
          ) : (
            <div className="settings-info-banner">
              {lockedMode === 'light' ? (
                <Sun size={15} strokeWidth={1.5} aria-hidden="true" />
              ) : (
                <Moon size={15} strokeWidth={1.5} aria-hidden="true" />
              )}
              <div className="settings-info-copy">
                <div className="settings-info-title">
                  {lockedMode === 'light' ? '仅浅色' : '仅深色'}
                </div>
                <div>
                  {activeTheme?.name ?? '当前主题'} 只提供
                  {lockedMode === 'light' ? '浅色' : '深色'}外观；你的「
                  {MODE_LABELS[data.prefs.theme]}」偏好已保留，切回支持双模式的主题后会继续生效。
                </div>
              </div>
            </div>
          )}
        </Section>
      </div>

      <Modal
        open={installOpen}
        title="安装主题"
        description="输入 Git 仓库地址或本地主题目录。安装前会先校验 theme.json。"
        onClose={closeInstall}
        width={560}
        footer={
          inspection ? (
            <>
              <button
                type="button"
                className="fc-btn"
                onClick={() => {
                  setInspection(null)
                  setInstallError(null)
                }}
                disabled={busy !== null}
              >
                重新检查
              </button>
              <button
                type="button"
                className="fc-btn fc-btn--primary"
                onClick={() => void installTheme()}
                disabled={busy !== null}
              >
                {busy === 'install' ? (
                  <span className="fc-spinner" aria-hidden="true" />
                ) : (
                  <Download size={13} strokeWidth={1.75} aria-hidden="true" />
                )}
                确认安装
              </button>
            </>
          ) : (
            <>
              <button type="button" className="fc-btn" onClick={closeInstall}>
                取消
              </button>
              <button
                type="button"
                className="fc-btn fc-btn--primary"
                disabled={!installInput.trim() || busy !== null}
                onClick={() => void inspectTheme()}
              >
                {busy === 'inspect' ? <span className="fc-spinner" aria-hidden="true" /> : null}
                检查主题
              </button>
            </>
          )
        }
      >
        {!inspection ? (
          <div className="settings-install-form">
            <label className="settings-field-label" htmlFor="theme-source">
              主题来源
            </label>
            <input
              id="theme-source"
              className="settings-input"
              value={installInput}
              onChange={(event) => setInstallInput(event.target.value)}
              placeholder="https://github.com/user/flashcast-theme 或 /path/to/theme"
              spellCheck={false}
              disabled={busy === 'inspect'}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void inspectTheme()
              }}
            />
            <p className="settings-hint">
              主题目录需包含 <code>theme.json</code>，可选 <code>LICENSE</code>。安装后主题会写入{' '}
              <code>{data.workspace.path}/themes/</code> 并提交到工作区 Git 仓库。
            </p>
            {busy === 'inspect' ? <InlineSpinner label="正在校验主题文件…" /> : null}
            {installError ? (
              <div className="settings-form-error" role="alert">
                <AlertTriangle size={13} strokeWidth={2} aria-hidden="true" />
                {installError}
              </div>
            ) : null}
          </div>
        ) : (
          <div className="settings-inspection">
            <div className="settings-inspection-head">
              <div className="settings-plugin-icon settings-plugin-icon--lg" aria-hidden="true">
                <Palette size={24} strokeWidth={1.5} />
              </div>
              <div className="settings-theme-install-copy">
                <div className="settings-inspection-name">
                  {inspection.definition.name}
                  <span className="settings-plugin-version">v{inspection.definition.version}</span>
                </div>
                <div className="settings-plugin-meta">
                  {inspection.sourceType === 'git' ? 'Git 仓库' : '本地目录'} · {inspection.source}
                </div>
              </div>
              <StatusChip tone={inspection.alreadyInstalled ? 'warning' : 'success'}>
                {inspection.alreadyInstalled
                  ? `已安装 v${inspection.installedVersion ?? '?'}，将覆盖`
                  : '可安装'}
              </StatusChip>
            </div>
            <p className="settings-inspection-description">
              {inspection.definition.description || '主题没有提供描述。'}
            </p>
            <div className="settings-theme-install-preview">
              <ThemePreview theme={inspection.definition} />
            </div>
            <div className="settings-kv">
              <span>主题 ID</span>
              <code>{inspection.definition.id}</code>
            </div>
            <div className="settings-kv">
              <span>作者</span>
              <span>{inspection.definition.author}</span>
            </div>
            <div className="settings-kv">
              <span>许可</span>
              <span>{inspection.definition.license}</span>
            </div>
            <div className="settings-kv">
              <span>模式</span>
              <span>
                {supportedThemeModes(inspection.definition)
                  .map((mode) => {
                    const variant = inspection.definition.variants[mode]
                    return `${MODE_LABELS[mode]}${variant?.label ? ` ${variant.label}` : ''}`
                  })
                  .join(' · ') || '无'}
              </span>
            </div>
            {inspection.warnings.length > 0 ? (
              <div className="settings-warning">
                <AlertTriangle size={14} strokeWidth={1.75} aria-hidden="true" />
                <div>
                  {inspection.warnings.map((warning, index) => (
                    <div key={`${warning.path}-${index}`}>
                      {warning.path ? `${warning.path}：` : ''}
                      {warning.message}
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
            {installError ? (
              <div className="settings-form-error" role="alert">
                <AlertTriangle size={13} strokeWidth={2} aria-hidden="true" />
                {installError}
              </div>
            ) : null}
          </div>
        )}
      </Modal>

      <Modal
        open={uninstallTarget !== null}
        title="卸载主题"
        description={
          uninstallTarget
            ? `将删除 ${uninstallTarget.name} 的主题文件，此操作无法撤销。`
            : undefined
        }
        onClose={() => {
          if (busy !== 'uninstall') setUninstallTarget(null)
        }}
        danger
        width={420}
        footer={
          <>
            <button
              type="button"
              className="fc-btn"
              onClick={() => setUninstallTarget(null)}
              disabled={busy === 'uninstall'}
            >
              取消
            </button>
            <button
              type="button"
              className="fc-btn fc-btn--danger"
              onClick={() => void uninstallTheme()}
              disabled={busy === 'uninstall'}
            >
              {busy === 'uninstall' ? (
                <span className="fc-spinner" aria-hidden="true" />
              ) : (
                <Trash2 size={13} strokeWidth={1.75} aria-hidden="true" />
              )}
              确认卸载
            </button>
          </>
        }
      >
        <p className="settings-hint">
          主题文件位于工作区的 themes 目录；卸载同样会提交到工作区 Git 仓库。
        </p>
      </Modal>
    </>
  )
}
