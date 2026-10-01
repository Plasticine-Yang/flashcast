// 与 src-tauri 的 command 返回结构一一对应。字段名为 camelCase。

export type ItemKind =
  | "application"
  | "memo"
  | "clipboardEntry"
  | "bookmark"
  | "command";

export type DefaultAction = "open" | "openInChrome" | "paste";

export type QueryScope =
  | { kind: "home" }
  | { kind: "plugin"; id: string; keyword: string };

export interface Score {
  tier: "keywordOrTagExact" | "titlePrefix" | "titleSubstring" | "metadataSubstring";
  relevance: number;
}

export interface ItemView {
  id: string;
  title: string;
  subtitle: string | null;
  iconDataUrl: string | null;
  source: string;
  kind: ItemKind;
  defaultAction: DefaultAction;
  defaultActionLabel: string;
  score: Score;
}

export interface Notice {
  level: "info" | "warning" | "error";
  message: string;
}

export interface PluginFailure {
  pluginId: string;
  reason: string;
  kind: "error" | "timeout" | "panic";
}

export interface QueryView {
  seq: number;
  scope: QueryScope;
  scopeLabel: string;
  input: string;
  items: ItemView[];
  selection: number;
  notice: Notice | null;
  pluginFailures: PluginFailure[];
}

/**
 * 命令入口的返回。
 *
 * `pastePending` 是宿主与外壳之间的中间状态（「已复制，等外壳关窗后完成粘贴」）：
 * 真实外壳（`src-tauri/src/commands.rs`）会先关闭浮窗、恢复目标应用并注入粘贴，
 * 再把最终状态（`done` / `copiedNeedsManualPaste`）交给 UI，因此界面上通常看不到它。
 */
export interface ActionOutcome {
  status: "done" | "copiedNeedsManualPaste" | "pastePending" | "failed";
  message: string | null;
  /** 待完成的粘贴计划（状态为 `pastePending` 时存在）。 */
  paste?: PastePlan | null;
}

/** 自动粘贴计划：内容是唤起前捕获的目标应用与本次执行的序号。 */
export interface PastePlan {
  target: { id: string; name: string; wmClass: string | null; pid: number | null; window: number | null };
  label: string;
  epoch: number;
  textBytes: number;
}

export interface BackView {
  restored: boolean;
  response: QueryView;
}

export interface Capabilities {
  os: string;
  osVersion: string | null;
  arch: string;
  session: string;
  desktopAvailable: boolean;
  hotkey: Support;
  clipboard: Support;
  autoPaste: Support;
  notes: string[];
}

export type Support =
  | { status: "supported" }
  | { status: "unsupported"; reason: string }
  | { status: "unknown"; reason: string };

export interface FocusedApp {
  id: string;
  name: string;
  wmClass: string | null;
  pid: number | null;
  window: number | null;
}

export interface HotkeyStatus {
  label: string;
  error: string | null;
  registered: boolean;
}

export interface PluginView {
  id: string;
  name: string;
  version: string;
  keywords: string[];
  enabled: boolean;
}

/** 实际生效的外观。 */
export type Appearance = "light" | "dark";

/** 主题声明的外观偏好；「跟随系统」按当前系统外观解析。 */
export type ThemeAppearance = "light" | "dark" | "system";

/** 宿主下发的一条 CSS 自定义属性。UI 只消费属性，不认识具体主题。 */
export interface CssVar {
  name: string;
  value: string;
}

/** 主题列表里的一条。 */
export interface ThemeEntry {
  id: string;
  name: string;
  version: string;
  enabled: boolean;
  selected: boolean;
  /** 内置主题不能被移除。 */
  builtin: boolean;
  appearance: ThemeAppearance;
  /** 主题文档当前是否能解析并通过校验。 */
  usable: boolean;
  error: string | null;
}

/** 宿主当前的主题状态（对应 flashcast_core::ThemeState）。 */
export interface ThemeState {
  selected: string;
  selectedName: string;
  preference: ThemeAppearance;
  appearance: Appearance;
  systemAppearance: Appearance;
  /** 直接写入根元素的 CSS 自定义属性。 */
  cssVars: CssVar[];
  themes: ThemeEntry[];
  /** 最近一次无效主题的中文原因；此时外观仍是上一次可用的。 */
  error: string | null;
}

