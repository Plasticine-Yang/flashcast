import { AlertTriangle, ArrowLeft, Check, ChevronDown, Minus, Plus } from 'lucide-react'
import { useEffect, useRef, useState } from 'react'
import type { GenericPluginSettingsPage, SettingsField } from '@shared/types'
import { api } from '../bridge'

type Values = Record<string, string | number | boolean | null>

export function PluginSettingsView({
  page,
  onBack,
  onSaved,
  onError
}: {
  page: GenericPluginSettingsPage
  onBack: () => void
  onSaved: (page: GenericPluginSettingsPage) => void
  onError: (message: string) => void
}): React.JSX.Element {
  const [values, setValues] = useState<Values>(() => initialValues(page))
  const [saved, setSaved] = useState(false)
  const [saving, setSaving] = useState(false)
  const [fieldError, setFieldError] = useState<string | null>(null)
  const [saveError, setSaveError] = useState<string | null>(null)
  const timers = useRef<{ save: number | null; saved: number | null }>({ save: null, saved: null })
  const pending = useRef<Record<string, unknown>>({})

  useEffect(() => {
    setValues(initialValues(page))
  }, [page.pluginId])

  useEffect(() => {
    const readonly = Object.fromEntries(
      page.fields
        .filter((field) => field.type === 'text' && field.readonly)
        .map((field) => [field.key, field.value])
    )
    if (Object.keys(readonly).length > 0) {
      setValues((current) => ({ ...current, ...readonly }))
    }
  }, [page.savedAt, page.pluginId])

  useEffect(() => {
    return () => {
      if (timers.current.save) window.clearTimeout(timers.current.save)
      if (timers.current.saved) window.clearTimeout(timers.current.saved)
      const pendingPatch = pending.current
      pending.current = {}
      if (Object.keys(pendingPatch).length > 0) {
        void api.savePluginSettings(page.pluginId, pendingPatch).catch(() => {})
      }
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [page.pluginId])

  const flush = async (patch: Record<string, unknown>): Promise<void> => {
    if (Object.keys(patch).length === 0) return
    setSaving(true)
    try {
      const result = await api.savePluginSettings(page.pluginId, patch)
      setSaving(false)
      setSaved(true)
      setSaveError(null)
      if (result.page.kind === 'settings') onSaved(result.page)
      if (timers.current.saved) window.clearTimeout(timers.current.saved)
      timers.current.saved = window.setTimeout(() => setSaved(false), 1600)
    } catch (error) {
      setSaving(false)
      const message = (error as Error).message
      setSaveError(message)
      onError(message)
    }
  }

  const scheduleSave = (patch: Record<string, unknown>): void => {
    pending.current = { ...pending.current, ...patch }
    if (timers.current.save) window.clearTimeout(timers.current.save)
    timers.current.save = window.setTimeout(() => {
      const next = pending.current
      pending.current = {}
      void flush(next)
    }, 280)
  }

  const updateField = (field: SettingsField, value: string | number | boolean | null): void => {
    setValues((current) => ({ ...current, [field.key]: value }))
    setFieldError(null)
    if (field.type === 'number') {
      const number = Number(value)
      if (!Number.isFinite(number) || number < field.min || number > field.max) {
        setFieldError(`${field.label}需要在 ${field.min} 到 ${field.max} 之间`)
        return
      }
      setFieldError(null)
    }
    if (field.type === 'select' || field.type === 'switch') {
      void flush({ [field.key]: value })
    } else {
      scheduleSave({ [field.key]: value })
    }
  }

  const flushPending = (): void => {
    if (timers.current.save) {
      window.clearTimeout(timers.current.save)
      timers.current.save = null
    }
    const next = pending.current
    pending.current = {}
    if (Object.keys(next).length > 0) void flush(next)
  }

  return (
    <div className="fc-plugin-settings">
      <div className="fc-settings-head">
        <button
          type="button"
          className="fc-back-button"
          onClick={() => {
            flushPending()
            onBack()
          }}
        >
          <ArrowLeft size={14} strokeWidth={1.75} aria-hidden="true" />
          返回
        </button>
        <h1 className="fc-settings-title">{page.title}</h1>
        <div className="fc-plugin-head-actions">
          {saving ? <span className="fc-spinner" aria-label="保存中" /> : null}
        </div>
      </div>
      <div className="fc-settings-scroll fc-scroll">
        {page.notice ? (
          <div className={`fc-notice${page.error ? ' fc-notice--warning' : ''}`}>
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
        <div className="fc-fields">
          {page.fields.map((field) => (
            <FieldControl
              key={field.key}
              field={field}
              value={values[field.key]}
              onChange={(value) => updateField(field, value)}
              onBlur={flushPending}
            />
          ))}
        </div>
        {page.fields.length === 0 ? (
          <div className="fc-empty" style={{ minHeight: 160 }}>
            <div className="fc-empty-title">这个插件没有可配置项</div>
            <div className="fc-empty-description">插件设置由插件自身维护，随工作区一起保存。</div>
          </div>
        ) : null}
      </div>
      <div className="fc-settings-footer" aria-live="polite">
        {saved ? (
          <span className="fc-saved-state">
            <Check size={12} strokeWidth={2.25} aria-hidden="true" />
            已自动保存
          </span>
        ) : null}
        {fieldError ? <span className="fc-inline-error">{fieldError}</span> : null}
      </div>
    </div>
  )
}

function FieldControl({
  field,
  value,
  onChange,
  onBlur
}: {
  field: SettingsField
  value: string | number | boolean | null | undefined
  onChange: (value: string | number | boolean | null) => void
  onBlur: () => void
}): React.JSX.Element {
  const id = `fc-field-${field.key}`

  if (field.type === 'select') {
    return (
      <label className="fc-field" htmlFor={id}>
        <span className="fc-field-label">{field.label}</span>
        <div className="fc-select-wrap">
          <select
            id={id}
            className="fc-select"
            value={typeof value === 'string' ? value : ''}
            disabled={field.disabled}
            data-settings-field
            onChange={(event) => onChange(event.target.value)}
            onBlur={onBlur}
          >
            {field.options.length === 0 ? <option value="">未检测到可用项</option> : null}
            {field.options.map((option) => (
              <option value={option.value} key={option.value}>
                {option.label}
              </option>
            ))}
          </select>
          <ChevronDown size={14} strokeWidth={1.75} aria-hidden="true" />
        </div>
        {field.description ? <span className="fc-field-description">{field.description}</span> : null}
      </label>
    )
  }

  if (field.type === 'number') {
    const number = typeof value === 'number' && Number.isFinite(value) ? value : field.value
    return (
      <div className="fc-field">
        <label className="fc-field-label" htmlFor={id}>
          {field.label}
        </label>
        <div className="fc-field-control">
          <div className="fc-number">
            <input
              id={id}
              className="fc-input tnum"
              type="number"
              value={number}
              min={field.min}
              max={field.max}
              step={field.step}
              disabled={field.disabled}
              data-settings-field
              onChange={(event) => onChange(Number(event.target.value))}
              onBlur={onBlur}
            />
            <div className="fc-stepper">
              <button
                type="button"
                aria-label={`减少${field.label}`}
                onClick={() => onChange(Math.max(field.min, number - field.step))}
              >
                <Minus size={12} strokeWidth={2} aria-hidden="true" />
              </button>
              <button
                type="button"
                aria-label={`增加${field.label}`}
                onClick={() => onChange(Math.min(field.max, number + field.step))}
              >
                <Plus size={12} strokeWidth={2} aria-hidden="true" />
              </button>
            </div>
          </div>
          {field.unit ? <span className="fc-field-unit">{field.unit}</span> : null}
        </div>
        {field.description ? <span className="fc-field-description">{field.description}</span> : null}
      </div>
    )
  }

  if (field.type === 'switch') {
    return (
      <div className="fc-field">
        <span className="fc-field-label">{field.label}</span>
        <div className="fc-field-control">
          <button
            type="button"
            className="fc-switch"
            role="switch"
            aria-checked={value === true}
            aria-label={field.label}
            data-settings-field
            onClick={() => onChange(!(value === true))}
          />
        </div>
        {field.description ? <span className="fc-field-description">{field.description}</span> : null}
      </div>
    )
  }

  return (
    <label className="fc-field" htmlFor={id}>
      <span className="fc-field-label">{field.label}</span>
      <input
        id={id}
        className={`fc-input${field.mono ? ' mono' : ''}`}
        value={typeof value === 'string' ? value : ''}
        readOnly={field.readonly}
        disabled={field.disabled}
        placeholder={field.placeholder}
        data-settings-field
        onChange={(event) => onChange(event.target.value)}
        onBlur={onBlur}
      />
      {field.description ? <span className="fc-field-description">{field.description}</span> : null}
    </label>
  )
}

function initialValues(page: GenericPluginSettingsPage): Values {
  const values: Values = {}
  for (const field of page.fields) {
    values[field.key] = field.value
  }
  return values
}
