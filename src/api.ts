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
  ChangedFile,
  CloneOutcome,
  CloneProgress,
  CommitOutcome,
  PullOutcome,
  PushOutcome,
  QueryView,
  Settings,
  StatusView,
  SyncBlock,
  SyncProgress,
  SyncStatus,
  UnlistenFn,
  WorkspaceChanges,
  WorkspaceEvent,
  WorkspaceRemote,
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
  /** 当前工作区的 Git 变更：状态分类、分支与逐文件真实差异。 */
  get_git_changes(): Promise<WorkspaceChanges>;
  /** 创建提交；提交范围只包含显式传入的路径。失败时 reject，原因为中文。 */
  commit_changes(message: string, paths: string[]): Promise<CommitOutcome>;
  /** 当前工作区的同步状态：分支、远端、领先 / 落后、阻塞原因与指引（只读）。 */
  get_sync_status(): Promise<SyncStatus>;
  /** 重新检测同步状态：用户在应用外部处理完阻塞后调用。 */
  redetect_sync_state(): Promise<SyncStatus>;
  /** 仅快进拉取。阻塞或失败时 reject，原因为中文。 */
  pull_workspace(): Promise<PullOutcome>;
  /** 显式推送当前分支到上游（永不 force）。失败时 reject，原因为中文。 */
  push_workspace(): Promise<PushOutcome>;
  /** 最近一次同步的进度快照；UI 轮询它显示进度。 */
  sync_progress(): Promise<SyncProgress>;
  /** 请求取消正在进行的同步。 */
  cancel_sync(): Promise<void>;
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
  get_git_changes: () => tauriInvoke("get_git_changes"),
  commit_changes: (message, paths) => tauriInvoke("commit_changes", { message, paths }),
  get_sync_status: () => tauriInvoke("get_sync_status"),
  redetect_sync_state: () => tauriInvoke("redetect_sync_state"),
  pull_workspace: () => tauriInvoke("pull_workspace"),
  push_workspace: () => tauriInvoke("push_workspace"),
  sync_progress: () => tauriInvoke("sync_progress"),
  cancel_sync: () => tauriInvoke("cancel_sync"),
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

/**
 * 模拟工作区的远端关系：已克隆的工作区才有远端记录，这里用它让同步区段
 * 展示出分支、远端与上游（真实关系由宿主的设备本地表与仓库配置决定）。
 */
const MOCK_SYNC_REMOTE: WorkspaceRemote = {
  name: "origin",
  url: MOCK_CLONE_URL,
  branch: "main",
  upstream: "origin/main",
};

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

const IDLE_SYNC_PROGRESS: SyncProgress = {
  phase: "idle",
  receivedObjects: 0,
  totalObjects: 0,
  receivedBytes: 0,
  updates: 0,
  message: null,
};

/**
 * 浏览器模拟宿主可切换的同步场景。真实判定（脏工作区、分叉、冲突、
 * 进行中操作、鉴权、离线）全部在 `flashcast-core::sync`，
 * 由 `crates/flashcast-core/tests/workspace_sync.rs` 在真实仓库上验证。
 *
 * `auth` / `offline` 只影响**操作结果**，不出现在状态里：状态查询是纯本地的，
 * 与真实宿主一样，只有真的发起网络操作才可能遇到鉴权或网络失败。
 */
type MockSyncScenario =
  | "ready"
  | "dirty"
  | "diverged"
  | "conflicts"
  | "inProgress"
  | "auth"
  | "offline";

