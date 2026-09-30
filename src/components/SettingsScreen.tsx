import { useEffect, useState } from "react";
import type { Settings, ThemeState, WorkspaceStatus } from "../types";

/** 设置页里显示的一条反馈。 */
export interface SettingsMessage {
  level: "info" | "error";
  text: string;
}

interface Props {
  workspace: WorkspaceStatus | null;
  settings: Settings | null;
  theme: ThemeState | null;
  hotkey: { label: string; error: string | null; registered: boolean } | null;
  message: SettingsMessage | null;
  /** 正在执行工作区操作，按钮暂时禁用。 */
  busy: boolean;
  onBack: () => void;
  onSelectWorkspace: (path: string) => void;
  onInitWorkspace: (path: string) => void;
  onSaveHotkey: (hotkey: string) => void;
  onSelectTheme: (id: string) => void;
  onToggleTheme: (id: string, enabled: boolean) => void;
  onInstallTheme: (path: string) => void;
  onRemoveTheme: (id: string) => void;
}

/**
 * 设置页：显示当前配置工作区、关联现有仓库或初始化新目录、显示校验失败原因，
 * 编辑全局快捷键，并管理主题（内置浅色 / 深色 / 跟随系统与已安装的本地主题包）。
 *
 * 延续紧凑列表的视觉语言（`styles.css` 的设计令牌，无 CSS 框架、无动画）：
 * 高频操作（快捷键保存、工作区切换、主题切换）不做过渡动画。
 */
