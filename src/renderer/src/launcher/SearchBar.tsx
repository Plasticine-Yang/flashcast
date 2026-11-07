import { Search, Settings, X } from 'lucide-react'
import type { PluginSummary, UpdateState } from '@shared/types'
import { PluginIcon } from '../components/ui'
import { UpdateChip } from './UpdateChip'

export function SearchBar({
  inputRef,
  value,
  placeholder,
  activePlugin,
  settingsActive,
  searchable,
  readOnly,
  update,
  settingsShortcut,
  onChange,
  onClear,
  onPluginClick,
  onOpenSettings,
  onUpdateAction,
  onCompositionStart,
  onCompositionEnd,
  activeDescendant
}: {
  inputRef: React.RefObject<HTMLInputElement | null>
  value: string
  placeholder: string
  activePlugin: PluginSummary | null
  settingsActive: boolean
  searchable: boolean
  readOnly: boolean
  update: UpdateState | null
  settingsShortcut: string
  onChange: (value: string) => void
  onClear: () => void
  onPluginClick: () => void
  onOpenSettings: () => void
  onUpdateAction: () => void
  onCompositionStart: () => void
  onCompositionEnd: () => void
  activeDescendant?: string
}): React.JSX.Element {
  const isReadonly = readOnly || !searchable || settingsActive
  return (
    <header className="fc-searchbar">
      <Search className="fc-searchbar-icon" size={20} strokeWidth={1.5} aria-hidden="true" />
      <div className="fc-search-input-wrap">
        <input
          ref={inputRef}
          className="fc-search-input"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onCompositionStart={onCompositionStart}
          onCompositionEnd={onCompositionEnd}
          readOnly={isReadonly}
          spellCheck={false}
          autoComplete="off"
          autoCorrect="off"
          autoCapitalize="off"
          aria-label={placeholder}
          aria-autocomplete="list"
          aria-controls="fc-results"
          aria-expanded="true"
          aria-activedescendant={activeDescendant}
          role="combobox"
          inputMode="search"
        />
        {!value ? (
          <span className="fc-placeholder" key={placeholder}>
            {placeholder}
          </span>
        ) : null}
      </div>
      <div className="fc-search-actions">
        <UpdateChip state={update} onAction={onUpdateAction} />
        {value ? (
          <button
            type="button"
            className="fc-clear-button"
            aria-label="清空搜索"
            title="清空搜索"
            onClick={onClear}
          >
            <X size={14} strokeWidth={1.75} aria-hidden="true" />
          </button>
        ) : null}
        {activePlugin ? (
          <button
            type="button"
            className="fc-plugin-badge"
            aria-label={settingsActive ? `返回${activePlugin.name}` : `打开${activePlugin.name}设置`}
            title={settingsActive ? '返回插件' : '插件设置'}
            onClick={onPluginClick}
          >
            <PluginIcon icon={activePlugin.icon} iconKey={activePlugin.iconKey} size={17} />
          </button>
        ) : null}
        {/* App settings live on the global page only: inside a plugin the
            top-right icon is the plugin's own settings, and a second gear next
            to it reads as the same action. ⌘, still works everywhere. */}
        {activePlugin ? null : (
          <button
            type="button"
            className="fc-settings-button"
            aria-label="打开设置"
            title={`打开设置（${settingsShortcut}）`}
            onClick={onOpenSettings}
          >
            <Settings size={16} strokeWidth={1.6} aria-hidden="true" />
          </button>
        )}
      </div>
    </header>
  )
}
