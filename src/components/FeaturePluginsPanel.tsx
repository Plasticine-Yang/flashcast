import type { PluginView } from "../types";
import { Glyph } from "./Glyph";

interface Props {
  plugins: PluginView[];
  busy: boolean;
  onToggle: (id: string, enabled: boolean) => void;
}
const descriptions: Record<string, string> = {
  memo: "查找常用内容，回车粘贴正文",
  "chrome-bookmarks": "搜索书签，在关联的 Chrome 中打开",
  clipboard: "记录复制内容，随时找回并粘贴",
};
export function FeaturePluginsPanel({ plugins, busy, onToggle }: Props) {
  return <section className="settings-section plugin-panel" data-testid="plugin-section">
    <header className="panel-heading"><h2 className="settings-section-title">功能插件</h2><span>{plugins.filter(p => p.enabled).length} / {plugins.length} 已启用</span></header>
    <ul className="plugin-list" data-testid="plugin-list">
      {plugins.map(plugin => <li className="plugin-row" key={plugin.id} data-testid="plugin-item" data-plugin-id={plugin.id} data-enabled={plugin.enabled}>
        <span className="plugin-glyph"><Glyph name={plugin.id === "memo" ? "memo" : plugin.id === "clipboard" ? "clipboardEntry" : "bookmark"} /></span>
        <div className="plugin-copy"><div className="plugin-title">{plugin.name}<small>v{plugin.version}</small></div><p>{descriptions[plugin.id] ?? "贡献搜索结果与操作"}</p><div className="plugin-keywords">{plugin.keywords.map(keyword => <span key={keyword}>{keyword}</span>)}</div></div>
        <button type="button" role="switch" aria-checked={plugin.enabled} aria-label={`${plugin.enabled ? "停用" : "启用"}${plugin.name}`} className="plugin-switch" data-testid="plugin-toggle" disabled={busy} onClick={() => onToggle(plugin.id, !plugin.enabled)}><span /></button>
        <span className="sr-only" data-testid="plugin-state">{plugin.enabled ? "已启用" : "已停用"}</span>
      </li>)}
    </ul>
    {plugins.length === 0 ? <p className="settings-hint" data-testid="plugin-empty">尚未读取到功能插件。</p> : null}
    <p className="panel-footnote">停用后停止搜索与后台活动。启用状态随配置同步。</p>
  </section>;
}