/** 与宿主一致的中文指引（节选），用于浏览器检查断言「给出了可操作的下一步」。 */
const MOCK_SYNC_BLOCKS: Record<Exclude<MockSyncScenario, "ready" | "auth" | "offline">, SyncBlock> = {
  dirty: {
    code: "dirtyWorktree",
    label: "有未提交修改",
    detail: "已暂存改动 + 未跟踪文件",
    hint: "在「变更与提交」区段提交这些改动（或在外部 git commit / git stash），然后重新检测。处理完成后回到本应用点击「重新检测」即可恢复同步。",
  },
  diverged: {
    code: "diverged",
    label: "历史已分叉",
    detail: "本地领先 1、落后 1",
    hint: "本地与远端都有对方没有的提交，需要人工合并：在外部执行 git pull --no-rebase（或 git fetch 后 git merge / git rebase）解决，首版不提供内置三方合并编辑器，也绝不强推覆盖对方。处理完成后回到本应用点击「重新检测」即可恢复同步。",
  },
  conflicts: {
    code: "conflicts",
    label: "存在合并冲突",
    detail: "memos/2026-10-01.md",
    hint: "在外部编辑冲突文件并 git add，然后 git commit（合并）或 git merge --abort / git rebase --abort 放弃本次操作。处理完成后回到本应用点击「重新检测」即可恢复同步。",
  },
  inProgress: {
    code: "operationInProgress",
    label: "Git 操作进行中",
    detail: "工作区正在进行变基（rebase），请先在外部完成或中止它，再创建提交",
    hint: "先在外部完成（git commit）或中止（git merge --abort、git rebase --abort、git cherry-pick --abort）这个操作。处理完成后回到本应用点击「重新检测」即可恢复同步。",
  },
};

const MOCK_AUTH_ERROR =
  "鉴权失败：unexpected http status code: 401（鉴权失败：https 请在设置中为该主机填写访问令牌，或确认系统 git 的凭证 helper 可用）";
const MOCK_OFFLINE_ERROR =
  "网络不可用：Could not resolve host: github.com（请检查网络、代理与远端地址是否正确）";

/**
 * 浏览器模拟宿主里的工作区变更样例：覆盖 UI 需要区分的状态
 * （未暂存 / 已暂存 + 未暂存 / 未跟踪）与真实差异文本。
 * 真实的 Git 行为由 `crates/flashcast-core/tests/workspace_git.rs` 在真实仓库上验证。
 */
