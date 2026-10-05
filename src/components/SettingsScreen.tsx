import { AppearanceControls } from "./AppearanceControls";
import { Glyph } from "./Glyph";
import { HotkeyConflictPanel } from "./HotkeyConflictPanel";
import { useEffect, useState } from "react";
import type {
  Capabilities,
  ChromeState,
  ClonePhase,
  CloneProgress,
  Memo,
  MemoProblem,
  PluginView,
  Settings,
  HotkeyStatus,
  HotkeyConflictReport,
  Support,
  SyncPhase,
  SyncProgress,
  SyncStatus,
  ThemeState,
  WorkspaceChanges,
  WorkspaceStatus,
  ClipboardStateView,
} from "../types";
import { ChangesPanel } from "./ChangesPanel";
import { ChromePanel } from "./ChromePanel";
import { ClipboardPanel } from "./ClipboardPanel";
import { FeaturePluginsPanel } from "./FeaturePluginsPanel";
import { MemoPanel } from "./MemoPanel";

/** 克隆阶段的中文说明。 */
const PHASE_LABEL: Record<ClonePhase, string> = {
  idle: "尚未开始",
  connecting: "正在连接远端",
  receiving: "正在接收数据",
  resolving: "正在解析数据",
  checkingOut: "正在检出文件",
  done: "克隆完成",
  failed: "克隆失败",
  cancelled: "已取消",
};

/** 克隆进度的中文描述（数值全部来自 git2 的真实回调）。 */
export function describeCloneProgress(progress: CloneProgress): string {
  const parts = [PHASE_LABEL[progress.phase]];
  if (progress.phase === "checkingOut" && progress.checkoutTotal > 0) {
    parts.push(`已检出 ${progress.checkoutCompleted}/${progress.checkoutTotal} 个文件`);
    if (progress.checkoutPath) {
      parts.push(`当前：${progress.checkoutPath}`);
    }
  } else if (progress.totalObjects > 0) {
    parts.push(`已接收 ${progress.indexedObjects}/${progress.totalObjects} 个对象`);
  }
  if (progress.message && (progress.phase === "failed" || progress.phase === "cancelled")) {
    parts.push(progress.message);
  }
  return parts.join("，");
}

/** 设置页里显示的一条反馈。 */
export interface SettingsMessage {
  level: "info" | "error";
  text: string;
}

/** 同步阶段的中文说明。 */
const SYNC_PHASE_LABEL: Record<SyncPhase, string> = {
  idle: "尚未开始",
  fetching: "拉取中",
  pushing: "推送中",
  done: "已完成",
  failed: "失败",
  cancelled: "已取消",
};

/** 同步进度的中文描述（数值来自 git2 的真实回调）。 */
export function describeSyncProgress(progress: SyncProgress): string {
  const parts = [SYNC_PHASE_LABEL[progress.phase]];
  if (progress.totalObjects > 0) {
    parts.push(`已接收 ${progress.receivedObjects}/${progress.totalObjects} 个对象`);
  } else if (progress.updates > 0) {
    parts.push(`已处理 ${progress.updates} 次传输回调`);
  }
  if (progress.message && !progress.phase.startsWith("fetch")) {
    parts.push(progress.message);
  }
  return parts.join("，");
}

/** 未提交改动的中文描述。 */
function describeDirty(sync: SyncStatus): string {
  const parts: string[] = [];
  if (sync.staged) parts.push("已暂存改动");
  if (sync.unstaged) parts.push("未暂存改动");
  if (sync.untracked) parts.push("未跟踪文件");
  if (sync.conflicted) parts.push("未解决冲突");
  return parts.length > 0 ? parts.join(" + ") : "工作区干净";
}

/** 操作系统的中文名（`Capabilities.os` 是平台层给出的短名）。 */
const OS_LABEL: Record<string, string> = {
  linux: "Linux",
  windows: "Windows",
  macos: "macOS",
  unknown: "未知系统",
};