/** 插件清单条目（对应 flashcast_core::ManifestEntry）。 */
export interface ManifestEntry {
  id: string;
  name: string;
  kind: "feature" | "theme";
  version: string;
  enabled: boolean;
  keywords: string[];
  capabilities: string[];
  origin: "builtin" | "registered" | "installed";
  appearance: ThemeAppearance | null;
}

export interface StatusView {
  previousApp: FocusedApp | null;
  hotkey: HotkeyStatus;
  capabilities: Capabilities;
  plugins: PluginView[];
}

export interface Settings {
  hotkey: string;
  launchAtStartup: boolean;
  quickAccessLimit: number;
  pluginTimeoutMs: number;
  disabledPlugins: string[];
}

/** 当前配置工作区与它的有效性（对应 flashcast_core::WorkspaceStatus）。 */
export interface WorkspaceStatus {
  /** 工作区根目录；null 表示尚未关联工作区。 */
  path: string | null;
  /** Git 仓库目录（`.git`）；不是 Git 仓库时为 null。 */
  gitDir: string | null;
  valid: boolean;
  settingsFile: string | null;
  /** 设置是否落在工作区文件里；未关联时只在内存中，重启不保留。 */
  persisted: boolean;
  /** 最近一次失败的中文原因。 */
  error: string | null;
  /** 远端仓库关系（克隆得到的工作区才有）。 */
  remote: WorkspaceRemote | null;
}

/** 工作区与远端仓库的关系（对应 flashcast_core::WorkspaceRemote）。 */
export interface WorkspaceRemote {
  /** 远端名，通常是 origin。 */
  name: string;
  /** 远端地址（已去掉 userinfo）。 */
  url: string;
  /** 工作区当前分支。 */
  branch: string;
  /** 上游跟踪分支（例如 origin/main）；未设置时为 null。 */
  upstream: string | null;
}

/** 克隆阶段（对应 flashcast_core::ClonePhase）。 */
export type ClonePhase =
  | "idle"
  | "connecting"
  | "receiving"
  | "resolving"
  | "checkingOut"
  | "done"
  | "failed"
  | "cancelled";

/** 一次克隆的进度快照（对应 flashcast_core::CloneProgress）。 */
export interface CloneProgress {
  phase: ClonePhase;
  receivedObjects: number;
  totalObjects: number;
  indexedObjects: number;
  receivedBytes: number;
  /** 当前检出的文件（相对工作区）。 */
  checkoutPath: string | null;
  checkoutCompleted: number;
  checkoutTotal: number;
  /** 已「计划通知」的文件数：取消在这一轮里生效。 */
  checkoutNotified: number;
  /** 真实回调触发次数；为 0 说明没有可报告的进度。 */
  updates: number;
  /** 中文说明：失败原因或结束语。 */
  message: string | null;
}

/** 克隆成功后的结果（对应 flashcast_core::CloneOutcome）。 */
export interface CloneOutcome {
  workspace: WorkspaceStatus;
  remote: WorkspaceRemote;
  /** 工作区记录为停用、但本机没有对应实现的插件 id。 */
  unavailablePlugins: string[];
  /** 工作区 theme.json 记录的主题名（主题支持由 ticket 06 落地）。 */
  recordedTheme: string | null;
}

/** 一次外部修改被处理后的结果。 */
export interface WorkspaceReload {
  path: string;
  applied: boolean;
  settings: Settings;
  /** 处理之后生效的主题状态（无效主题保留上一次可用外观）。 */
  theme: ThemeState;
  error: string | null;
}

