import { AlertTriangle, Check, Code2, RefreshCw, Save } from 'lucide-react'
import { useEffect, useState } from 'react'
import type {
  PluginSettingsPage,
  VscodeLikeWorkspaceOptions,
  VscodeLikeWorkspaceSettingsPage
} from '@shared/types'
import { api } from '../bridge'

interface VariantDraft {
  enabled: boolean
  trigger: string
}

function draftsFromPage(page: VscodeLikeWorkspaceSettingsPage): Record<string, VariantDraft> {
  return Object.fromEntries(
    page.variants.map((variant) => [
      variant.id,
      { enabled: variant.enabled, trigger: variant.trigger }
    ])
  )
}

export function VscodeLikeWorkspaceSettingsView({
  page,
  onBack,
  onSaved,
  onError
}: {
  page: VscodeLikeWorkspaceSettingsPage
  onBack: () => void
  onSaved: (page: PluginSettingsPage) => void
  onError: (message: string) => void
}): React.JSX.Element {
  const [drafts, setDrafts] = useState<Record<string, VariantDraft>>(() => draftsFromPage(page))
  const [options, setOptions] = useState<VscodeLikeWorkspaceOptions>(page.options)
  const [errors, setErrors] = useState<Record<string, string>>(page.triggerErrors)
  const [saving, setSaving] = useState(false)
  const [rescanning, setRescanning] = useState(false)
  const [saved, setSaved] = useState(false)
  const [saveError, setSaveError] = useState<string | null>(null)

  useEffect(() => {
    setDrafts(draftsFromPage(page))
    setOptions(page.options)
    setErrors(page.triggerErrors)
  }, [page.savedAt, page.pluginId, page.variants, page.options, page.triggerErrors])

  useEffect(() => {
    if (!saved) return
    const timer = window.setTimeout(() => setSaved(false), 1600)
    return () => window.clearTimeout(timer)
  }, [saved])

  const updateTrigger = (id: string, trigger: string): void => {
    setSaved(false)
    setSaveError(null)
    setErrors((current) => {
      if (!current[id]) return current
      const next = { ...current }
      delete next[id]
      return next
    })
    setDrafts((current) => ({
      ...current,
      [id]: { ...(current[id] ?? { enabled: true, trigger: '' }), trigger }
    }))
  }

  const updateEnabled = (id: string, enabled: boolean): void => {
    setSaved(false)
    setSaveError(null)
    setDrafts((current) => ({
      ...current,
      [id]: { ...(current[id] ?? { enabled: true, trigger: '' }), enabled }
    }))
  }

  const save = async (): Promise<void> => {
    if (saving) return
    setSaving(true)
    setSaveError(null)
    try {
      const payload: Record<string, unknown> = {
        variants: Object.fromEntries(
          page.variants.map((variant) => [
            variant.id,
            {
              enabled: drafts[variant.id]?.enabled ?? true,
              trigger: drafts[variant.id]?.trigger ?? ''
            }
          ])
        ),
        ...options
      }
      const result = await api.savePluginSettings(page.pluginId, payload)
      const next = result.page
      if (next.kind !== 'vscode-like-workspace-settings') {
        onSaved(next)
        setSaving(false)
        return
      }
      setErrors(next.triggerErrors)
      if (result.savedAt === 0) {
        setSaving(false)
        return
      }
      setSaved(true)
      onSaved(next)
    } catch (error) {
      const message = (error as Error).message
      setSaveError(message)
      onError(message)
    } finally {
      setSaving(false)
    }
  }

  const rescan = async (): Promise<void> => {
    if (rescanning) return
    setRescanning(true)
    setSaveError(null)
    try {
      const next = await api.getPluginSettings(page.pluginId, { refresh: true })
      if (next.kind === 'vscode-like-workspace-settings') {
        onSaved(next)
      }
    } catch (error) {
      const message = (error as Error).message
      setSaveError(message)
      onError(message)
    } finally {
      setRescanning(false)
    }
  }

  const hasVariants = page.variants.length > 0

  return (
    <div className="fc-plugin-settings fc-workspace-settings">
      <div className="fc-settings-head">
        <button type="button" className="fc-back-button" onClick={onBack}>
          返回
        </button>
        <h1 className="fc-settings-title">{page.title}</h1>
        <div className="fc-plugin-head-actions">
          <button
            type="button"
            className="fc-btn fc-btn--ghost"
            onClick={() => void rescan()}
            disabled={rescanning}
          >
            <RefreshCw
              size={13}
              strokeWidth={1.75}
              className={rescanning ? 'fc-spin' : undefined}
              aria-hidden="true"
            />
            重新扫描
          </button>
          <button
            type="button"
            className="fc-btn fc-btn--primary"
            onClick={() => void save()}
            disabled={saving || !hasVariants}
          >
            <Save size={13} strokeWidth={1.75} aria-hidden="true" />
            {saving ? '保存中…' : '保存'}
          </button>
        </div>
      </div>

      <div className="fc-settings-scroll fc-scroll">
        {page.notice ? (
          <div className="fc-notice">
            <AlertTriangle size={14} strokeWidth={1.75} aria-hidden="true" />
            <span>{page.notice}</span>
          </div>
        ) : null}
        {saveError ? (
          <div className="fc-inline-error" role="alert">
            <AlertTriangle size={13} strokeWidth={1.75} aria-hidden="true" />
            {saveError}
          </div>
        ) : null}

        {hasVariants ? (
          <>
            <div className="fc-settings-section-title">检测到的编辑器</div>
            <div className="fc-editor-settings-list">
              {page.variants.map((variant) => {
                const draft = drafts[variant.id] ?? { enabled: variant.enabled, trigger: variant.trigger }
                const error = errors[variant.id]
                return (
                  <section className="fc-editor-setting" key={variant.id}>
                    <div className="fc-editor-setting-head">
                      <span className="fc-editor-setting-icon">
                        <Code2 size={18} strokeWidth={1.5} aria-hidden="true" />
                      </span>
                      <div className="fc-editor-setting-title">
                        <span className="fc-editor-setting-name">{variant.name}</span>
                        <span className="fc-editor-setting-version">
                          {variant.version ? `v${variant.version}` : '版本未知'}
                        </span>
                      </div>
                      <button
                        type="button"
                        className="fc-switch"
                        role="switch"
                        aria-checked={draft.enabled}
                        aria-label={`启用 ${variant.name}`}
                        onClick={() => updateEnabled(variant.id, !draft.enabled)}
                      />
                    </div>
                    <div className="fc-editor-setting-path" title={variant.dataDir}>
                      {variant.dataDir}
                    </div>
                    <label className="fc-field fc-editor-trigger">
                      <span className="fc-field-label">触发词</span>
                      <div className="fc-editor-trigger-row">
                        <input
                          className="fc-input"
                          value={draft.trigger}
                          placeholder="如 trae，留空表示不设置"
                          spellCheck={false}
                          autoComplete="off"
                          disabled={!draft.enabled}
                          aria-invalid={Boolean(error)}
                          onChange={(event) => updateTrigger(variant.id, event.target.value)}
                          onKeyDown={(event) => {
                            if (event.key === 'Enter') {
                              event.preventDefault()
                              event.currentTarget.blur()
                              void save()
                            }
                          }}
                        />
                      </div>
                      {error ? (
                        <span className="fc-inline-error" role="alert">
                          <AlertTriangle size={12} strokeWidth={1.75} aria-hidden="true" />
                          {error}
                        </span>
                      ) : draft.trigger ? (
                        <span className="fc-field-description">
                          输入 <kbd>{draft.trigger}</kbd> 后回车进入；也可以搜索「{draft.trigger}」选择这个入口
                        </span>
                      ) : (
                        <span className="fc-field-description">
                          未设置触发词，只能从插件入口进入
                        </span>
                      )}
                    </label>
                  </section>
                )
              })}
            </div>
          </>
        ) : (
          <div className="fc-empty" style={{ minHeight: 200 }}>
            <div className="fc-empty-icon">
              <Code2 size={26} strokeWidth={1.5} aria-hidden="true" />
            </div>
            <div className="fc-empty-title">没有检测到 VS Code 系编辑器</div>
            <div className="fc-empty-description">
              支持 VS Code、Trae、Cursor、Windsurf、VSCodium 等基于 VS Code 的产品。安装后点击「重新扫描」。
            </div>
          </div>
        )}

        <div className="fc-settings-section-title">通用</div>
        <div className="fc-fields">
          <div className="fc-field">
            <span className="fc-field-label">包含远程工作区</span>
            <button
              type="button"
              className="fc-switch"
              role="switch"
              aria-checked={options.includeRemote}
              aria-label="包含远程工作区"
              onClick={() => {
                setSaved(false)
                setOptions((current) => ({ ...current, includeRemote: !current.includeRemote }))
              }}
            />
            <span className="fc-field-description">SSH、WSL、Dev Container 打开的工作区也会出现在列表里</span>
          </div>
          <div className="fc-field">
            <span className="fc-field-label">隐藏不存在的路径</span>
            <button
              type="button"
              className="fc-switch"
              role="switch"
              aria-checked={options.hideMissing}
              aria-label="隐藏不存在的路径"
              onClick={() => {
                setSaved(false)
                setOptions((current) => ({ ...current, hideMissing: !current.hideMissing }))
              }}
            />
            <span className="fc-field-description">工作区目录已被删除时不显示</span>
          </div>
          <label className="fc-field">
            <span className="fc-field-label">最大工作区数量</span>
            <input
              className="fc-input fc-workspace-number"
              type="number"
              min={100}
              max={5000}
              step={100}
              value={options.maxItems}
              onChange={(event) => {
                setSaved(false)
                const value = Number(event.target.value)
                if (Number.isFinite(value)) {
                  setOptions((current) => ({ ...current, maxItems: value }))
                }
              }}
            />
            <span className="fc-field-description">超过后按最近使用排序截断，默认 1000</span>
          </label>
        </div>
      </div>

      <div className="fc-settings-footer" aria-live="polite">
        {saved ? (
          <span className="fc-saved-state">
            <Check size={12} strokeWidth={2.25} aria-hidden="true" />
            已保存
          </span>
        ) : null}
      </div>
    </div>
  )
}
