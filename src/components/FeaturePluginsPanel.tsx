import type { PluginView } from "../types";

interface Props {
  /** 随应用提供的功能插件（标识、版本、关键词与启用状态）。 */
  plugins: PluginView[];
  busy: boolean;
  onToggle: (id: string, enabled: boolean) => void;
}

/**
 * 功能插件区段：显示随应用提供的插件与启用状态，并允许启用 / 停用。
 *
 * 停用同时停止该插件的搜索贡献与后台活动（宿主侧保证）；启用状态记录在工作区的
 * `manifest.json` 里，因此可以随配置一起提交与同步。
 */
export function FeaturePluginsPanel({ plugins, busy, onToggle }: Props) {
  return (
    <section className="settings-section" data-testid="plugin-section">
      <h2 className="settings-section-title">功能插件</h2>
      {plugins.length === 0 ? (
        <p className="settings-hint" data-testid="plugin-empty">
          尚未读取到功能插件。
        </p>
      ) : (
        <ul className="theme-list" data-testid="plugin-list">
          {plugins.map((plugin) => (
            <li
              className="theme-item"
              data-testid="plugin-item"
              data-plugin-id={plugin.id}
              data-enabled={plugin.enabled ? "true" : "false"}
              key={plugin.id}
            >
              <span className="theme-name">
                {plugin.name}
                <span className="theme-meta">
                  {plugin.id} · v{plugin.version} · 关键词：
                  {plugin.keywords.length > 0 ? plugin.keywords.join(" / ") : "无"}
                </span>
              </span>
              <span className="theme-badge" data-testid="plugin-state">
                {plugin.enabled ? "已启用" : "已停用"}
              </span>
              <button
                type="button"
                className="secondary-button"
                data-testid="plugin-toggle"
                disabled={busy}
                onClick={() => onToggle(plugin.id, !plugin.enabled)}
              >
                {plugin.enabled ? "停用" : "启用"}
              </button>
            </li>
          ))}
        </ul>
      )}
      <p className="settings-hint">
        插件实现随应用提供，启用状态记录在工作区的 manifest.json 里；停用后既不贡献
        搜索结果，也不产生后台活动。原生能力（例如写入剪贴板）只在插件声明了对应能力时
        由宿主在边界处校验后执行。
      </p>
    </section>
  );
}
