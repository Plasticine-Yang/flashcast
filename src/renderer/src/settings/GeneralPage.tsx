import { Keyboard, Monitor, Moon, Sun } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { BootstrapData, ThemeSetting } from '@shared/types'
import {
  GLASS_INTENSITY_DEFAULT,
  GLASS_INTENSITY_MAX,
  GLASS_INTENSITY_MIN,
  GLASS_INTENSITY_STEP,
  glassIntensityLabel,
  normalizeGlassIntensity
} from '@shared/glass'
import { keycapParts } from '@shared/shortcuts'
import { api } from '../bridge'
import { applyGlass } from '../glass'
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
  const [themeBusy, setThemeBusy] = useState<ThemeSetting | null>(null)
  const [glass, setGlass] = useState(data.prefs.glassIntensity)
  const glassRef = useRef(data.prefs.glassIntensity)
  const persistedGlassRef = useRef(data.prefs.glassIntensity)
  const glassTimerRef = useRef<number | null>(null)

  const update = async (patch: Parameters<typeof api.updatePrefs>[0]): Promise<void> => {
    setBusy(true)
    try {
      const next = await api.updatePrefs(patch)
      onData(next)
    } catch (error) {
      onToast((error as Error).message, 'danger')
    } finally {
      setBusy(false)
      setThemeBusy(null)
    }
  }

  useEffect(() => {
    glassRef.current = data.prefs.glassIntensity
    persistedGlassRef.current = data.prefs.glassIntensity
    setGlass(data.prefs.glassIntensity)
  }, [data.prefs.glassIntensity])

  /* The slider applies to the document immediately and is persisted after a
     short pause, so dragging stays smooth but leaving the page never loses it. */
  const persistGlass = (value: number): void => {
    if (value === persistedGlassRef.current) return
    persistedGlassRef.current = value
    void api
      .updatePrefs({ glassIntensity: value })
      .then(onData)
      .catch((error: Error) => onToast(error.message, 'danger'))
  }

  const scheduleGlass = (value: number): void => {
    const next = normalizeGlassIntensity(value)
    glassRef.current = next
    setGlass(next)
    applyGlass(next, data.theme)
    if (glassTimerRef.current !== null) window.clearTimeout(glassTimerRef.current)
    glassTimerRef.current = window.setTimeout(() => {
      glassTimerRef.current = null
      persistGlass(next)
    }, 280)
  }

  const flushGlass = (): void => {
    if (glassTimerRef.current !== null) {
      window.clearTimeout(glassTimerRef.current)
      glassTimerRef.current = null
    }
    persistGlass(glassRef.current)
  }

  useEffect(
    () => () => {
      if (glassTimerRef.current !== null) window.clearTimeout(glassTimerRef.current)
      if (glassRef.current !== persistedGlassRef.current) {
        void api.updatePrefs({ glassIntensity: glassRef.current }).catch(() => undefined)
      }
    },
    []
  )

  const themes: Array<{ value: ThemeSetting; label: string; icon: React.JSX.Element }> = [
    { value: 'system', label: '跟随系统', icon: <Monitor size={15} strokeWidth={1.5} /> },
    { value: 'dark', label: '深色', icon: <Moon size={15} strokeWidth={1.5} /> },
    { value: 'light', label: '浅色', icon: <Sun size={15} strokeWidth={1.5} /> }
  ]

  const glassLabel = glassIntensityLabel(glass)

  return (
    <>
      <PageHeader title="通用" description="外观、启动行为与语言。所有设置都会保存到当前工作区。" />
      <div className="settings-stack">
        <Section title="外观" description="启动器与设置窗口使用同一套主题令牌。">
          <div className="settings-theme-picker" role="radiogroup" aria-label="主题">
            {themes.map((theme) => (
              <button
                key={theme.value}
                type="button"
                role="radio"
                aria-checked={data.prefs.theme === theme.value}
                className={`settings-theme-option${data.prefs.theme === theme.value ? ' is-active' : ''}`}
                onClick={() => {
                  setThemeBusy(theme.value)
                  void update({ theme: theme.value })
                }}
                disabled={themeBusy !== null}
              >
                <span className={`settings-theme-preview settings-theme-preview--${theme.value}`}>
                  <span className="settings-theme-preview-bar" />
                  <span className="settings-theme-preview-row" />
                  <span className="settings-theme-preview-row short" />
                </span>
                <span className="settings-theme-label">
                  {theme.icon}
                  {theme.label}
                  {themeBusy === theme.value ? <span className="fc-spinner" /> : null}
                </span>
              </button>
            ))}
          </div>
          <p className="settings-hint">跟随系统时会响应操作系统的深色模式变化。</p>

          <div className="settings-glass">
            <div className="settings-glass-head">
              <div className="settings-row-copy">
                <label className="settings-row-label" htmlFor="glass-intensity">
                  毛玻璃强度
                </label>
                <div className="settings-row-description">
                  数值越高，启动器面板越不透明、背景越模糊。当前为{glassLabel}。
                </div>
              </div>
              <div className="settings-glass-readout" aria-hidden="true">
                <span className="settings-glass-value">{glass}</span>
                <span className="settings-glass-unit">%</span>
              </div>
            </div>

            <div className="settings-glass-body">
              <div className="settings-glass-slider-wrap">
                <div className="settings-glass-slider-row">
                  <span className="settings-glass-endcap" aria-hidden="true">
                    通透
                  </span>
                  <input
                    id="glass-intensity"
                    className="settings-range"
                    type="range"
                    min={GLASS_INTENSITY_MIN}
                    max={GLASS_INTENSITY_MAX}
                    step={GLASS_INTENSITY_STEP}
                    value={glass}
                    aria-label="毛玻璃强度"
                    aria-valuetext={`${glass}%（${glassLabel}）`}
                    style={{ '--range-progress': `${glass}%` } as React.CSSProperties}
                    onChange={(event) => scheduleGlass(Number(event.target.value))}
                    onPointerUp={flushGlass}
                    onKeyUp={flushGlass}
                    onBlur={flushGlass}
                  />
                  <span className="settings-glass-endcap" aria-hidden="true">
                    磨砂
                  </span>
                </div>
                <div className="settings-glass-foot">
                  <span className="settings-hint">
                    面板会吸收桌面壁纸的颜色；强度越高，透出的内容越少。
                  </span>
                  <button
                    type="button"
                    className="settings-link-button"
                    onClick={() => {
                      scheduleGlass(GLASS_INTENSITY_DEFAULT)
                      glassRef.current = GLASS_INTENSITY_DEFAULT
                      persistGlass(GLASS_INTENSITY_DEFAULT)
                    }}
                  >
                    恢复默认
                  </button>
                </div>
              </div>

              <div className="settings-glass-preview" aria-hidden="true">
                <div className="settings-glass-preview-scene" />
                <div className="settings-glass-preview-panel">
                  <span className="settings-glass-preview-bar" />
                  <span className="settings-glass-preview-line" />
                  <span className="settings-glass-preview-line short" />
                </div>
              </div>
            </div>
          </div>
        </Section>

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