/** Linux 会话类型的中文名；其它平台为「不适用」。 */
const SESSION_LABEL: Record<string, string> = {
  x11: "X11",
  wayland: "Wayland",
  unknown: "未知会话类型",
  headless: "无桌面会话",
  "not-applicable": "不适用",
};

/**
 * 能力状态的中文文案。
 *
 * 与平台层的 `Support::label_zh` 一致：`unknown` 表示**本环境无法判定**（未覆盖），
 * 不等于「支持」，也不等于「已判定不支持」。设置页必须把这两种情况分开写。
 */
export function describeSupport(support: Support): string {
  switch (support.status) {
    case "supported":
      return "支持";
    case "unsupported":
      return `不支持（${support.reason}）`;
    case "unknown":
      return `未覆盖（${support.reason}）`;
  }
}

/** 能力列表里的一行。`id` 用于稳定的 `data-testid`，不随状态变化。 */
function CapabilityRow({
  id,
  label,
  support,
}: {
  id: string;
  label: string;
  support: Support;
}) {
  return (
    <div className="settings-fact">
      <dt>{label}</dt>
      <dd data-testid={`capability-${id}`} data-support-status={support.status}>
        {describeSupport(support)}
      </dd>
    </div>
  );
}

interface Props {
  workspace: WorkspaceStatus | null;
  settings: Settings | null;
  theme: ThemeState | null;
  materialNotice: string | null;
  onAppearanceChange: (appearance: import("../types").ThemeAppearance, style: string, reduce: boolean) => void;
  hotkey: HotkeyStatus | null;
  /** 运行环境与能力状态（来自平台层的真实探测）。 */
  capabilities: Capabilities | null;
  message: SettingsMessage | null;
  /** 正在执行工作区操作，按钮暂时禁用。 */
  busy: boolean;
  /** 最近一次克隆的进度快照（另一线程轮询宿主）。 */
  cloneProgress: CloneProgress | null;
  /** 当前工作区的 Git 变更；`null` 表示尚未读取。 */
  changes: WorkspaceChanges | null;
  /** 当前工作区的同步状态；`null` 表示尚未读取。 */
  sync: SyncStatus | null;
  /** 最近一次同步的进度快照（另一线程轮询宿主）。 */
  syncProgress: SyncProgress | null;
  commitMessage: string;
  selectedPaths: string[];
  diffPath: string | null;
  /** 随应用提供的功能插件（备忘录等）与启用状态。 */
  plugins: PluginView[];
  /** 当前工作区里的备忘录。 */
  memos: Memo[];
  /** 无法读取的备忘录文件与中文原因。 */
  memoProblems: MemoProblem[];
  /** 备忘录插件是否启用（决定能否创建 / 修改）。 */
  memoEnabled: boolean;
  /** 当前 Chrome 状态：发现结果、profile 关联与书签索引。 */
  chrome: ChromeState | null;
  /** 剪贴板历史状态：启用、暂停、存储、容量与条目列表。 */
  clipboard: ClipboardStateView | null;
  onBack: () => void;
  onSelectWorkspace: (path: string) => void;
  onInitWorkspace: (path: string) => void;
  onSaveHotkey: (hotkey: string) => void;
  onCloneWorkspace: (
    url: string,
    target: string,
    token: { username: string; token: string } | null,
  ) => void;
  onCancelClone: () => void;
  onSelectTheme: (id: string) => void;
  onToggleTheme: (id: string, enabled: boolean) => void;
  onInstallTheme: (path: string) => void;
  onRemoveTheme: (id: string) => void;
  onTogglePath: (path: string) => void;
  onToggleAllPaths: () => void;
  onSelectDiff: (path: string) => void;
  onCommitMessageChange: (value: string) => void;
  onCommit: () => void;
  onRefreshChanges: () => void;
  onPull: () => void;
  onPush: () => void;
  onRedetectSync: () => void;
  onCancelSync: () => void;
  onToggleFeaturePlugin: (id: string, enabled: boolean) => void;
  onAssociateChromeProfile: (profileDir: string) => void;
  onRefreshChromeBookmarks: () => void;
  onCreateMemo: (title: string, tags: string[], body: string) => Promise<boolean>;
  onUpdateMemo: (id: string, title: string, tags: string[], body: string) => Promise<boolean>;
  onDeleteMemo: (id: string) => Promise<boolean>;
  onToggleClipboardPaused: (paused: boolean) => void;
  onSaveClipboardLimits: (retentionDays: number, capacity: number) => void;
  onPinClipboardEntry: (id: string, pinned: boolean) => void;
  onDeleteClipboardEntry: (id: string) => void;
  onClearClipboardHistory: () => void;
  /** 显式为某个文件引用保存本机副本（ticket 12）。 */
  onSaveClipboardFileCopy: (id: string, attachmentId: string) => void;
  /** 当前显示的区块。由 App 持有，所以在会话内切走再回来会回到同一区块。 */
  current: SettingsSectionId;
  onSectionChange: (section: SettingsSectionId) => void;
}

