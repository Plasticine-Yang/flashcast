// 与宿主交互的唯一入口。
//
// 在 Tauri 中运行时走 `invoke`；在普通浏览器中（UI 浏览器交互检查）走本文件内的
// 确定性模拟宿主。模拟宿主实现与 `flashcast_core::Host` 相同的语义：
// seq 单调递增、同一输入重复渲染保留键盘选择、空查询给快速访问项、
// 启动失败给出中文反馈。**模拟宿主只用于开发与浏览器检查，不是产品行为。**

import type {
  ActionOutcome,
  BackView,
  Capabilities,
  QueryView,
  Settings,
  StatusView,
  UnlistenFn,
} from "./types";

const inTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in (window as object);

type Handler = (payload: unknown) => void;

export interface HostApi {
  query(input: string): Promise<QueryView>;
  execute(itemId: string): Promise<ActionOutcome>;
  move_selection(delta: number): Promise<QueryView>;
  set_selection(index: number): Promise<QueryView>;
  back(): Promise<BackView>;
  rescan(): Promise<QueryView>;
  refresh_state(): Promise<QueryView>;
  get_capabilities(): Promise<Capabilities>;
  get_settings(): Promise<Settings>;
  set_settings(settings: Settings): Promise<{ label: string; error: string | null; registered: boolean }>;
  get_status(): Promise<StatusView>;
  hide_window(): Promise<void>;
  on(event: string, handler: Handler): Promise<UnlistenFn>;
  readonly kind: "tauri" | "browser";
}

async function tauriInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<T>(command, args);
}

const tauriApi: HostApi = {
  kind: "tauri",
  query: (input) => tauriInvoke("query", { input }),
  execute: (itemId) => tauriInvoke("execute", { itemId }),
  move_selection: (delta) => tauriInvoke("move_selection", { delta }),
  set_selection: (index) => tauriInvoke("set_selection", { index }),
  back: () => tauriInvoke("back"),
  rescan: () => tauriInvoke("rescan"),
  refresh_state: () => tauriInvoke("refresh_state"),
  get_capabilities: () => tauriInvoke("get_capabilities"),
  get_settings: () => tauriInvoke("get_settings"),
  set_settings: (settings) => tauriInvoke("set_settings", { settings }),
  get_status: () => tauriInvoke("get_status"),
  hide_window: () => tauriInvoke("hide_window"),
  on: async (event, handler) => {
    const { listen } = await import("@tauri-apps/api/event");
    const unlisten = await listen(event, (message) => handler(message.payload));
    return unlisten;
  },
};

// ---------------------------------------------------------------------------
// 浏览器模拟宿主
// ---------------------------------------------------------------------------

interface MockApp {
  id: string;
  title: string;
  subtitle: string | null;
  keywords: string[];
  /** 模拟启动失败的软件，用于验证失败反馈。 */
  failsToLaunch?: boolean;
}

const MOCK_APPS: MockApp[] = [
  { id: "firefox", title: "Firefox 浏览器", subtitle: "浏览网页", keywords: ["browser", "web"] },
  { id: "files", title: "文件", subtitle: "浏览本地文件", keywords: ["files", "nautilus"] },
  { id: "terminal", title: "终端", subtitle: "命令行", keywords: ["terminal", "shell"] },
  { id: "code", title: "Visual Studio Code", subtitle: "代码编辑器", keywords: ["editor", "code"] },
  { id: "gimp", title: "GIMP 图像编辑器", subtitle: "位图编辑", keywords: ["image"] },
  { id: "calc", title: "计算器", subtitle: "基础计算", keywords: ["calculator"] },
  { id: "music", title: "音乐播放器", subtitle: "播放本地音乐", keywords: ["music", "audio"] },
  { id: "broken", title: "损坏的示例软件", subtitle: "用于验证启动失败反馈", keywords: ["broken"], failsToLaunch: true },
];

const MOCK_CAPABILITIES: Capabilities = {
  os: "linux",
  osVersion: "浏览器模拟环境",
  arch: "x86_64",
  session: "x11",
  desktopAvailable: false,
  hotkey: { status: "unknown", reason: "浏览器中不检查全局快捷键" },
  clipboard: { status: "unknown", reason: "浏览器中不检查剪贴板" },
  autoPaste: { status: "unknown", reason: "浏览器中不检查自动粘贴" },
  notes: ["当前运行在浏览器模拟宿主中，不代表真实桌面行为。"],
};

class MockHost implements HostApi {
  readonly kind = "browser" as const;
  private seq = 0;
  private input = "";
  private selection = 0;
  private items: QueryView["items"] = [];
  private history: { input: string; selection: number }[] = [];
  private handlers = new Map<string, Set<Handler>>();
  private settings: Settings = {
    hotkey: "Ctrl+Alt+Space",
    launchAtStartup: false,
    quickAccessLimit: 6,
    pluginTimeoutMs: 400,
    disabledPlugins: [],
  };
  /** 最近一次「启动失败」的条目 id，供检查脚本断言。 */
  lastLaunched: string | null = null;
  hidden = false;

  private emit(event: string, payload?: unknown) {
    for (const handler of this.handlers.get(event) ?? []) {
      handler(payload);
    }
  }

  on(event: string, handler: Handler): Promise<UnlistenFn> {
    const set = this.handlers.get(event) ?? new Set<Handler>();
    set.add(handler);
    this.handlers.set(event, set);
    return Promise.resolve(() => set.delete(handler));
  }