/** 变更列表中的一个文件（对应 flashcast_core::ChangedFile）。 */
export interface ChangedFile {
  /** 仓库相对路径，`/` 分隔。 */
  path: string;
  /** `git status --short` 风格的两列代码。 */
  code: string;
  /** 面向用户的中文状态说明。 */
  statusLabel: string;
  /** 索引里已有改动（已暂存）：不会随本次提交自动纳入。 */
  staged: boolean;
  /** 工作区里有改动（未暂存）。 */
  unstaged: boolean;
  untracked: boolean;
  conflicted: boolean;
  /** 真实补丁文本（差异基准见 WorkspaceChanges.diffBase）。 */
  diff: string;
  diffTruncated: boolean;
}

/** 工作区 Git 变更快照（对应 flashcast_core::WorkspaceChanges）。 */
export interface WorkspaceChanges {
  repository: boolean;
  branch: string | null;
  detached: boolean;
  hasChanges: boolean;
  /** 差异基准说明，例如 `HEAD (1a2b3c4) → 工作区（含已暂存改动）`。 */
  diffBase: string;
  files: ChangedFile[];
  /** 工作区异常（合并 / 变基 / 分离 HEAD……）的中文说明。 */
  state: string | null;
  error: string | null;
}

/** 一次成功提交的结果（对应 flashcast_core::CommitOutcome）。 */
export interface CommitOutcome {
  oid: string;
  short: string;
  message: string;
  authorName: string;
  authorEmail: string;
  paths: string[];
  changes: WorkspaceChanges;
}

/** 宿主推送的工作区事件。 */
export interface WorkspaceEvent {
  status: WorkspaceStatus;
  settings: Settings;
  theme: ThemeState;
  reload: WorkspaceReload | null;
}

/** 同步阻塞 / 失败分类（对应 flashcast_core::SyncBlockKind）。 */
export type SyncBlockKind =
  | "noWorkspace"
  | "notARepository"
  | "dirtyWorktree"
  | "diverged"
  | "conflicts"
  | "operationInProgress"
  | "detachedHead"
  | "noRemote"
  | "noUpstream"
  | "remoteBranchMissing"
  | "authFailed"
  | "offline"
  | "certificate"
  | "remoteError"
  | "remoteRejected"
  | "nothingToPush"
  | "unrelatedHistories"
  | "git";

/**
 * 一个同步阻塞状态：分类 + 具体情况 + 可操作的中文指引。
 * 指引一律指向「在应用外部处理，然后点重新检测」。
 */
export interface SyncBlock {
  code: SyncBlockKind;
  /** 中文短标签，例如「历史已分叉」。 */
  label: string;
  /** 具体情况（已脱敏）。 */
  detail: string;
  /** 可操作的中文指引。 */
  hint: string;
}

/** 当前工作区的同步状态（对应 flashcast_core::SyncStatus）。 */
export interface SyncStatus {
  repository: boolean;
  branch: string | null;
  detached: boolean;
  remote: WorkspaceRemote | null;
  remoteName: string | null;
  remoteUrl: string | null;
  upstream: string | null;
  /** 本地领先远端的提交数。 */
  ahead: number;
  /** 本地落后远端的提交数。 */
  behind: number;
  /** 是否已有本地远端跟踪引用（决定 ahead / behind 是否可信）。 */
  tracking: boolean;
  dirty: boolean;
  staged: boolean;
  unstaged: boolean;
  untracked: boolean;
  conflicted: boolean;
  /** 进行中的 Git 操作的中文说明。 */
  state: string | null;
  canPull: boolean;
  canPush: boolean;
  nothingToPush: boolean;
  /** 拉取的阻塞原因（含指引）；null 表示可以拉取。 */
  blocking: SyncBlock | null;
  /** 推送的阻塞原因；null 表示可以推送。 */
  pushBlocking: SyncBlock | null;
  /** 是否有同步操作正在进行。 */
  busy: boolean;
  error: string | null;
}

/** 拉取结果（对应 flashcast_core::PullResult）。 */
export interface PullResult {
  kind: "upToDate" | "fastForwarded";
  from?: string | null;
  to?: string;
  shortTo?: string;
  commits?: number;
}

