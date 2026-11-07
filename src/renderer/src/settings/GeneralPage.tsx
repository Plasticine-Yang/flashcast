import { Keyboard } from 'lucide-react'
import { useState } from 'react'
import type { BootstrapData } from '@shared/types'
import { keycapParts } from '@shared/shortcuts'
import { api } from '../bridge'
import { PageHeader, Section, Row, Select, InlineSpinner } from './components'

export function GeneralPage({
  data,
  onData,
  onToast,
  onNavigate
}: {
  data: BootstrapData
  onData: (data: BootstrapData) => void
  onToast: (message: string, tone?: 'default' | 'success' | 'warning' | 'danger') => void
  onNavigate: (page: string) => void
}): React.JSX.Element {
  const [busy, setBusy] = useState(false)

  const update = async (patch: Parameters<typeof api.updatePrefs>[0]): Promise<void> => {
    setBusy(true)
    try {
      const next = await api.updatePrefs(patch)
      onData(next)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(false)
    }
  }

  return (
    <>
      <PageHeader title="通用" description="启动行为与语言。所有设置都会保存到当前工作区。" />
      <div className="settings-stack">
        <Section title="启动">
          <Row
            label="开机自启"
            description={
              data.platform === 'linux'
                ? '登录桌面后自动启动并常驻，快捷键随时唤起。'
                : '登录系统后自动启动并常驻菜单栏 / 托盘。'
            }
          >
            <button
              type="button"
              className="fc-switch"
              role="switch"
              aria-checked={data.prefs.launchAtLogin}
              aria-label="开机自启"
              disabled={busy}
              onClick={() => void update({ launchAtLogin: !data.prefs.launchAtLogin })}
            />
          </Row>
          <Row
            label="全局快捷键"
            description={`${keycapParts(data.prefs.hotkeys.launcher, data.platform).join(' ')} 唤起启动器，可在快捷键页单独调整。`}
          >
            <button
              type="button"
              className="settings-link-button"
              onClick={() => onNavigate('shortcuts')}
            >
              <Keyboard size={14} strokeWidth={1.5} aria-hidden="true" />
              管理快捷键
            </button>
          </Row>
        </Section>

        <Section title="语言">
          <Row label="界面语言" description="当前版本仅提供简体中文。">
            <Select
              label="界面语言"
              value="zh-CN"
              onChange={() => undefined}
              disabled
              options={[{ value: 'zh-CN', label: '简体中文' }]}
            />
          </Row>
        </Section>

        {busy ? (
          <div className="settings-footnote">
            <InlineSpinner label="正在应用设置…" />
          </div>
        ) : null}
      </div>
    </>
  )
}