  private buildItems(): QueryView["items"] {
    const query = this.input.trim().toLowerCase();
    const commandItems: QueryView["items"] = [
      {
        id: "flashcast.command.rescan",
        title: "重新扫描软件",
        subtitle: "刷新已安装软件列表",
        iconDataUrl: null,
        source: "flashcast",
        kind: "command",
        defaultAction: "open",
        defaultActionLabel: "打开",
        score: { tier: "titlePrefix", relevance: 0 },
      },
      {
        id: "flashcast.command.capabilities",
        title: "查看平台能力",
        subtitle: "显示会话类型与各能力的真实支持状态",
        iconDataUrl: null,
        source: "flashcast",
        kind: "command",
        defaultAction: "open",
        defaultActionLabel: "打开",
        score: { tier: "titlePrefix", relevance: 0 },
      },
    ];
    if (query.length === 0) {
      const quick = MOCK_APPS.slice(0, this.settings.quickAccessLimit).map((app) =>
        this.toItem(app, { tier: "titlePrefix", relevance: 0 }),
      );
      return [...commandItems, ...quick];
    }
    const scored = MOCK_APPS.map((app) => {
      const title = app.title.toLowerCase();
      if (title === query) return { app, tier: "titlePrefix" as const, relevance: 100 };
      if (title.startsWith(query)) return { app, tier: "titlePrefix" as const, relevance: 80 };
      if (title.includes(query)) return { app, tier: "titleSubstring" as const, relevance: 55 };
      if (app.keywords.some((keyword) => keyword.includes(query))) {
        return { app, tier: "metadataSubstring" as const, relevance: 30 };
      }
      if ((app.subtitle ?? "").toLowerCase().includes(query)) {
        return { app, tier: "metadataSubstring" as const, relevance: 25 };
      }
      return null;
    }).filter((value): value is NonNullable<typeof value> => value !== null);
    const tierOrder = { titlePrefix: 0, titleSubstring: 1, metadataSubstring: 2, keywordOrTagExact: 0 };
    scored.sort(
      (a, b) =>
        tierOrder[a.tier] - tierOrder[b.tier] ||
        b.relevance - a.relevance ||
        a.app.title.localeCompare(b.app.title),
    );
    return scored.map(({ app, tier, relevance }) => this.toItem(app, { tier, relevance }));
  }

  private toItem(app: MockApp, score: QueryView["items"][number]["score"]): QueryView["items"][number] {
    return {
      id: `app:${app.id}`,
      title: app.title,
      subtitle: app.subtitle,
      iconDataUrl: null,
      source: "flashcast",
      kind: "application",
      defaultAction: "open",
      defaultActionLabel: "打开",
      score,
    };
  }

  private response(notice: QueryView["notice"] = null): QueryView {
    this.seq += 1;
    return {
      seq: this.seq,
      scope: { kind: "home" },
      scopeLabel: "首屏",
      input: this.input,
      items: this.items,
      selection: this.selection,
      notice,
      pluginFailures: [],
    };
  }

  async query(input: string): Promise<QueryView> {
    const sameInput = this.input === input;
    this.input = input;
    this.items = this.buildItems();
    if (!sameInput) {
      this.selection = 0;
    }
    this.selection = Math.min(this.selection, Math.max(0, this.items.length - 1));
    return this.response();
  }

  async move_selection(delta: number): Promise<QueryView> {
    this.selection = Math.max(0, Math.min(this.items.length - 1, this.selection + delta));
    return this.response();
  }

  async set_selection(index: number): Promise<QueryView> {
    this.selection = Math.max(0, Math.min(this.items.length - 1, index));
    return this.response();
  }

  async back(): Promise<BackView> {
    const entry = this.history.pop();
    if (!entry) {
      return { restored: false, response: this.response() };
    }
    this.input = entry.input;
    this.items = this.buildItems();
    this.selection = entry.selection;
    return { restored: true, response: this.response() };
  }

  async rescan(): Promise<QueryView> {
    this.items = this.buildItems();
    return this.response({ level: "info", message: "已重新扫描软件列表（浏览器模拟）" });
  }

  async refresh_state(): Promise<QueryView> {
    return this.response();
  }

  async get_capabilities(): Promise<Capabilities> {
    return MOCK_CAPABILITIES;
  }

  async get_settings(): Promise<Settings> {
    return this.settings;
  }

  async set_settings(settings: Settings): Promise<{ label: string; error: string | null; registered: boolean }> {
    this.settings = settings;
    return { label: settings.hotkey, error: null, registered: true };
  }

  async get_status(): Promise<StatusView> {
    return {
      previousApp: null,
      hotkey: { label: this.settings.hotkey, error: null, registered: true },
      capabilities: MOCK_CAPABILITIES,
      plugins: [],
    };
  }

  async hide_window(): Promise<void> {
    this.hidden = true;
    this.emit("flashcast://dismissed");
  }

  /** 模拟用户按下全局快捷键唤起窗口。 */
  summon(): void {
    this.hidden = false;
    this.emit("flashcast://summoned", { previousApp: null });
  }
}

const mock = new MockHost();

// 供浏览器交互检查脚本驱动「唤起」与观察隐藏状态。
declare global {
  interface Window {
    __flashcastMock?: MockHost;
  }
}
if (typeof window !== "undefined" && !inTauri) {
  window.__flashcastMock = mock;
}

export const api: HostApi = inTauri ? tauriApi : mock;
export const isBrowserMock = !inTauri;
