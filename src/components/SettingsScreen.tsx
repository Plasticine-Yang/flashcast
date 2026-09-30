import { useEffect, useState } from "react";
import type { Settings, WorkspaceChanges, WorkspaceStatus } from "../types";
import { ChangesPanel } from "./ChangesPanel";

/** 设置页里显示的一条反馈。 */
export interface SettingsMessage {
  level: "info" | "error";
  text: string;
}

interface Props {
  workspace: WorkspaceStatus | null;
  settings: Settings | null;
  hotkey: { label: string; error: string | null; registered: boolean } | null;
  message: SettingsMessage | null;
  /** 正在执行工作区操作，按钮暂时禁用。 */
  busy: boolean;
  /** 当前工作区的 Git 变更；`null` 表示尚未读取。 */
  changes: WorkspaceChanges | null;
  commitMessage: string;
  selectedPaths: string[];
  diffPath: string | null;
  onBack: () => void;
  onSelectWorkspace: (path: string) => void;
  onInitWorkspace: (path: string) => void;
  onSaveHotkey: (hotkey: string) => void;
  onTogglePath: (path: string) => void;
  onToggleAllPaths: () => void;
  onSelectDiff: (path: string) => void;
  onCommitMessageChange: (value: string) => void;
  onCommit: () => void;
  onRefreshChanges: () => void;
}

/**
 * 设置页：显示当前配置工作区、关联现有仓库或初始化新目录、显示校验失败原因，
 * 并编辑全局快捷键。
 *
 * 延续紧凑列表的视觉语言（`styles.css` 的设计令牌，无 CSS 框架、无动画）：
 * 高频操作（快捷键保存、工作区切换）不做过渡动画。
 */
export function SettingsScreen({
  workspace,
  settings,
  hotkey,
  message,
  busy,
  changes,
  commitMessage,
  selectedPaths,
  diffPath,
  onBack,
  onSelectWorkspace,
  onInitWorkspace,
  onSaveHotkey,
  onTogglePath,
  onToggleAllPaths,
  onSelectDiff,
  onCommitMessageChange,
  onCommit,
  onRefreshChanges,
}: Props) {
  const [path, setPath] = useState(workspace?.path ?? "");
  const [hotkeyDraft, setHotkeyDraft] = useState(settings?.hotkey ?? "");

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

  const linked = workspace?.path != null;

  return (
    <div
      className="settings"
      data-testid="settings-screen"
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          onBack();
        }
      }}
    >
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

        <ChangesPanel
          changes={changes}
          message={commitMessage}
          selected={selectedPaths}
          diffPath={diffPath}
          busy={busy}
          onToggle={onTogglePath}
          onToggleAll={onToggleAllPaths}
          onSelectDiff={onSelectDiff}
          onMessageChange={onCommitMessageChange}
          onCommit={onCommit}
          onRefresh={onRefreshChanges}
        />

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