/** 一次成功拉取的结果：核心报告 + 重新加载后的视图。 */
export interface PullOutcome {
  result: PullResult;
  status: SyncStatus;
  reload: WorkspaceReload;
  /** 拉取后工作区记录的主题名。 */
  theme: string | null;
  /** 拉取后 memos/ 下的备忘录（仓库相对路径）。 */
  memos: string[];
  message: string;
}

/** 一个被推送的引用。 */
export interface PushUpdateView {
  local: string;
  remote: string;
  localOid: string;
  remoteOid: string;
}

/** 一次成功推送的结果。 */
export interface PushOutcome {
  branch: string;
  remote: string;
  updated: PushUpdateView[];
  status: SyncStatus;
  message: string;
}

/** 同步阶段（对应 flashcast_core::SyncPhase）。 */
export type SyncPhase = "idle" | "fetching" | "pushing" | "done" | "failed" | "cancelled";

/** 一次同步的进度快照。 */
export interface SyncProgress {
  phase: SyncPhase;
  receivedObjects: number;
  totalObjects: number;
  receivedBytes: number;
  updates: number;
  message: string | null;
}

export type UnlistenFn = () => void;

// ---------------------------------------------------------------------------
// 备忘录（ticket 07）
// ---------------------------------------------------------------------------

/** 一条备忘录（对应 flashcast_core::Memo）。 */
export interface Memo {
  /** 稳定标识，同时是工作区里 `memos/<id>.md` 的文件名。 */
  id: string;
  title: string;
  /** 多个标签，顺序稳定。 */
  tags: string[];
  /** 文字正文。 */
  body: string;
}

/** 无法读取的备忘录文件：宿主保留可用内容并如实报告原因。 */
export interface MemoProblem {
  /** 工作区里的文件路径。 */
  path: string;
  /** 面向用户的中文原因。 */
  reason: string;
}

/**
 * 预览内容（对应 flashcast_core::Preview）。
 * 备忘录给出完整正文；其它条目类型在后续切片里扩展。
 */
export type Preview =
  | { kind: "none" }
  | { kind: "text"; title: string | null; body: string }
  | { kind: "image"; path: string };

// ---------------------------------------------------------------------------
// Chrome 书签（ticket 13）
// ---------------------------------------------------------------------------

/** 书签文件的状态（对应 flashcast_core::BookmarksStatus）。 */
export type BookmarksStatus =
  | { kind: "notAssociated" }
  | { kind: "missing" }
  | { kind: "ok"; count: number }
  | { kind: "corrupt"; reason: string }
  | { kind: "unreadable"; reason: string };

/** 索引快照（对应 flashcast_core::BookmarkSnapshot）。 */
export interface BookmarkSnapshot {
  /** 书签文件的本机路径；未关联时为 null。 */
  path: string | null;
  status: BookmarksStatus;
  entries: BookmarkEntry[];
}

/** 一条书签（对应 flashcast_core::BookmarkEntry）。 */
export interface BookmarkEntry {
  id: string;
  title: string;
  url: string;
  /** 目录路径，例如「书签栏 / 开发」。 */
  folder: string;
}

/** 一个 Chrome profile（对应 flashcast_core::ChromeProfileView）。 */
export interface ChromeProfileView {
  /** **目录名**（`Default`、`Profile 1`）：关联与 `--profile-directory` 用的就是它。 */
  dir: string;
  /** 显示名（来自 Local State 的 profile.info_cache）。 */
  name: string;
  userName: string | null;
  managed: boolean;
  hasBookmarks: boolean;
  bookmarksReadable: boolean;
  unreadableReason: string | null;
  associated: boolean;
}

/** 当前 Chrome 状态（对应 flashcast_core::ChromeState）。 */
export interface ChromeState {
  /** 是否发现了 Chrome 可执行文件。 */
  available: boolean;
  brandLabel: string | null;
  /** 用户数据目录是否在默认位置之外（决定是否传 --user-data-dir）。 */
  customUserDataDir: boolean;
  binary: string | null;
  userDataDir: string | null;
  profiles: ChromeProfileView[];
  associated: string | null;
  associatedName: string | null;
  error: string | null;
  warnings: string[];
  bookmarks: BookmarkSnapshot;
  bookmarksLabel: string;
}
