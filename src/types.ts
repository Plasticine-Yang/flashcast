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

export type UnlistenFn = () => void;
