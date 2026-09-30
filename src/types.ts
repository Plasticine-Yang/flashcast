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

export interface ActionOutcome {
  status: "done" | "copiedNeedsManualPaste" | "failed";
  message: string | null;
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
  error: string | null;
}

/** 宿主推送的工作区事件。 */
export interface WorkspaceEvent {
  status: WorkspaceStatus;
  settings: Settings;
  reload: WorkspaceReload | null;
}

export type UnlistenFn = () => void;
