import { ExternalLink, Power, RefreshCw, ScrollText } from 'lucide-react'
import { useState } from 'react'
import type { BootstrapData } from '@shared/types'
import { api } from '../bridge'
import { Modal, PageHeader, PlatformName, Section, Logo, StatusChip } from './components'

const LICENSES = [
  { name: 'Electron', license: 'MIT', note: '跨平台桌面运行时' },
  { name: 'React', license: 'MIT', note: '界面渲染' },
  { name: 'Lucide', license: 'ISC', note: '线性图标' },
  { name: 'Inter', license: 'OFL-1.1', note: '界面字体' },
  { name: 'JetBrains Mono', license: 'OFL-1.1', note: '等宽字体' },
  { name: 'electron-vite / Vite', license: 'MIT', note: '构建工具链' }
]

export function AboutPage({
  data,
  onToast
}: {
  data: BootstrapData
  onToast: (message: string, tone?: 'default' | 'success' | 'warning' | 'danger') => void
}): React.JSX.Element {
  const [checking, setChecking] = useState(false)
  const [updateMessage, setUpdateMessage] = useState<string | null>(null)
  const [licensesOpen, setLicensesOpen] = useState(false)
  const [quitBusy, setQuitBusy] = useState(false)

  const check = async (): Promise<void> => {
    setChecking(true)
    try {
      const result = await api.checkUpdates()
      setUpdateMessage(result.message)
      onToast(result.message, 'default')
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setChecking(false)
    }
  }

  return (
    <>
      <PageHeader title="关于" description="版本信息、更新检查与开源许可。" />
      <div className="settings-stack">
        <div className="settings-about-hero">
          <div className="settings-about-mark">
            <Logo size={56} />
          </div>
          <div className="settings-about-copy">
            <div className="settings-about-name">
              Flashcast
              <StatusChip tone="accent">v{data.version}</StatusChip>
            </div>
            <p className="settings-about-tagline">
              键盘优先、插件驱动、动画流畅的跨平台启动器。像一道闪电一样打开你的工作流。
            </p>
            <div className="settings-about-tags">
              <span>{PlatformName({ platform: data.platform })}</span>
              <span>{data.plugins.filter((plugin) => plugin.builtin).length} 个内置插件</span>
              <span>本地优先</span>
            </div>
          </div>
        </div>

        <Section title="应用信息">
          <div className="settings-kv settings-kv--card">
            <span>版本</span>
            <code>v{data.version}</code>
          </div>
          <div className="settings-kv settings-kv--card">
            <span>运行平台</span>
            <span>{PlatformName({ platform: data.platform })}</span>
          </div>
          <div className="settings-kv settings-kv--card">
            <span>数据目录</span>
            <code>{data.dataDir}</code>
          </div>
          <div className="settings-kv settings-kv--card">
            <span>构建类型</span>
            <span>{data.isDev ? '开发构建' : '稳定构建'}</span>
          </div>
        </Section>

        <Section title="更新">
          <div className="settings-update-row">
            <div>
              <div className="settings-row-label">检查更新</div>
              <div className="settings-row-description">
                当前版本 v{data.version}
                {updateMessage ? ` · ${updateMessage}` : ''}
              </div>
            </div>
            <button type="button" className="fc-btn" onClick={() => void check()} disabled={checking}>
              {checking ? <span className="fc-spinner" aria-hidden="true" /> : <RefreshCw size={13} strokeWidth={1.75} aria-hidden="true" />}
              检查更新
            </button>
          </div>
        </Section>

        <Section title="开源许可">
          <div className="settings-update-row">
            <div>
              <div className="settings-row-label">第三方组件</div>
              <div className="settings-row-description">
                Flashcast 使用了 Electron、React、Lucide 与开源字体等组件。
              </div>
            </div>
            <button type="button" className="fc-btn" onClick={() => setLicensesOpen(true)}>
              <ScrollText size={13} strokeWidth={1.75} aria-hidden="true" />
              查看许可
            </button>
          </div>
        </Section>

        <div className="settings-about-footer">
          <button
            type="button"
            className="fc-btn fc-btn--danger"
            disabled={quitBusy}
            onClick={() => {
              setQuitBusy(true)
              api.quit()
            }}
          >
            <Power size={13} strokeWidth={1.75} aria-hidden="true" />
            退出 Flashcast
          </button>
          <span className="settings-hint">
            退出后将停止监听全局快捷键与剪贴板。可以随时重新启动。
          </span>
        </div>
      </div>

      <Modal
        open={licensesOpen}
        title="开源许可"
        description="以下组件遵循各自的开源许可证。"
        onClose={() => setLicensesOpen(false)}
        width={540}
        footer={
          <button type="button" className="fc-btn" onClick={() => setLicensesOpen(false)}>
            关闭
          </button>
        }
      >
        <div className="settings-license-list">
          {LICENSES.map((license) => (
            <div className="settings-license-row" key={license.name}>
              <div>
                <div className="settings-license-name">{license.name}</div>
                <div className="settings-hint">{license.note}</div>
              </div>
              <span className="settings-chip">{license.license}</span>
            </div>
          ))}
        </div>
        <p className="settings-hint">
          完整许可证文本随应用分发，可在项目仓库的 node_modules 许可证文件中查看。
        </p>
        <button type="button" className="settings-link-button" onClick={() => setLicensesOpen(false)}>
          <ExternalLink size={12} strokeWidth={1.5} aria-hidden="true" />
          返回设置
        </button>
      </Modal>
    </>
  )
}