export function SettingsScreen({
  workspace,
  settings,
  theme,
  hotkey,
  message,
  busy,
  onBack,
  onSelectWorkspace,
  onInitWorkspace,
  onSaveHotkey,
  onSelectTheme,
  onToggleTheme,
  onInstallTheme,
  onRemoveTheme,
}: Props) {
  const [path, setPath] = useState(workspace?.path ?? "");
  const [hotkeyDraft, setHotkeyDraft] = useState(settings?.hotkey ?? "");
  const [themePackage, setThemePackage] = useState("");

  // 工作区或设置在外部被改写（宿主重载）时同步输入框。
  useEffect(() => {
    if (workspace?.path) {
      setPath(workspace.path);
    }
  }, [workspace?.path]);

  useEffect(() => {
    if (settings?.hotkey) {
      setHotkeyDraft(settings.hotkey);
    }
  }, [settings?.hotkey]);

  // Escape 返回搜索首屏。挂在 window 上而不是容器上：点击主题按钮后按钮会变成
  // 禁用状态并失去焦点，此时焦点在 body 上，容器上的 onKeyDown 不会再触发。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onBack();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onBack]);

  const linked = workspace?.path != null;

  return (
    <div className="settings" data-testid="settings-screen">
      <header className="settings-header">
        <h1 className="settings-title">设置</h1>
        <button
          type="button"
          className="ghost-button"
          data-testid="settings-back"
          onClick={onBack}
        >
          返回
        </button>
      </header>

      <div className="settings-body">
        <section className="settings-section" data-testid="workspace-section">
          <h2 className="settings-section-title">配置工作区</h2>

          <dl className="settings-facts">
            <div className="settings-fact">
              <dt>当前工作区</dt>
              <dd data-testid="workspace-path-value">
                {workspace?.path ?? "尚未关联配置工作区"}
              </dd>
            </div>
            <div className="settings-fact">
              <dt>状态</dt>
              <dd data-testid="workspace-validity">
                {linked
                  ? workspace.gitDir
                    ? "已关联 Git 仓库"
                    : "已关联（目录不是 Git 仓库）"
                  : "未关联"}
                {linked && !workspace.persisted ? "，设置仅保存在内存中" : ""}
              </dd>
            </div>
            {workspace?.settingsFile ? (
              <div className="settings-fact">
                <dt>设置文件</dt>
                <dd data-testid="workspace-settings-file">{workspace.settingsFile}</dd>
              </div>
            ) : null}
          </dl>

          {workspace?.error ? (
            <div className="banner banner-error" data-testid="workspace-error" role="alert">
              {workspace.error}
            </div>
          ) : null}

          <label className="settings-label" htmlFor="workspace-path">
            本地目录
          </label>
          <div className="settings-row">
            <input
              id="workspace-path"
              className="path-input"
              data-testid="workspace-path-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="/home/用户名/flashcast-config"
              value={path}
              onChange={(event) => setPath(event.target.value)}
            />
            <button
              type="button"
              className="primary-button"
              data-testid="workspace-select"
              disabled={busy}
              onClick={() => onSelectWorkspace(path)}
            >
              选择现有仓库
            </button>
            <button
              type="button"
              className="secondary-button"
              data-testid="workspace-init"
              disabled={busy}
              onClick={() => onInitWorkspace(path)}
            >
              初始化新目录
            </button>
          </div>
          <p className="settings-hint">
            选择现有仓库会读取其中的 settings.toml；初始化新目录要求目录为空，
            并会一并建立 Git 仓库，不会覆盖已有文件。
          </p>
        </section>

        <section className="settings-section" data-testid="hotkey-section">
          <h2 className="settings-section-title">全局快捷键</h2>
          <div className="settings-row">
            <input
              id="hotkey"
              className="path-input"
              data-testid="hotkey-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Ctrl+Alt+Space"
              value={hotkeyDraft}
              onChange={(event) => setHotkeyDraft(event.target.value)}
            />
            <button
              type="button"
              className="primary-button"
              data-testid="hotkey-save"
              disabled={busy}
              onClick={() => onSaveHotkey(hotkeyDraft)}
            >
              保存
            </button>
          </div>
          <p className="settings-hint" data-testid="hotkey-status">
            当前生效：{hotkey?.registered ? hotkey.label : "未注册"}
            {hotkey?.error ? `（${hotkey.error}）` : ""}
          </p>
          <p className="settings-hint">
            修改后立即生效，并写入工作区的 settings.toml，可以直接用编辑器维护。
          </p>
        </section>

        <section className="settings-section" data-testid="theme-section">
          <h2 className="settings-section-title">外观主题</h2>

          <dl className="settings-facts">
            <div className="settings-fact">
              <dt>当前主题</dt>
              <dd data-testid="theme-current">
                {theme ? theme.selectedName : "尚未加载"}
              </dd>
            </div>
            <div className="settings-fact">
              <dt>实际外观</dt>
              <dd data-testid="theme-appearance">
                {theme
                  ? `${theme.appearance === "dark" ? "深色" : "浅色"}${
                      theme.preference === "system" ? "（跟随系统）" : ""
                    }`
                  : "尚未加载"}
              </dd>
            </div>
          </dl>

          {theme?.error ? (
            <div className="banner banner-error" data-testid="theme-error" role="alert">
              {theme.error}
            </div>
          ) : null}

          <ul className="theme-list" data-testid="theme-list">
            {(theme?.themes ?? []).map((entry) => (
              <li
                key={entry.id}
                className="theme-item"
                data-testid="theme-item"
                data-theme-id={entry.id}
                data-selected={entry.selected ? "true" : "false"}
                data-enabled={entry.enabled ? "true" : "false"}
                data-usable={entry.usable ? "true" : "false"}
              >
                <span className="theme-name">
                  {entry.name}
                  <span className="theme-meta">
                    {entry.builtin ? "内置" : "已安装"} · v{entry.version} ·{" "}
                    {entry.appearance === "system"
                      ? "跟随系统"
                      : entry.appearance === "dark"
                        ? "深色"
                        : "浅色"}
                  </span>
                </span>
                {entry.selected ? (
                  <span className="theme-badge" data-testid="theme-selected-badge">
                    已选择
                  </span>
                ) : null}
                {!entry.usable ? (
                  <span className="theme-badge theme-badge-error" data-testid="theme-unusable">
                    不可用
                  </span>
                ) : null}
                <button
                  type="button"
                  className="primary-button"
                  data-testid="theme-select"
                  disabled={busy || entry.selected || !entry.enabled || !entry.usable}
                  onClick={() => onSelectTheme(entry.id)}
                >
                  选择
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  data-testid="theme-toggle"
                  disabled={busy}
                  onClick={() => onToggleTheme(entry.id, !entry.enabled)}
                >
                  {entry.enabled ? "停用" : "启用"}
                </button>
                {entry.builtin ? null : (
                  <button
                    type="button"
                    className="secondary-button"
                    data-testid="theme-remove"
                    disabled={busy}
                    onClick={() => onRemoveTheme(entry.id)}
                  >
                    移除
                  </button>
                )}
              </li>
            ))}
          </ul>
          <label className="settings-label" htmlFor="theme-package-path">
            本地主题包
          </label>
          <div className="settings-row">
            <input
              id="theme-package-path"
              className="path-input"
              data-testid="theme-package-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="/home/用户名/主题包目录"
              value={themePackage}
              onChange={(event) => setThemePackage(event.target.value)}
            />
            <button
              type="button"
              className="primary-button"
              data-testid="theme-install"
              disabled={busy}
              onClick={() => onInstallTheme(themePackage)}
            >
              安装主题包
            </button>
          </div>
          <p className="settings-hint">
            主题包可以是包含 theme.json 的目录，也可以直接是主题 JSON 文件；校验失败会给出
            原因并保留当前外观。主题是声明式数据（颜色、字体、间距、圆角、阴影与状态语义），
            不执行任何代码；间距与字号由宿主固定，因此切换主题不会移动控件。
          </p>
        </section>

        {message ? (
          <div
            className={`banner banner-${message.level}`}
            data-testid="settings-message"
            role={message.level === "error" ? "alert" : "status"}
          >
            {message.text}
          </div>
        ) : null}
      </div>
    </div>
  );
}