/** 设置页的区块标识。App 持有「当前区块」以便在会话内记住它。 */
export type SettingsSectionId =
  | "hotkey"
  | "theme"
  | "plugins"
  | "clipboard"
  | "memos"
  | "chrome"
  | "workspace"
  | "sync"
  | "changes"
  | "capabilities";

/**
 * 设置页的分组与区块。
 *
 * 分组按**使用频率**排，而不是按原来的平铺顺序：常用 → 数据与内容 → 配置与同步 → 关于。
 * 原来的平铺把低频运维放在最前面（配置工作区 414px + 远端同步 327px + 变更与提交 117px
 * 占掉 640x420 窗口里最前面的 2.4 屏），而「全局快捷键」排第 4、「外观主题」排第 6。
 *
 * 区块在 DOM 里始终存在、只按当前项切换 `hidden`（见 `.settings-body` 的规则）：
 * 切走再切回不会丢掉正在编辑的内容（备忘录正文、剪贴板确认态等）。
 */
export const SETTINGS_GROUPS: {
  id: string;
  label: string;
  sections: { id: SettingsSectionId; title: string }[];
}[] = [
  {
    id: "common",
    label: "常用",
    sections: [
      { id: "hotkey", title: "全局快捷键" },
      { id: "theme", title: "外观主题" },
    ],
  },
  {
    id: "content",
    label: "数据与内容",
    sections: [
      { id: "plugins", title: "功能插件" },
      { id: "clipboard", title: "剪贴板历史" },
      { id: "memos", title: "备忘录" },
      { id: "chrome", title: "Chrome 书签" },
    ],
  },
  {
    id: "config",
    label: "配置与同步",
    sections: [
      { id: "workspace", title: "配置工作区" },
      { id: "sync", title: "远端同步" },
      { id: "changes", title: "变更与提交" },
    ],
  },
  {
    id: "about",
    label: "关于",
    sections: [{ id: "capabilities", title: "运行环境" }],
  },
];

/** 设置页：显示当前配置工作区、关联现有仓库或初始化新目录、显示校验失败原因，
 * 编辑全局快捷键，并管理主题（内置浅色 / 深色 / 跟随系统与已安装的本地主题包）。
 *
 * 延续紧凑列表的视觉语言（`styles.css` 的设计令牌，无 CSS 框架、无动画）：
 * 高频操作（快捷键保存、工作区切换、主题切换）不做过渡动画。
 */
