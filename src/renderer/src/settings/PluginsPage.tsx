import {
  AlertTriangle,
  Download,
  FolderOpen,
  MoreHorizontal,
  Plus,
  Puzzle,
  RefreshCw,
  ShieldAlert,
  Trash2
} from 'lucide-react'
import { useState } from 'react'
import type { BootstrapData, PluginInspection, PluginSummary } from '@shared/types'
import { api } from '../bridge'
import { Glyph, Switch } from '../components/ui'
import {
  InlineSpinner,
  Modal,
  PageHeader,
  Section,
  StatusChip
} from './components'

type ToastTone = 'default' | 'success' | 'warning' | 'danger'

export function PluginsPage({
  data,
  onData,
  onToast
}: {
  data: BootstrapData
  onData: (data: BootstrapData) => void
  onToast: (message: string, tone?: ToastTone) => void
}): React.JSX.Element {
  const [installOpen, setInstallOpen] = useState(false)
  const [installInput, setInstallInput] = useState('')
  const [inspection, setInspection] = useState<PluginInspection | null>(null)
  const [inspecting, setInspecting] = useState(false)
  const [installing, setInstalling] = useState(false)
  const [installError, setInstallError] = useState<string | null>(null)
  const [uninstallTarget, setUninstallTarget] = useState<PluginSummary | null>(null)
  const [updatingId, setUpdatingId] = useState<string | null>(null)
  const [togglingId, setTogglingId] = useState<string | null>(null)
  const [menuFor, setMenuFor] = useState<string | null>(null)

  const refresh = async (): Promise<void> => {
    try {
      const next = await api.getBootstrap()
      onData(next)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    }
  }

  const closeInstall = (): void => {
    if (installing) return
    setInstallOpen(false)
    setInstallInput('')
    setInspection(null)
    setInstallError(null)
    setInspecting(false)
  }

  const inspect = async (): Promise<void> => {
    if (!installInput.trim()) return
    setInspecting(true)
    setInstallError(null)
    setInspection(null)
    try {
      const result = await api.inspectPlugin(installInput.trim())
      setInspection(result)
    } catch (error) {
      setInstallError((error as Error).message)
    } finally {
      setInspecting(false)
    }
  }

  const install = async (): Promise<void> => {
    if (!inspection) return
    setInstalling(true)
    setInstallError(null)
    try {
      const result = await api.installPlugin(inspection.token)
      await refresh()
      onToast(`已安装 ${result.plugin.name}`, 'success')
      setInstalling(false)
      setInstallOpen(false)
      setInstallInput('')
      setInspection(null)
      setInstallError(null)
    } catch (error) {
      setInstallError((error as Error).message)
      setInstalling(false)
    }
  }

  const toggle = async (plugin: PluginSummary): Promise<void> => {
    setTogglingId(plugin.id)
    try {
      const result = await api.setPluginEnabled(plugin.id, !plugin.enabled)
      if (!result.ok) onToast(result.error ?? '切换插件状态失败', 'danger')
      else onToast(plugin.enabled ? `已停用 ${plugin.name}` : `已启用 ${plugin.name}`, 'success')
      await refresh()
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setTogglingId(null)
    }
  }

  const update = async (plugin: PluginSummary): Promise<void> => {
    setUpdatingId(plugin.id)
    setMenuFor(null)
    try {
      const result = await api.updatePlugin(plugin.id)
      if (!result.ok) onToast(result.error ?? '更新失败', 'danger')
      else if (result.toast) onToast(result.toast.message, result.toast.tone)
      else onToast('插件已更新', 'success')
      await refresh()
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setUpdatingId(null)
    }
  }

  const uninstall = async (): Promise<void> => {
    if (!uninstallTarget) return
    const target = uninstallTarget
    try {
      const result = await api.uninstallPlugin(target.id)
      if (!result.ok) onToast(result.error ?? '卸载失败', 'danger')
      else onToast(`已卸载 ${target.name}`, 'success')
      setUninstallTarget(null)
      await refresh()
    } catch (error) {
      onToast((error as Error).message, 'danger')
    }
  }

  return (
    <>
      <PageHeader
        title="插件"
        description="安装、更新与启停扩展。插件自身的设置页由插件维护，不在这里显示。"
        actions={
          <button type="button" className="settings-primary-button" onClick={() => setInstallOpen(true)}>
            <Plus size={14} strokeWidth={2} aria-hidden="true" />
            安装插件
          </button>
        }
      />
      <div className="settings-stack">
        <div className="settings-info-banner">
          <Puzzle size={15} strokeWidth={1.5} aria-hidden="true" />
          <span>
            内置插件随 Flashcast 更新；外部插件从 Git 仓库或本地目录安装。安装前请确认来源可信。
          </span>
        </div>

        <Section>
          <div className="settings-plugin-list">
            {data.plugins.map((plugin) => {
              const external = !plugin.builtin
              const statusTone = plugin.status === 'error' ? 'danger' : plugin.enabled ? 'success' : 'default'
              return (
                <div className={`settings-plugin-row${plugin.enabled ? '' : ' is-disabled'}`} key={plugin.id}>
                  <div className="settings-plugin-icon">
                    <Glyph name={plugin.iconKey ?? 'puzzle'} src={plugin.icon} size={22} />
                  </div>
                  <div className="settings-plugin-main">
                    <div className="settings-plugin-name-line">
                      <span className="settings-plugin-name">{plugin.name}</span>
                      <span className="settings-plugin-version">v{plugin.version}</span>
                      <span className="settings-plugin-tag">{plugin.builtin ? '内置' : '外部'}</span>
                      {plugin.source === 'git' ? <span className="settings-plugin-tag">Git</span> : null}
                    </div>
                    <div className="settings-plugin-description">
                      {plugin.description || '没有提供描述'}
                    </div>
                    <div className="settings-plugin-meta">
                      {plugin.builtin ? '内置' : plugin.source === 'git' ? 'Git 安装' : '本地安装'}
                      {' · '}
                      关键字 {plugin.keywords.length} 个
                      {plugin.permissions.length > 0 ? ` · ${plugin.permissions.length} 项权限` : ''}
                    </div>
                    {plugin.error ? (
                      <div className="settings-plugin-error">
                        <AlertTriangle size={12} strokeWidth={2} aria-hidden="true" />
                        {plugin.error}
                      </div>
                    ) : null}
                  </div>
                  <div className="settings-plugin-actions">
                    <StatusChip tone={statusTone}>
                      {plugin.status === 'error' ? '加载失败' : plugin.enabled ? '已启用' : '已停用'}
                    </StatusChip>
                    <Switch
                      checked={plugin.enabled}
                      onChange={() => void toggle(plugin)}
                      label={`${plugin.enabled ? '停用' : '启用'} ${plugin.name}`}
                    />
                    {external ? (
                      <button
                        type="button"
                        className="settings-ghost-button"
                        onClick={() => void update(plugin)}
                        disabled={updatingId === plugin.id}
                      >
                        {updatingId === plugin.id ? (
                          <span className="fc-spinner" aria-hidden="true" />
                        ) : (
                          <RefreshCw size={12} strokeWidth={1.75} aria-hidden="true" />
                        )}
                        更新
                      </button>
                    ) : null}
                    {external ? (
                      <div className="settings-menu-anchor">
                        <button
                          type="button"
                          className="settings-icon-button"
                          aria-label={`${plugin.name} 更多操作`}
                          aria-haspopup="menu"
                          aria-expanded={menuFor === plugin.id}
                          onClick={() => setMenuFor(menuFor === plugin.id ? null : plugin.id)}
                        >
                          <MoreHorizontal size={16} strokeWidth={1.75} aria-hidden="true" />
                        </button>
                        {menuFor === plugin.id ? (
                          <>
                            <div
                              className="settings-menu-backdrop"
                              onClick={() => setMenuFor(null)}
                              aria-hidden="true"
                            />
                            <div className="settings-menu" role="menu">
                              <button
                                type="button"
                                role="menuitem"
                                onClick={async () => {
                                  setMenuFor(null)
                                  try {
                                    await api.openPluginDirectory(plugin.id)
                                  } catch (error) {
                                    onToast((error as Error).message, 'danger')
                                  }
                                }}
                              >
                                <FolderOpen size={14} strokeWidth={1.5} aria-hidden="true" />
                                打开插件目录
                              </button>
                              <button
                                type="button"
                                role="menuitem"
                                className="is-danger"
                                onClick={() => {
                                  setMenuFor(null)
                                  setUninstallTarget(plugin)
                                }}
                              >
                                <Trash2 size={14} strokeWidth={1.5} aria-hidden="true" />
                                卸载插件
                              </button>
                            </div>
                          </>
                        ) : null}
                      </div>
                    ) : null}
                    {togglingId === plugin.id ? <InlineSpinner /> : null}
                  </div>
                </div>
              )
            })}
          </div>
        </Section>
      </div>

      <Modal
        open={installOpen}
        title="安装插件"
        description="输入 Git 仓库地址或本地插件目录。安装前会先读取插件清单。"
        onClose={closeInstall}
        width={560}
        footer={
          inspection ? (
            <>
              <button type="button" className="fc-btn" onClick={() => setInspection(null)} disabled={installing}>
                重新检查
              </button>
              <button
                type="button"
                className="fc-btn fc-btn--primary"
                onClick={() => void install()}
                disabled={installing || (inspection.alreadyInstalled && inspection.manifest.builtin)}
              >
                {installing ? <span className="fc-spinner" aria-hidden="true" /> : <Download size={13} strokeWidth={1.75} />}
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
                disabled={!installInput.trim() || inspecting}
                onClick={() => void inspect()}
              >
                {inspecting ? <span className="fc-spinner" aria-hidden="true" /> : null}
                检查插件
              </button>
            </>
          )
        }
      >
        {!inspection ? (
          <div className="settings-install-form">
            <label className="settings-field-label" htmlFor="plugin-source">
              插件来源
            </label>
            <input
              id="plugin-source"
              className="settings-input"
              value={installInput}
              onChange={(event) => setInstallInput(event.target.value)}
              placeholder="https://github.com/user/flashcast-plugin 或 /path/to/plugin"
              spellCheck={false}
              disabled={inspecting}
              onKeyDown={(event) => {
                if (event.key === 'Enter') void inspect()
              }}
            />
            <p className="settings-hint">
              本地目录需包含 <code>plugin.json</code>（或 <code>package.json</code> 中的 <code>flashcast</code> 字段）与入口文件。
            </p>
            {inspecting ? <InlineSpinner label="正在读取插件清单与文件信息…" /> : null}
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
              <div className="settings-plugin-icon settings-plugin-icon--lg">
                <Glyph
                  name={inspection.manifest.iconKey ?? 'puzzle'}
                  src={inspection.manifest.icon}
                  size={26}
                />
              </div>
              <div>
                <div className="settings-inspection-name">
                  {inspection.manifest.name}
                  <span className="settings-plugin-version">v{inspection.manifest.version}</span>
                </div>
                <div className="settings-plugin-meta">
                  {inspection.sourceType === 'git' ? 'Git 仓库' : '本地目录'} · {inspection.fileCount} 个文件 ·{' '}
                  {formatBytes(inspection.size)}
                </div>
              </div>
              <StatusChip tone={inspection.alreadyInstalled ? 'warning' : 'success'}>
                {inspection.alreadyInstalled ? `已安装 v${inspection.installedVersion}` : '可安装'}
              </StatusChip>
            </div>
            <p className="settings-inspection-description">
              {inspection.manifest.description || '插件没有提供描述。'}
            </p>
            <div className="settings-kv">
              <span>插件 ID</span>
              <code>{inspection.manifest.id}</code>
            </div>
            <div className="settings-kv">
              <span>入口</span>
              <code>{inspection.entry}</code>
            </div>
            <div className="settings-kv">
              <span>关键字</span>
              <span>{inspection.manifest.keywords.join(' · ') || '无'}</span>
            </div>
            <div className="settings-kv">
              <span>权限</span>
              <span>{inspection.permissions.length ? inspection.permissions.join(' · ') : '未声明额外权限'}</span>
            </div>
            {inspection.warning ? (
              <div className="settings-warning">
                <ShieldAlert size={14} strokeWidth={1.75} aria-hidden="true" />
                <span>{inspection.warning}</span>
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
        title="卸载插件"
        description={
          uninstallTarget
            ? `将删除 ${uninstallTarget.name} 的插件代码与设置，此操作无法撤销。`
            : undefined
        }
        onClose={() => setUninstallTarget(null)}
        danger
        width={420}
        footer={
          <>
            <button type="button" className="fc-btn" onClick={() => setUninstallTarget(null)}>
              取消
            </button>
            <button type="button" className="fc-btn fc-btn--danger" onClick={() => void uninstall()}>
              <Trash2 size={13} strokeWidth={1.75} aria-hidden="true" />
              确认卸载
            </button>
          </>
        }
      >
        <p className="settings-hint">插件保存在工作区的 extensions 目录中，设置文件位于 plugins 目录。</p>
      </Modal>
    </>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`
}
