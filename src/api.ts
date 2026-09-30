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
  CloneOutcome,
  CloneProgress,
  QueryView,
  Settings,
  StatusView,
  UnlistenFn,
  WorkspaceEvent,
  WorkspaceStatus,
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
  /** 当前配置工作区与它的有效性。 */
  get_workspace(): Promise<WorkspaceStatus>;
  /** 关联已存在的本地仓库 / 目录。失败时 reject，原因为中文。 */
  select_workspace(path: string): Promise<WorkspaceStatus>;
  /** 在空目录初始化工作区与 Git 仓库。失败时 reject，原因为中文。 */
  init_workspace(path: string): Promise<WorkspaceStatus>;
  /**
   * 从远端克隆配置工作区。失败、取消或目标非空时 reject，原因为中文。
   * `token` 只在填写时提供：宿主把它存进**设备本地**目录，不写入工作区。
   */
  clone_workspace(
    url: string,
    target: string,
    token: { username: string; token: string } | null,
  ): Promise<CloneOutcome>;
  /** 最近一次克隆的进度快照；UI 轮询它显示进度。 */
  clone_progress(): Promise<CloneProgress>;
  /** 请求取消正在进行的克隆。 */
  cancel_clone(): Promise<void>;
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
  get_workspace: () => tauriInvoke("get_workspace"),
  select_workspace: (path) => tauriInvoke("select_workspace", { path }),
  init_workspace: (path) => tauriInvoke("init_workspace", { path }),
  clone_workspace: (url, target, token) =>
    tauriInvoke("clone_workspace", { url, target, token }),
  clone_progress: () => tauriInvoke("clone_progress"),
  cancel_clone: () => tauriInvoke("cancel_clone"),
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

// 浏览器模拟宿主的工作区：只模拟 UI 需要区分的几种结果。
// 真实行为（校验、拒绝覆盖、真实 Git 仓库、真实文件）由
// `crates/flashcast-core/tests/workspace*.rs` 的集成测试验证。
const MOCK_REPO = "/home/user/.config/flashcast";
const MOCK_BROKEN_REPO = "/home/user/broken-repo";
const MOCK_NON_EMPTY_DIR = "/home/user/Documents";
// 克隆用的模拟地址（见 MockHost.clone_workspace）。
export const MOCK_CLONE_TARGET = "/home/user/flashcast-clone";
export const MOCK_CLONE_URL = "https://github.com/me/flashcast-config.git";
export const MOCK_CLONE_BAD_URL = "https://github.com/me/not-found-config.git";
export const MOCK_CLONE_SECRET_URL = "https://alice:sekret@github.com/me/flashcast-config.git";

const IDLE_CLONE_PROGRESS: CloneProgress = {
  phase: "idle",
  receivedObjects: 0,
  totalObjects: 0,
  indexedObjects: 0,
  receivedBytes: 0,
  checkoutPath: null,
  checkoutCompleted: 0,
  checkoutTotal: 0,
  checkoutNotified: 0,
  updates: 0,
  message: null,
};

