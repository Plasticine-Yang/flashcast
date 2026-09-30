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

/** 宿主推送的工作区事件。 */
export interface WorkspaceEvent {
  status: WorkspaceStatus;
  settings: Settings;
  theme: ThemeState;
  reload: WorkspaceReload | null;
}

export type UnlistenFn = () => void;