const MOCK_CHANGES: ChangedFile[] = [
  {
    path: "settings.toml",
    code: " M",
    statusLabel: "未暂存修改",
    staged: false,
    unstaged: true,
    untracked: false,
    conflicted: false,
    diff: [
      "diff --git a/settings.toml b/settings.toml",
      "index 1f2c3ab..9d4e5f6 100644",
      "--- a/settings.toml",
      "+++ b/settings.toml",
      "@@ -1,2 +1,2 @@",
      '-hotkey = "Ctrl+Alt+Space"',
      '+hotkey = "Super+Space"',
      " quickAccessLimit = 6",
      "",
    ].join("\n"),
    diffTruncated: false,
  },
  {
    path: "theme.json",
    code: "MM",
    statusLabel: "已暂存修改 + 未暂存修改",
    staged: true,
    unstaged: true,
    untracked: false,
    conflicted: false,
    diff: [
      "diff --git a/theme.json b/theme.json",
      "index 3401802..2735556 100644",
      "--- a/theme.json",
      "+++ b/theme.json",
      "@@ -1 +1 @@",
      '-{"theme":"dark"}',
      '+{"theme":"dark","font":"serif"}',
      "",
    ].join("\n"),
    diffTruncated: false,
  },
  {
    path: "memos/2026-10-01.md",
    code: "??",
    statusLabel: "未跟踪（新文件）",
    staged: false,
    unstaged: false,
    untracked: true,
    conflicted: false,
    diff: [
      "diff --git a/memos/2026-10-01.md b/memos/2026-10-01.md",
      "new file mode 100644",
      "--- /dev/null",
      "+++ b/memos/2026-10-01.md",
      "@@ -0,0 +1 @@",
      "+备忘录内容：今天做的事",
      "",
    ].join("\n"),
    diffTruncated: false,
  },
];

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

  // ---- Git 变更与提交（浏览器模拟） ----
  private gitChanges: ChangedFile[] = MOCK_CHANGES.map((file) => ({ ...file }));
  private commitCount = 0;
  /** 模拟一次提交失败的中文原因；`null` 表示提交会成功。 */
  private commitError: string | null = null;
  private gitUnavailable = false;

  // ---- 同步（浏览器模拟） ----
  private syncScenario: MockSyncScenario = "ready";
  private syncAhead = 0;
  private syncBehind = 0;
  private syncBusy = false;
  private syncProgress: SyncProgress = IDLE_SYNC_PROGRESS;
  /** 远端记录的主题与备忘录；拉取时按远端内容重新加载。 */
  private theme: string | null = "dark";
  private memos = ["memos/hello.md"];
  /** 远端待拉取的内容（设置快捷键、主题、新增备忘录）。 */
  private incoming: { hotkey: string; theme: string; memo: string } | null = null;

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
    // 切换工作区后同步状态重新建立（真实宿主也会按新仓库重新探测）。
    this.syncScenario = "ready";
    this.syncAhead = 0;
    this.syncBehind = 0;
    this.incoming = null;
    this.syncProgress = IDLE_SYNC_PROGRESS;
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

  async get_git_changes(): Promise<WorkspaceChanges> {
    if (this.workspace.path === null || this.gitUnavailable) {
      return {
        repository: false,
        branch: null,
        detached: false,
        hasChanges: false,
        diffBase: "",
        files: [],
        state: null,
        error: this.gitUnavailable
          ? `当前工作区不是 Git 仓库，无法提交：${this.workspace.path ?? MOCK_REPO}`
          : null,
      };
    }
    return this.changes();
  }

  async commit_changes(message: string, paths: string[]): Promise<CommitOutcome> {
    if (this.workspace.path === null) {
      throw "尚未关联配置工作区，无法执行 Git 操作";
    }
    if (this.commitError) {
      throw this.commitError;
    }
    if (message.trim().length === 0) {
      throw "提交说明不能为空";
    }
    if (paths.length === 0) {
      throw "没有选择要提交的文件：提交范围只包含你显式勾选的路径";
    }
    const known = new Set(this.gitChanges.map((file) => file.path));
    for (const path of paths) {
      if (!known.has(path)) {
        throw `所选路径不在当前变更列表中，已拒绝提交：${path}`;
      }
    }
    if (this.gitChanges.length === 0) {
      throw "没有可提交的变更";
    }
    this.commitCount += 1;
    const short = ["3f9c1a2", "8b2d4e1", "c4a70f9"][(this.commitCount - 1) % 3];
    // 与宿主一致：提交后这些路径从变更列表消失（未勾选的改动仍然保留）。
    this.gitChanges = this.gitChanges.filter((file) => !paths.includes(file.path));
    return {
      oid: `${short}${"0".repeat(33)}`,
      short,
      message: message.trim(),
      authorName: "Flashcast 模拟身份",
      authorEmail: "mock@example.invalid",
      paths: [...paths],
      changes: this.changes(),
    };
  }

  private changes(): WorkspaceChanges {
    return {
      repository: true,
      branch: "main",
      detached: false,
      hasChanges: this.gitChanges.length > 0,
      diffBase: "HEAD (3f9c1a2) → 工作区（含已暂存改动）",
      files: this.gitChanges.map((file) => ({ ...file })),
      state: null,
      error: null,
    };
  }

  /** 让随后的提交以给定的中文原因失败，用于检查错误反馈（一次设定，显式清除）。 */
  simulateCommitError(kind: "identity" | "locked" | "abnormal" | "detached" | "nothing"): void {
    this.commitError = {
      identity: "Git 用户身份未配置，请先设置 user.name 与 user.email",
      locked: `Git 索引被占用（${MOCK_REPO}/.git/index.lock），可能有其它 Git 操作正在进行，请稍后重试`,
      abnormal: "工作区正在进行合并（merge），请先在外部完成或中止它，再创建提交",
      detached:
        "工作区处于分离 HEAD 状态（HEAD 未指向任何分支），无法在分支上创建提交；请先在外部切换回分支",
      nothing: "没有可提交的变更",
    }[kind];
  }

  clearCommitError(): void {
    this.commitError = null;
  }

  /** 模拟工作区不是 Git 仓库。 */
  simulateGitUnavailable(unavailable: boolean): void {
    this.gitUnavailable = unavailable;
  }

  // ---- 同步（浏览器模拟，语义与 flashcast_core::sync 对齐） ----

  /** 切换同步场景。 */
  simulateSyncScenario(scenario: MockSyncScenario): void {
    this.syncScenario = scenario;
  }

  /**
   * 模拟远端新增一次提交：下一次拉取会快进，并把有效设置、主题与备忘录
   * 「重新加载」成远端的内容（与宿主拉取后的行为一致）。
   */
  simulateRemoteCommit(hotkey = "Alt+Space"): void {
    this.syncBehind = 1;
    this.incoming = { hotkey, theme: "solarized", memo: "memos/remote-note.md" };
  }

  /** 模拟本地新增一次提交：下一次推送会把远端分支推到新提交。 */
  simulateLocalCommit(): void {
    this.syncAhead = 1;
  }

  /** 当前场景对应的拉取阻塞原因；鉴权与离线只有发起操作才会遇到。 */
  private syncBlock(): SyncBlock | null {
    if (this.workspace.path === null) {
      return {
        code: "noWorkspace",
        label: "尚未关联工作区",
        detail: "尚未关联配置工作区",
        hint: "先在「配置工作区」区段选择或克隆一个配置工作区。",
      };
    }
    if (this.syncScenario in MOCK_SYNC_BLOCKS) {
      return MOCK_SYNC_BLOCKS[this.syncScenario as keyof typeof MOCK_SYNC_BLOCKS];
    }
    return null;
  }

  private syncStatusSnapshot(): SyncStatus {
    const blocking = this.syncBlock();
    // 推送只搬运已提交对象：未提交修改不阻塞推送。
    const pushBlocking = blocking?.code === "dirtyWorktree" ? null : blocking;
    const linked = this.workspace.path !== null;
    const remote = this.workspace.remote ?? (linked ? MOCK_SYNC_REMOTE : null);
    // 分叉意味着两边各有对方没有的提交。
    const diverged = this.syncScenario === "diverged";
    const ahead = diverged ? 1 : this.syncAhead;
    const behind = diverged ? 1 : this.syncBehind;
    return {
      repository: linked,
      branch: linked ? "main" : null,
      detached: false,
      remote,
      remoteName: remote?.name ?? null,
      remoteUrl: remote?.url ?? null,
      upstream: remote?.upstream ?? null,
      ahead,
      behind,
      tracking: linked,
      dirty: this.syncScenario === "dirty",
      staged: this.syncScenario === "dirty",
      unstaged: this.syncScenario === "conflicts",
      untracked: this.syncScenario === "dirty",
      conflicted: this.syncScenario === "conflicts",
      state:
        this.syncScenario === "inProgress" ? MOCK_SYNC_BLOCKS.inProgress.detail : null,
      canPull: blocking === null,
      canPush: pushBlocking === null,
      nothingToPush: pushBlocking === null && ahead === 0,
      blocking,
      pushBlocking,
      busy: this.syncBusy,
      error: null,
    };
  }

  async get_sync_status(): Promise<SyncStatus> {
    return this.syncStatusSnapshot();
  }

  async redetect_sync_state(): Promise<SyncStatus> {
    return this.syncStatusSnapshot();
  }

  async pull_workspace(): Promise<PullOutcome> {
    if (this.workspace.path === null) {
      throw "尚未关联配置工作区，无法同步";
    }
    const blocking = this.syncBlock();
    if (blocking) {
      throw `${blocking.label}：${blocking.detail}。${blocking.hint}`;
    }
    if (this.syncScenario === "auth") {
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "failed", message: MOCK_AUTH_ERROR };
      throw MOCK_AUTH_ERROR;
    }
    if (this.syncScenario === "offline") {
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "failed", message: MOCK_OFFLINE_ERROR };
      throw MOCK_OFFLINE_ERROR;
    }

    this.syncBusy = true;
    try {
      for (const stage of [
        { phase: "fetching" as const, updates: 3, receivedObjects: 12, totalObjects: 40 },
        { phase: "fetching" as const, updates: 7, receivedObjects: 40, totalObjects: 40 },
      ]) {
        this.syncProgress = {
          ...IDLE_SYNC_PROGRESS,
          phase: stage.phase,
          updates: stage.updates,
          receivedObjects: stage.receivedObjects,
          totalObjects: stage.totalObjects,
        };
        await new Promise((resolve) => setTimeout(resolve, 160));
      }
      const path = `${this.workspace.path}/settings.toml`;
      if (this.syncBehind > 0) {
        this.syncBehind = 0;
        this.syncAhead = 0;
        const incoming = this.incoming;
        if (incoming) {
          // 与宿主一致：拉取后重新加载生效设置，并重新读取主题与备忘录。
          this.settings = { ...this.settings, hotkey: incoming.hotkey };
          this.theme = incoming.theme;
          this.memos = [...this.memos, incoming.memo];
        }
        this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "done", updates: 8, message: "拉取完成" };
        return {
          result: {
            kind: "fastForwarded",
            from: "3f9c1a2e0f9c4b1e8a7d6c5b4a39281706f5e4d3",
            to: "8b2d4e1c9a7f3b5d2e6c8a0f4b7d9e1c3a5f7024",
            shortTo: "8b2d4e1",
            commits: 1,
          },
          status: this.syncStatusSnapshot(),
          reload: { path, applied: this.incoming !== null, settings: this.settings, error: null },
          theme: this.theme,
          memos: this.memos,
          message: "已快进拉取到 8b2d4e1，共 1 个提交",
        };
      }
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "done", updates: 8, message: "已是最新" };
      return {
        result: { kind: "upToDate" },
        status: this.syncStatusSnapshot(),
        reload: { path, applied: false, settings: this.settings, error: null },
        theme: this.theme,
        memos: this.memos,
        message: "远端没有新的提交，本地已是最新",
      };
    } finally {
      this.syncBusy = false;
    }
  }

  async push_workspace(): Promise<PushOutcome> {
    if (this.workspace.path === null) {
      throw "尚未关联配置工作区，无法同步";
    }
    const blocking = this.syncBlock();
    // 与宿主一致：未提交修改不阻塞推送。
    if (blocking && blocking.code !== "dirtyWorktree") {
      throw `${blocking.label}：${blocking.detail}。${blocking.hint}`;
    }
    if (this.syncScenario === "auth") {
      throw MOCK_AUTH_ERROR;
    }
    if (this.syncScenario === "offline") {
      throw MOCK_OFFLINE_ERROR;
    }
    if (this.syncAhead === 0) {
      throw "没有需要推送的提交：本地与远端已经一致";
    }

    this.syncBusy = true;
    try {
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "pushing", updates: 2 };
      await new Promise((resolve) => setTimeout(resolve, 240));
      this.syncAhead = 0;
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "done", updates: 4, message: "推送完成" };
      return {
        branch: "main",
        remote: "origin",
        updated: [
          {
            local: "refs/heads/main",
            remote: "refs/heads/main",
            localOid: "8b2d4e1c9a7f3b5d2e6c8a0f4b7d9e1c3a5f7024",
            remoteOid: "3f9c1a2e0f9c4b1e8a7d6c5b4a39281706f5e4d3",
          },
        ],
        status: this.syncStatusSnapshot(),
        message: "已推送到 origin/main（1 个引用）",
      };
    } finally {
      this.syncBusy = false;
    }
  }

  async sync_progress(): Promise<SyncProgress> {
    return this.syncProgress;
  }

  async cancel_sync(): Promise<void> {
    this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "cancelled", message: "同步已取消，未改动任何内容" };
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