/** 模拟宿主认得的快捷键写法：至少一个「+」，且只有字母、数字与「+」。 */
function looksLikeHotkey(value: string): boolean {
  return /^[A-Za-z0-9+]+$/.test(value.trim()) && value.includes("+");
}

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
  private workspace: WorkspaceStatus = {
    path: null,
    gitDir: null,
    valid: false,
    settingsFile: null,
    persisted: false,
    error: null,
    remote: null,
  };
  /** 最近一次请求启动的条目 id（含失败样例），供浏览器交互检查脚本断言「是否真的执行了」。 */
  lastLaunched: string | null = null;
  hidden = false;
  /** 模拟的克隆进度与取消请求。 */
  private cloneState: CloneProgress = IDLE_CLONE_PROGRESS;
  private cloneCancelled = false;
  /** 模拟设备本地保存过令牌的主机（令牌本身不出现在 UI 状态里）。 */
  storedTokenHost: string | null = null;

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

  async execute(itemId: string): Promise<ActionOutcome> {
    // 记录本次请求，浏览器交互检查脚本据此判断回车是否真的触发了执行。
    this.lastLaunched = itemId;
    const app = MOCK_APPS.find((candidate) => itemId === `app:${candidate.id}`);
    if (app?.failsToLaunch) {
      return {
        status: "failed",
        message: `无法启动「${app.title}」：浏览器模拟宿主中的失败样例`,
      };
    }
    return { status: "done", message: null };
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
    // 与宿主一致：无效设置被拒绝，保留上一次有效状态。
    if (!looksLikeHotkey(settings.hotkey)) {
      throw `快捷键无效：无法识别的写法「${settings.hotkey}」`;
    }
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

  async get_workspace(): Promise<WorkspaceStatus> {
    return this.workspace;
  }

  async select_workspace(path: string): Promise<WorkspaceStatus> {
    const target = path.trim();
    if (target.length === 0) {
      throw "目录不存在：路径为空";
    }
    if (target === MOCK_BROKEN_REPO) {
      throw "配置无效：settings.toml 解析失败：unknown field `quick_access_limit`";
    }
    if (target !== MOCK_REPO) {
      throw `目录不存在：${target}`;
    }
    return this.link(target);
  }

  async init_workspace(path: string): Promise<WorkspaceStatus> {
    const target = path.trim();
    if (target.length === 0) {
      throw "目标目录非空，已拒绝初始化以免覆盖已有文件：路径为空";
    }
    if (target === MOCK_NON_EMPTY_DIR) {
      throw `目标目录非空，已拒绝初始化以免覆盖已有文件：${target}`;
    }
    return this.link(target);
  }

  async clone_workspace(
    url: string,
    target: string,
    token: { username: string; token: string } | null,
  ): Promise<CloneOutcome> {
    const address = url.trim();
    const destination = target.trim();
    // 与宿主同序：地址校验 → 目标目录校验 → 传输 / 检出 → 校验并关联。
    if (address.length === 0) {
      throw "克隆地址不能为空";
    }
    if (/^[a-z][a-z0-9+.-]*:\/\/[^/@\s]*:[^/@\s]*@/i.test(address)) {
      throw "克隆地址包含密码，已拒绝：请改用访问令牌（保存在本机设备目录）或系统 git 的凭证管理，不要把口令写在地址里";
    }
    if (destination.length === 0) {
      throw "目录不存在：路径为空";
    }
    if (destination === MOCK_NON_EMPTY_DIR) {
      throw `目标目录非空，已拒绝克隆以免覆盖已有文件：${destination}`;
    }
    if (token && token.token.trim().length > 0) {
      // 与宿主一致：令牌只写进设备本地目录，不进入工作区。
      this.storedTokenHost = "github.com";
    }

    this.cloneCancelled = false;
    const stages: CloneProgress[] = [
      { ...IDLE_CLONE_PROGRESS, phase: "connecting", updates: 1 },
      { ...IDLE_CLONE_PROGRESS, phase: "receiving", receivedObjects: 42, totalObjects: 100, indexedObjects: 30, receivedBytes: 65536, updates: 8 },
      { ...IDLE_CLONE_PROGRESS, phase: "resolving", receivedObjects: 100, totalObjects: 100, indexedObjects: 100, receivedBytes: 131072, updates: 12 },
      { ...IDLE_CLONE_PROGRESS, phase: "checkingOut", receivedObjects: 100, totalObjects: 100, indexedObjects: 100, receivedBytes: 131072, checkoutTotal: 4, checkoutCompleted: 2, checkoutNotified: 3, checkoutPath: "settings.toml", updates: 14 },
    ];
    for (const stage of stages) {
      if (this.cloneCancelled) {
        this.cloneState = { ...IDLE_CLONE_PROGRESS, phase: "cancelled", updates: stage.updates, message: "克隆已取消，未留下任何目录" };
        throw "克隆已取消，未留下任何目录";
      }
      this.cloneState = stage;
      await new Promise((resolve) => setTimeout(resolve, 120));
    }
    if (address === MOCK_CLONE_BAD_URL) {
      const message = "克隆失败：Could not resolve host: github.com（请检查网络、代理与远端地址是否正确）";
      this.cloneState = { ...IDLE_CLONE_PROGRESS, phase: "failed", updates: 15, message };
      throw message;
    }
    const remote = {
      name: "origin",
      url: address,
      branch: "main",
      upstream: "origin/main",
    };
    const status = this.link(destination, remote);
    this.cloneState = {
      ...IDLE_CLONE_PROGRESS,
      phase: "done",
      receivedObjects: 100,
      totalObjects: 100,
      indexedObjects: 100,
      receivedBytes: 131072,
      checkoutTotal: 4,
      checkoutCompleted: 4,
      checkoutNotified: 4,
      updates: 16,
      message: "克隆完成",
    };
    return {
      workspace: status,
      remote,
      unavailablePlugins: ["unavailable-plugin"],
      recordedTheme: "dark",
    };
  }

  async clone_progress(): Promise<CloneProgress> {
    return this.cloneState;
  }

  async cancel_clone(): Promise<void> {
    this.cloneCancelled = true;
  }

  private link(path: string, remote: WorkspaceStatus["remote"] = null): WorkspaceStatus {
    this.workspace = {
      path,
      gitDir: `${path}/.git`,
      valid: true,
      settingsFile: `${path}/settings.toml`,
      persisted: true,
      error: null,
      remote,
    };
    return this.workspace;
  }

  /**
   * 模拟外部编辑设置文件后宿主重载：有效则立即生效，无效则保留上次有效设置
   * 并给出原因。与 `Host::reload_from_workspace` 的语义一致。
   */
  simulateExternalEdit(hotkey: string): void {
    const reload = {
      path: `${this.workspace.path ?? MOCK_REPO}/settings.toml`,
      applied: false,
      settings: this.settings,
      error: null as string | null,
    };
    if (!looksLikeHotkey(hotkey)) {
      reload.error = `配置无效：settings.toml 内容不合法：快捷键无效：无法识别的主键：${hotkey}`;
      // 与宿主一致：配置无效时工作区标记为不可用，但保留上次有效设置。
      this.workspace = { ...this.workspace, valid: false, error: reload.error };
    } else if (hotkey === this.settings.hotkey) {
      // 幂等：内容没变就不做任何事。
    } else {
      this.settings = { ...this.settings, hotkey };
      reload.applied = true;
      reload.settings = this.settings;
      this.workspace = { ...this.workspace, valid: true, error: null };
    }
    const payload: WorkspaceEvent = {
      status: this.workspace,
      settings: this.settings,
      reload,
    };
    this.emit("flashcast://workspace", payload);
    if (reload.applied) {
      // 与外壳一致：外部改了快捷键后重新注册，并推送注册结果。
      this.emit("flashcast://hotkey-status", {
        label: this.settings.hotkey,
        error: null,
        registered: true,
      });
    }
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