export function SettingsScreen({
  workspace,
  settings,
  theme,
  materialNotice,
  onAppearanceChange,
  hotkey,
  capabilities,
  message,
  busy,
  cloneProgress,
  changes,
  sync,
  syncProgress,
  commitMessage,
  selectedPaths,
  diffPath,
  plugins,
  memos,
  memoProblems,
  memoEnabled,
  chrome,
  clipboard,
  onBack,
  onSelectWorkspace,
  onInitWorkspace,
  onSaveHotkey,
  onCloneWorkspace,
  onCancelClone,
  onSelectTheme,
  onToggleTheme,
  onInstallTheme,
  onRemoveTheme,
  onTogglePath,
  onToggleAllPaths,
  onSelectDiff,
  onCommitMessageChange,
  onCommit,
  onRefreshChanges,
  onPull,
  onPush,
  onRedetectSync,
  onCancelSync,
  onToggleFeaturePlugin,
  onAssociateChromeProfile,
  onRefreshChromeBookmarks,
  onCreateMemo,
  onUpdateMemo,
  onDeleteMemo,
  onToggleClipboardPaused,
  onSaveClipboardLimits,
  onPinClipboardEntry,
  onDeleteClipboardEntry,
  onClearClipboardHistory,
  onSaveClipboardFileCopy,
  current,
  onSectionChange,
}: Props) {
  const [path, setPath] = useState(workspace?.path ?? "");
  const [hotkeyConflict, setHotkeyConflict] = useState<HotkeyConflictReport | null>(null);
  const [hotkeyBusy, setHotkeyBusy] = useState(false);
  const [hotkeyDraft, setHotkeyDraft] = useState(settings?.hotkey ?? "");
  const [remoteUrl, setRemoteUrl] = useState("");
  const [tokenUser, setTokenUser] = useState("");
  const [token, setToken] = useState("");
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
      if (event.key === "Escape" && !document.querySelector("dialog[open]")) {
        event.preventDefault();
        onBack();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [onBack]);

  // ↑↓ 在侧栏里移动区块。同样挂在 window 上：点过按钮后焦点会落到 body。
  // 输入框与多行文本自己处理方向键，所以可编辑控件上不接管（←→ 与数字键另有用途）。
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey || document.querySelector("dialog[open]")) {
        return;
      }
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") {
        return;
      }
      const target = event.target as HTMLElement | null;
      if (
        target &&
        (/^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName) || target.isContentEditable)
      ) {
        return;
      }
      event.preventDefault();
      const flat = SETTINGS_GROUPS.flatMap((group) => group.sections);
      const index = flat.findIndex((section) => section.id === current);
      const next =
        event.key === "ArrowDown"
          ? Math.min(index + 1, flat.length - 1)
          : Math.max(index - 1, 0);
      onSectionChange(flat[next].id);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [current, onSectionChange]);

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
          ‹ 搜索
        </button>
        <span className="settings-signature">FLASHCAST</span>
      </header>

      {/* 提示条紧贴头部、常驻可见。它原本在整个设置页的最底部：内容总高 3837px 时，
          在靠前的区块里操作完根本看不到反馈。 */}
      {message ? (
        <div
          className={`banner banner-${message.level}`}
          data-testid="settings-message"
          role={message.level === "error" ? "alert" : "status"}
        >
          {message.text}
        </div>
      ) : null}

      <nav className="settings-nav" data-testid="settings-nav" aria-label="设置分组">
        {SETTINGS_GROUPS.map((group) => (
          <div className="settings-nav-group" key={group.id}>
            <div className="settings-nav-label">{group.label}</div>
            {group.sections.map((section) => (
              <button
                key={section.id}
                type="button"
                className="settings-nav-item"
                data-testid={`settings-nav-${section.id}`}
                data-current={section.id === current ? "true" : "false"}
                aria-current={section.id === current ? "true" : undefined}
                onClick={() => onSectionChange(section.id)}
              >
                <Glyph name={({memos:"memo",clipboard:"clipboardEntry",chrome:"bookmark"} as Record<string,string>)[section.id] ?? section.id} />
                {section.title}
              </button>
            ))}
          </div>
        ))}
      </nav>

      <div className="settings-body">
        <section
          className="settings-section"
          data-section="workspace"
          hidden={current !== "workspace"}
          data-testid="workspace-section"
        >
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
            {workspace?.remote ? (
              <div className="settings-fact">
                <dt>远端</dt>
                <dd data-testid="workspace-remote">
                  {workspace.remote.name} → {workspace.remote.url}
                  {`（分支 ${workspace.remote.branch}`}
                  {workspace.remote.upstream
                    ? `，上游 ${workspace.remote.upstream}）`
                    : "，未设置上游）"}
                </dd>
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

          <h3 className="settings-section-title">从远端克隆</h3>
          <label className="settings-label" htmlFor="clone-url">
            远端地址（克隆到上方的「本地目录」）
          </label>
          <div className="settings-row">
            <input
              id="clone-url"
              className="path-input"
              data-testid="clone-url-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="https://github.com/用户名/flashcast-config.git"
              value={remoteUrl}
              onChange={(event) => setRemoteUrl(event.target.value)}
            />
          </div>
          <div className="settings-row">
            <input
              id="clone-user"
              className="path-input"
              data-testid="clone-user-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="令牌用户名（可留空，默认 x-access-token）"
              value={tokenUser}
              onChange={(event) => setTokenUser(event.target.value)}
            />
            <input
              id="clone-token"
              className="path-input"
              data-testid="clone-token-input"
              type="password"
              autoComplete="off"
              placeholder="访问令牌（只保存在本机设备目录）"
              value={token}
              onChange={(event) => setToken(event.target.value)}
            />
          </div>
          <div className="settings-row">
            <button
              type="button"
              className="primary-button"
              data-testid="workspace-clone"
              disabled={busy}
              onClick={() =>
                onCloneWorkspace(
                  remoteUrl,
                  path,
                  token.trim().length > 0
                    ? { username: tokenUser, token }
                    : null,
                )
              }
            >
              从远端克隆
            </button>
            <button
              type="button"
              className="secondary-button"
              data-testid="workspace-clone-cancel"
              disabled={!busy}
              onClick={onCancelClone}
            >
              取消克隆
            </button>
          </div>
          {cloneProgress ? (
            <p className="settings-hint" data-testid="clone-progress" role="status">
              {describeCloneProgress(cloneProgress)}
            </p>
          ) : null}
          <p className="settings-hint">
            目标目录必须是空目录：已有文件时拒绝克隆，不会覆盖。失败或取消会自动清理
            本次创建的内容，当前工作区与设置保持不变。https 令牌只保存在本机设备目录，
            不会写入工作区或日志；ssh 复用 ssh-agent 与 ~/.ssh 下的密钥。
          </p>
        </section>

        <section
          className="settings-section"
          data-section="sync"
          hidden={current !== "sync"}
          data-testid="sync-section"
        >
          <h2 className="settings-section-title">远端同步</h2>

          {sync ? (
            <>
              <dl className="settings-facts">
                <div className="settings-fact">
                  <dt>分支</dt>
                  <dd data-testid="sync-branch">
                    {sync.branch ?? (sync.detached ? "分离 HEAD" : "尚无分支")}
                  </dd>
                </div>
                <div className="settings-fact">
                  <dt>远端</dt>
                  <dd data-testid="sync-remote">
                    {sync.remoteName && sync.remoteUrl
                      ? `${sync.remoteName} → ${sync.remoteUrl}`
                      : "未配置远端"}
                    {sync.upstream ? `（上游 ${sync.upstream}）` : ""}
                  </dd>
                </div>
                <div className="settings-fact">
                  <dt>待同步</dt>
                  <dd data-testid="sync-counts">
                    {sync.tracking
                      ? `领先 ${sync.ahead} 个提交，落后 ${sync.behind} 个提交`
                      : "尚未取得远端状态（先拉取一次）"}
                  </dd>
                </div>
                <div className="settings-fact">
                  <dt>未提交改动</dt>
                  <dd data-testid="sync-dirty">{describeDirty(sync)}</dd>
                </div>
                <div className="settings-fact">
                  <dt>同步能力</dt>
                  <dd data-testid="sync-capability">
                    {`拉取${sync.canPull ? "可用" : "不可用"}，推送${
                      sync.canPush ? "可用" : "不可用"
                    }`}
                    {sync.nothingToPush ? "（本地没有需要推送的提交）" : ""}
                  </dd>
                </div>
                <div className="settings-fact">
                  <dt>进行中的操作</dt>
                  <dd data-testid="sync-state">{sync.state ?? "无"}</dd>
                </div>
              </dl>

              {sync.error ? (
                <div className="banner banner-error" data-testid="sync-error" role="alert">
                  {sync.error}
                </div>
              ) : null}

              {sync.blocking ? (
                <div className="banner banner-warning" data-testid="sync-block" role="status">
                  <strong data-testid="sync-block-label">{sync.blocking.label}</strong>
                  {sync.blocking.detail ? `：${sync.blocking.detail}` : ""}
                  <p className="settings-hint" data-testid="sync-block-hint">
                    {sync.blocking.hint}
                  </p>
                </div>
              ) : (
                <p className="settings-hint" data-testid="sync-block">
                  没有阻塞：可以进行无冲突的快进拉取与推送。
                </p>
              )}

              <div className="settings-row">
                <button
                  type="button"
                  className="primary-button"
                  data-testid="sync-pull"
                  disabled={busy || !sync.canPull}
                  onClick={onPull}
                >
                  拉取（仅快进）
                </button>
                <button
                  type="button"
                  className="primary-button"
                  data-testid="sync-push"
                  disabled={busy || !sync.canPush}
                  onClick={onPush}
                >
                  推送
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  data-testid="sync-redetect"
                  disabled={busy}
                  onClick={onRedetectSync}
                >
                  重新检测
                </button>
                <button
                  type="button"
                  className="secondary-button"
                  data-testid="sync-cancel"
                  disabled={!busy}
                  onClick={onCancelSync}
                >
                  取消同步
                </button>
              </div>

              {syncProgress ? (
                <p className="settings-hint" data-testid="sync-progress" role="status">
                  {describeSyncProgress(syncProgress)}
                </p>
              ) : null}

              <p className="settings-hint">
                同步只作用于本配置工作区：快进拉取会重新加载有效设置、主题与备忘录；
                分叉、冲突、未提交修改与进行中的 Git 操作都会先阻塞并给出外部处理指引，
                首版不提供内置三方合并编辑器，也绝不强推或自动丢弃更改。
              </p>
            </>
          ) : (
            <p className="settings-hint" data-testid="sync-unavailable">
              尚未读取同步状态。
            </p>
          )}
        </section>

        <div data-section="changes" hidden={current !== "changes"}>
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
        </div>

        <section
          className="settings-section"
          data-section="hotkey"
          hidden={current !== "hotkey"}
          data-testid="hotkey-section"
        >
          <h2 className="settings-section-title">全局快捷键</h2>
          <div className="settings-row">
            <input
              id="hotkey"
              className="path-input"
              data-testid="hotkey-input"
              type="text"
              spellCheck={false}
              autoComplete="off"
              placeholder="Alt+Space"
              disabled={busy || hotkeyBusy}
              value={hotkeyDraft}
              onChange={(event) => setHotkeyDraft(event.target.value)}
            />
            <button
              type="button"
              className="primary-button"
              data-testid="hotkey-save"
              disabled={busy || hotkeyBusy}
              onClick={() => onSaveHotkey(hotkeyDraft)}
            >
              保存
            </button>
          </div>
          <p className="settings-hint" data-testid="hotkey-status">
            当前绑定：{hotkey?.pending ? "等待系统授权" : hotkey?.registered ? hotkeyConflict?.effective ?? hotkey.label : "未注册"}
            {hotkey?.error ? `（${hotkey.error}）` : ""}
          </p>
          <HotkeyConflictPanel desired={settings?.hotkey ?? ""} hotkey={hotkey} active={current === "hotkey"} busy={busy} onBusyChange={setHotkeyBusy} onReportChange={setHotkeyConflict} onSaveHotkey={onSaveHotkey}/>
          <p className="settings-hint">系统决定最终绑定的按键。修改后请确认可以唤起 Flashcast。</p>
        </section>

        <section
          className="settings-section"
          data-section="capabilities"
          hidden={current !== "capabilities"}
          data-testid="capability-section"
        >
          <h2 className="settings-section-title">运行环境与能力</h2>
          <dl className="settings-facts">
            <div className="settings-fact">
              <dt>操作系统</dt>
              <dd data-testid="capability-os">
                {capabilities
                  ? `${OS_LABEL[capabilities.os] ?? capabilities.os}${
                      capabilities.osVersion ? ` ${capabilities.osVersion}` : "（版本未知）"
                    }`
                  : "尚未加载"}
              </dd>
            </div>
            <div className="settings-fact">
              <dt>架构</dt>
              <dd data-testid="capability-arch">{capabilities?.arch ?? "尚未加载"}</dd>
            </div>
            <div className="settings-fact">
              <dt>桌面会话</dt>
              <dd data-testid="capability-session">
                {capabilities
                  ? SESSION_LABEL[capabilities.session] ?? capabilities.session
                  : "尚未加载"}
                {capabilities && capabilities.session !== "not-applicable"
                  ? capabilities.desktopAvailable
                    ? "（存在可交互桌面）"
                    : "（无可用桌面会话）"
                  : ""}
              </dd>
            </div>
            {capabilities ? (
              <>
                <CapabilityRow id="hotkey" label="全局快捷键" support={capabilities.hotkey} />
                <CapabilityRow id="clipboard" label="剪贴板" support={capabilities.clipboard} />
                <CapabilityRow id="auto-paste" label="自动粘贴" support={capabilities.autoPaste} />
              </>
            ) : null}
          </dl>

          {capabilities && capabilities.notes.length > 0 ? (
            <ul className="settings-hint" data-testid="capability-notes">
              {capabilities.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          ) : null}

          <p className="settings-hint" data-testid="capability-disclaimer">
            以上是这台机器上的实际探测结果：「未覆盖」表示当前环境无法判定，不代表支持；
            X11 下的结果不能推断 Wayland 可用。本报告只包含系统、会话与权限状态，
            不包含剪贴板内容、书签或凭证。
          </p>
        </section>

        <section
          className="settings-section"
          data-section="theme"
          hidden={current !== "theme"}
          data-testid="theme-section"
        >
          <h2 className="settings-section-title">外观</h2>
          {theme ? <AppearanceControls theme={theme} busy={busy} materialNotice={materialNotice} onChange={onAppearanceChange} /> : null}

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
            {(theme?.themes ?? []).filter(entry => !entry.builtin || !entry.legacy || entry.selected).map((entry) => (
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
                    {!entry.legacy ? "深浅成对" : entry.appearance === "system"
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
                  disabled={busy || !entry.canDisable}
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
            选择 theme.json 文件或所在目录。主题需同时提供浅色和深色设计；安装失败会保留当前外观。
          </p>
        </section>

        <div data-section="plugins" hidden={current !== "plugins"}>
          <FeaturePluginsPanel
            plugins={plugins}
            busy={busy}
            onToggle={onToggleFeaturePlugin}
          />
        </div>

        <div data-section="chrome" hidden={current !== "chrome"}>
          <ChromePanel
            chrome={chrome}
            busy={busy}
            onAssociate={onAssociateChromeProfile}
            onRefresh={onRefreshChromeBookmarks}
          />
        </div>

        <div data-section="clipboard" hidden={current !== "clipboard"}>
          <ClipboardPanel
            clipboard={clipboard}
            busy={busy}
            onEnable={() => onToggleFeaturePlugin("clipboard", true)}
            onTogglePaused={onToggleClipboardPaused}
            onSaveLimits={onSaveClipboardLimits}
            onPin={onPinClipboardEntry}
            onDelete={onDeleteClipboardEntry}
            onClear={onClearClipboardHistory}
            onSaveCopy={onSaveClipboardFileCopy}
          />
        </div>

        <div data-section="memos" hidden={current !== "memos"}>
          <MemoPanel
            linked={linked}
            enabled={memoEnabled}
            memos={memos}
            problems={memoProblems}
            busy={busy}
            onCreate={onCreateMemo}
            onUpdate={onUpdateMemo}
            onDelete={onDeleteMemo}
          />
        </div>
      </div>
    </div>
  );
}
