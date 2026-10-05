import type { ThemeAppearance, ThemeState } from "../types";

interface Props {
  theme: ThemeState;
  busy: boolean;
  materialNotice: string | null;
  onChange: (appearance: ThemeAppearance, style: string, reduce: boolean) => void;
}

export function AppearanceControls({ theme, busy, materialNotice, onChange }: Props) {
  const legacy = theme.themes.find(t => t.selected)?.legacy ?? false;
  return <div className="appearance-controls">
    <div className="appearance-setting">
      <span className="setting-label">深浅模式</span>
      <div className="appearance-segment" role="group" aria-label="深浅模式">
        {([['system', '跟随系统'], ['light', '浅色'], ['dark', '深色']] as const).map(([value, label]) =>
          <button key={value} type="button" data-appearance={value} aria-pressed={theme.preference === value}
            disabled={busy || legacy} onClick={() => onChange(value, theme.style, theme.reduceTransparency)}>{label}</button>)}
      </div>
    </div>
    {theme.styles.length > 1 ? <div className="appearance-setting">
      <span className="setting-label">表面风格</span>
      <div className="appearance-segment" role="group" aria-label="表面风格">
        {theme.styles.map(style => <button key={style.id} type="button" aria-pressed={theme.style === style.id}
          disabled={busy} onClick={() => onChange(theme.preference, style.id, theme.reduceTransparency)}>{style.name}</button>)}
      </div>
    </div> : null}
    <label className="appearance-setting transparency-setting">
      <span><span className="setting-label">减少透明度</span><small>使用实底，提高阅读稳定性</small></span>
      <input type="checkbox" checked={theme.reduceTransparency} disabled={busy}
        onChange={e => onChange(theme.preference, theme.style, e.target.checked)} />
    </label>
    {legacy ? <p className="settings-hint">旧版主题使用自身的深浅设置。更新为新版主题后，可独立切换模式。</p> : null}
    {materialNotice ? <p className="settings-hint" role="status">{materialNotice}</p> : null}
  </div>;
}
