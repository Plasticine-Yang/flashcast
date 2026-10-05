// 与宿主交互的唯一入口。
//
// 在 Tauri 中运行时走 `invoke`；在普通浏览器中（UI 浏览器交互检查）走本文件内的
// 确定性模拟宿主。模拟宿主实现与 `flashcast_core::Host` 相同的语义：
// seq 单调递增、同一输入重复渲染保留键盘选择、空查询给快速访问项、
// 启动失败给出中文反馈。**模拟宿主只用于开发与浏览器检查，不是产品行为。**

import type {
  ActionOutcome,
  Appearance,
  BackView,
  BookmarkEntry,
  BookmarksStatus,
  Capabilities,
  ChangedFile,
  ChromeProfileView,
  ChromeState,
  ClipboardEntryView,
  ClipboardFileView,
  ClipboardStateView,
  CloneOutcome,
  CloneProgress,
  CommitOutcome,
  Memo,
  MemoProblem,
  PluginView,
  Preview,
  PullOutcome,
  PushOutcome,
  QueryScope,
  QueryView,
  Settings,
  StatusView,
  SyncBlock,
  SyncProgress,
  SyncStatus,
  ThemeState,
  UnlistenFn,
  WorkspaceChanges,
  WorkspaceEvent,
  WorkspaceRemote,
  WorkspaceStatus,
} from "./types";
import { MOCK_INSTALLED_THEME, MOCK_THEMES, buildMockThemeState } from "./mockThemes";

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
  set_settings(settings: Settings): Promise<StatusView["hotkey"]>;
  get_status(): Promise<StatusView>;
  /** 当前主题状态：选中主题、CSS 自定义属性与可选主题列表。 */
  get_theme(): Promise<ThemeState>;
  /** 选择主题。失败时 reject，原因为中文，且当前外观不变。 */
  select_theme(id: string): Promise<ThemeState>;
  set_appearance_preferences(appearance: import("./types").ThemeAppearance, style: string, reduceTransparency: boolean): Promise<ThemeState>;
  sync_window_material(): Promise<{ supported: boolean; reason: string | null }>;
  /** 启用或停用插件（功能插件与主题插件共用）。 */
  set_plugin_enabled(id: string, enabled: boolean): Promise<ThemeState>;
  /** 校验并安装一个本地主题包（目录或 JSON 文件）。 */
  install_theme(path: string): Promise<ThemeState>;
  /** 移除一个已安装的本地主题包；内置主题会被拒绝。 */
  remove_theme(id: string): Promise<ThemeState>;
  /** 上报当前系统外观，「跟随系统」的主题据此在运行时切换。 */
  set_system_appearance(appearance: Appearance): Promise<ThemeState>;
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
  /** 启用或停用**功能插件**；返回更新后的功能插件列表（含启用状态）。 */
  set_feature_plugin_enabled(id: string, enabled: boolean): Promise<PluginView[]>;
  /** 当前生效的备忘录（按标识排序）。 */
  memos(): Promise<Memo[]>;
  /** 无法读取的备忘录文件：保留可用内容并如实报告原因。 */
  memo_problems(): Promise<MemoProblem[]>;
  /** 新建备忘录。未关联工作区或插件停用时 reject，原因为中文。 */
  create_memo(title: string, tags: string[], body: string): Promise<Memo>;
  /** 修改一条备忘录（标识不变）。失败时 reject，原因为中文。 */
  update_memo(id: string, title: string, tags: string[], body: string): Promise<Memo>;
  /** 删除一条备忘录（同时删除工作区里的文件）。 */
  delete_memo(id: string): Promise<void>;
  /** 预览某条结果；未知 id 返回 null。 */
  preview(itemId: string): Promise<Preview | null>;
  /** 剪贴板历史状态：启用、暂停、后台捕获、存储、容量与条目列表。 */
  get_clipboard_state(): Promise<ClipboardStateView>;
  /** 暂停 / 恢复记录。 */
  set_clipboard_paused(paused: boolean): Promise<ClipboardStateView>;
  /** 设置保留期限（天）与容量（条目数），并立即回收超出的条目。 */
  set_clipboard_limits(retentionDays: number, capacity: number): Promise<ClipboardStateView>;
  /** 置顶 / 取消置顶一条历史。 */
  pin_clipboard_entry(id: string, pinned: boolean): Promise<ClipboardStateView>;
  /** 删除一条历史。 */
  delete_clipboard_entry(id: string): Promise<ClipboardStateView>;
  /** 清空历史（置顶条目也会被清掉）。 */
  clear_clipboard_history(): Promise<ClipboardStateView>;
  /**
   * 用户**显式**为一个原文件保存受容量限制的本机副本。
   *
   * 失败时 reject，原因由宿主给出（原文件失效、访问失败、超过单份或总容量、
   * 复制中断、不支持的类型），UI 原样展示。
   */
  save_clipboard_file_copy(id: string, attachmentId: string): Promise<ClipboardStateView>;
  /** 当前 Chrome 状态：发现结果、profile 列表、关联状态与书签索引状态。 */
  get_chrome_state(): Promise<ChromeState>;
  /** 关联一个已发现的 Chrome profile（参数是目录名）。失败时 reject，原因为中文。 */
  associate_chrome_profile(profileDir: string): Promise<ChromeState>;
  /** 显式重新读取书签文件（外部改动后的兜底入口）。 */
  refresh_chrome_bookmarks(): Promise<ChromeState>;
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
  get_theme: () => tauriInvoke("get_theme"),
  select_theme: (id) => tauriInvoke("select_theme", { id }),
  set_appearance_preferences: (appearance, style, reduceTransparency) => tauriInvoke("set_appearance_preferences", { appearance, style, reduceTransparency }),
  sync_window_material: () => tauriInvoke("sync_window_material"),
  set_plugin_enabled: (id, enabled) =>
    tauriInvoke("set_plugin_enabled", { id, enabled }).then(() => tauriInvoke("get_theme")),
  install_theme: (path) => tauriInvoke("install_theme", { path }),
  remove_theme: (id) => tauriInvoke("remove_theme", { id }),
  set_system_appearance: (appearance) =>
    tauriInvoke("set_system_appearance", { appearance }),
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
  set_feature_plugin_enabled: (id, enabled) =>
    tauriInvoke("set_plugin_enabled", { id, enabled })
      .then(() => tauriInvoke<StatusView>("get_status"))
      .then((status) => status.plugins),
  memos: () => tauriInvoke("memos"),
  memo_problems: () => tauriInvoke("memo_problems"),
  create_memo: (title, tags, body) => tauriInvoke("create_memo", { title, tags, body }),
  update_memo: (id, title, tags, body) =>
    tauriInvoke("update_memo", { id, title, tags, body }),
  delete_memo: (id) => tauriInvoke("delete_memo", { id }),
  preview: (itemId) => tauriInvoke("preview", { itemId }),
  get_clipboard_state: () => tauriInvoke("get_clipboard_state"),
  set_clipboard_paused: (paused) => tauriInvoke("set_clipboard_paused", { paused }),
  set_clipboard_limits: (retentionDays, capacity) =>
    tauriInvoke("set_clipboard_limits", { retentionDays, capacity }),
  pin_clipboard_entry: (id, pinned) => tauriInvoke("pin_clipboard_entry", { id, pinned }),
  delete_clipboard_entry: (id) => tauriInvoke("delete_clipboard_entry", { id }),
  clear_clipboard_history: () => tauriInvoke("clear_clipboard_history"),
  save_clipboard_file_copy: (id, attachmentId) =>
    tauriInvoke("save_clipboard_file_copy", { id, attachmentId }),
  get_chrome_state: () => tauriInvoke("get_chrome_state"),
  associate_chrome_profile: (profileDir) =>
    tauriInvoke("associate_chrome_profile", { profileDir }),
  refresh_chrome_bookmarks: () => tauriInvoke("refresh_chrome_bookmarks"),
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

/** 备忘录插件在模拟宿主里的 id（与 flashcast-core 一致）。 */
export const MOCK_MEMO_PLUGIN_ID = "memo";

/** 书签索引状态的中文说明（与 flashcast_core::BookmarksStatus::label_zh 一致）。 */
function describeBookmarksStatus(status: BookmarksStatus): string {
  switch (status.kind) {
    case "notAssociated":
      return "尚未关联 Chrome profile";
    case "missing":
      return "该 profile 还没有书签文件（正常空状态）";
    case "ok":
      return `已索引 ${status.count} 条书签`;
    case "corrupt":
      return `书签文件无法解析：${status.reason}`;
    case "unreadable":
      return `书签文件无法读取：${status.reason}`;
  }
}

/** 匹配层级的排序权重（与 ADR §4 的稳定排序一致）。 */
const MATCH_TIER_ORDER: Record<string, number> = {
  keywordOrTagExact: 0,
  titlePrefix: 1,
  titleSubstring: 2,
  metadataSubstring: 3,
};

/** 备忘录插件的关键词别名（与 flashcast-core 的插件清单一致）。 */
export const MOCK_MEMO_KEYWORDS = ["备忘录", "memo", "memos"];

/**
 * 浏览器模拟宿主里的备忘录。语义与真实宿主一致：稳定标识、多个标签、文字正文；
 * 首屏按**完整标签**命中，进入插件范围后标题 / 标签 / 正文都可检索。
 * 真实行为（工作区 Markdown 文件、重启保留、外部修改）由
 * `crates/flashcast-core/tests/memos.rs` 在真实临时工作区上验证。
 */
export const MOCK_MEMOS: Memo[] = [
  {
    id: "memo-1",
    title: "常用回复",
    tags: ["回复", "工作"],
    body: "收到，我看一下再回复你。",
  },
  {
    id: "memo-2",
    title: "会议邀请",
    tags: ["会议", "工作"],
    body: "下午三点在三楼会议室，麻烦确认一下时间。",
  },
];

/** Chrome 书签插件在模拟宿主里的 id（与 flashcast-core 一致）。 */
export const MOCK_CHROME_PLUGIN_ID = "chrome-bookmarks";

export const MOCK_CHROME_KEYWORDS = ["chrome bookmarks", "chrome 书签"];

/**
 * 剪贴板历史插件在模拟宿主里的 id 与关键词别名（与 flashcast-core 一致）。
 *
 * 「剪贴板」与「剪切板」是**同一个**插件的两个输入别名：模拟宿主里也只有一份历史。
 */
/**
 * 浏览器模拟宿主里那一张「图片历史」的缩略图/预览 data URL。
 *
 * 真实外壳会读本机附件并编码（PNG）；这里内嵌一张 12×8 的真实 PNG，
 * 让 UI 走与生产完全相同的 `<img src="data:image/png;base64,…">` 路径。
 */
const MOCK_CLIPBOARD_IMAGE_DATA_URL =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAwAAAAICAYAAADN5B7xAAAAjklEQVR4nBXLIQFCMRQAQJKQgQCoZXgZpn6ABUAtw8swRYBJBGoZ1gNu4uTd7s/P70EhqDQ6yWCy2NzuIVAIKo1OMpgsdpxwCRSCSqOTDCaLfZ3wEigElUYnGUwW+3VCChSCSqOTDCaLnSe8BQpBpdFJBpPFfp/wFSgElUYnGUwW+3vCFigElUYnGUwWmz9ywhEQhoZgdAAAAABJRU5ErkJggg==";

export const MOCK_CLIPBOARD_PLUGIN_ID = "clipboard";
export const MOCK_CLIPBOARD_KEYWORDS = ["剪贴板", "剪切板", "clipboard"];

/**
 * 模拟历史里的初始条目（真实行为由 crates/flashcast-core/tests/clipboard.rs 验证）。
 *
 * `html` 是**本机历史里保存的富文本载荷**（对应真实存储的 `clipboard_payloads`）。
 * 真实宿主的 `ClipboardEntryView` **不**把它下发给界面（见 src-tauri 的视图构造），
 * 模拟宿主同样在 `clipboardStateView` 里剥掉它：界面只拿到可索引的纯文本，因此剪贴板
 * 提供的标记没有机会进入 DOM。`clipboard-rich-preview` 这条检查依赖这个事实。
 */
type MockClipboardEntry = ClipboardEntryView & {
  /** 仅用于模拟「本机保存了 HTML 载荷」；不下发给界面。 */
  html?: string | null;
};

const MOCK_CLIPBOARD_RICH_HTML =
  '<p><strong>会议纪要草稿</strong></p><script>alert("clipboard-xss")</script><img src="https://example.invalid/track.png" onerror="steal()">';

const MOCK_CLIPBOARD_ENTRIES: MockClipboardEntry[] = [
  {
    id: "clip-mock-1",
    summary: "会议纪要草稿 上午十点在三楼会议室",
    text: "会议纪要草稿\n上午十点在三楼会议室\n确认一下参加人",
    formats: ["文字", "HTML", "RTF"],
    html: MOCK_CLIPBOARD_RICH_HTML,
    source: "Firefox",
    capturedAtMs: Date.now() - 3 * 60 * 1000,
    pinned: false,
    copies: 1,
    attachments: 0,
    imageDataUrl: null,
    imageSize: null,
    files: [],
    references: 0,
    fileCopies: 0,
  },
  {
    id: "clip-mock-2",
    summary: "https://example.com/report",
    text: "https://example.com/report",
    formats: ["文字"],
    source: null,
    capturedAtMs: Date.now() - 40 * 60 * 1000,
    pinned: true,
    copies: 2,
    attachments: 0,
    imageDataUrl: null,
    imageSize: null,
    files: [],
    references: 0,
    fileCopies: 0,
  },
  {
    id: "clip-mock-3",
    summary: "图片 PNG 12×8（0.1 KB）",
    text: null,
    formats: ["图片"],
    source: "GIMP",
    capturedAtMs: Date.now() - 70 * 60 * 1000,
    pinned: false,
    copies: 1,
    attachments: 1,
    imageDataUrl: MOCK_CLIPBOARD_IMAGE_DATA_URL,
    imageSize: "PNG 12×8",
    files: [],
    references: 0,
    fileCopies: 0,
  },
  {
    // 多文件列表：含空格、非 ASCII 与一个视频文件（视频在 v0.1 按文件处理）。
    id: "clip-mock-4",
    summary: "3 个文件：报告 草稿.pdf、照片 一.png、视频 片段.mp4",
    text: null,
    formats: ["文件"],
    source: "文件管理器",
    capturedAtMs: Date.now() - 12 * 60 * 1000,
    pinned: false,
    copies: 1,
    attachments: 3,
    files: [
      {
        attachmentId: "att-mock-pdf",
        name: "报告 草稿.pdf",
        kind: { kind: "fileReference" },
        kindLabel: "引用",
        mime: "application/pdf",
        bytes: 18432,
        recoverable: true,
        problem: null,
      },
      {
        attachmentId: "att-mock-png",
        name: "照片 一.png",
        kind: { kind: "fileReference" },
        kindLabel: "引用",
        mime: "image/png",
        bytes: 65536,
        recoverable: true,
        problem: null,
      },
      {
        attachmentId: "att-mock-mp4",
        name: "视频 片段.mp4",
        kind: { kind: "fileReference" },
        kindLabel: "引用",
        mime: "video/mp4",
        bytes: 7340032,
        recoverable: true,
        problem: null,
      },
    ],
    references: 3,
    fileCopies: 0,
    imageDataUrl: null,
    imageSize: null,
  },
  {
    // 原文件已经被删除的引用：必须显示「不可恢复」而不是假装还能粘贴。
    id: "clip-mock-5",
    summary: "已归档 说明.txt",
    text: null,
    formats: ["文件"],
    source: null,
    capturedAtMs: Date.now() - 2 * 60 * 60 * 1000,
    pinned: false,
    copies: 1,
    attachments: 1,
    files: [
      {
        attachmentId: "att-mock-gone",
        name: "已归档 说明.txt",
        kind: { kind: "fileReference" },
        kindLabel: "引用",
        mime: "text/plain",
        bytes: 96,
        recoverable: false,
        problem: "原文件已不存在：/home/user/下载/已归档 说明.txt",
      },
    ],
    references: 1,
    fileCopies: 0,
    imageDataUrl: null,
    imageSize: null,
  },
];

/**
 * 浏览器模拟宿主里的 Chrome profile 与书签。语义与真实宿主一致：
 * 关联记录在设备本地、书签按标题 / 网址 / 目录检索、默认操作是在 Chrome 打开。
 * 真实行为（真实 Local State、Bookmarks 解析、变化后刷新、argv）由
 * `crates/flashcast-core/tests/chrome.rs` 与 `chrome_fixture.rs` 验证。
 */
const MOCK_CHROME_PROFILES: ChromeProfileView[] = [
  {
    dir: "Default",
    name: "个人",
    userName: "me@example.com",
    managed: false,
    hasBookmarks: true,
    bookmarksReadable: true,
    unreadableReason: null,
    associated: false,
  },
  {
    dir: "Profile 1",
    name: "工作",
    userName: "work@corp.example",
    managed: true,
    hasBookmarks: true,
    bookmarksReadable: true,
    unreadableReason: null,
    associated: true,
  },
];

const MOCK_BOOKMARKS: BookmarkEntry[] = [
  {
    id: "7",
    title: "Rust 官网",
    url: "https://www.rust-lang.org/",
    folder: "书签栏",
  },
  {
    id: "9",
    title: "Rust 文档",
    url: "https://doc.rust-lang.org/std/?q=中文&x=1",
    folder: "书签栏 / 开发",
  },
  {
    id: "10",
    title: "内网登录",
    url: "https://intranet.example.com/login?token=abc&next=首页",
    folder: "其他书签",
  },
  {
    id: "11",
    title: "分析工具",
    url: "https://rust-analyzer.github.io/",
    folder: "其他书签",
  },
];

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
  private history: { input: string; selection: number; scope: QueryScope }[] = [];
  /** 当前查询范围：首屏或某个功能插件的范围。 */
  private scope: QueryScope = { kind: "home" };
  private handlers = new Map<string, Set<Handler>>();
  private settings: Settings = {
    hotkey: "Ctrl+Alt+Space",
    launchAtStartup: false,
    quickAccessLimit: 6,
    pluginTimeoutMs: 400,
    disabledPlugins: [],
    // 与 flashcast_core::ClipboardSettings::default 一致。
    clipboard: { paused: false, retentionDays: 30, capacity: 500 },
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
  /** 最近一次复制进剪贴板的内容（备忘录的默认操作是复制）。 */
  lastCopied: string | null = null;
  /** 最近一次复制进剪贴板的**文件列表**（ticket 12 的恢复路径）。 */
  lastCopiedFiles: string[] | null = null;
  /**
   * 最近一次自动粘贴：内容、目标应用与外壳执行的步骤。
   *
   * 浏览器模拟宿主把「准备剪贴板 → 关闭浮窗 → 恢复目标应用 → 注入粘贴」按真实外壳
   * （`src-tauri/src/commands.rs::execute`）的顺序记录下来，交互检查据此断言顺序与内容，
   * 而**不是**断言「命令已发送」。真实桌面上的粘贴需要检查目标应用的内容，这里做不到。
   */
  lastPaste: { content: string; target: string; sequence: string[] } | null = null;
  /** 唤起前的前台应用（真实外壳在唤起时捕获；浏览器里是固定样例）。 */
  previousApp: { id: string; name: string } | null = { id: "code", name: "Visual Studio Code" };
  /** 本会话能否自动粘贴。浏览器默认**不能**（没有可注入按键的桌面会话）。 */
  private autoPasteSupported = false;
  hidden = false;
  /** 模拟的克隆进度与取消请求。 */
  private cloneState: CloneProgress = IDLE_CLONE_PROGRESS;
  private cloneCancelled = false;
  /** 模拟设备本地保存过令牌的主机（令牌本身不出现在 UI 状态里）。 */
  storedTokenHost: string | null = null;

  /** 浏览器模拟宿主认得的主题包路径。 */
  static readonly THEME_PACKAGE = "/home/user/themes/solarized";
  static readonly BROKEN_THEME_PACKAGE = "/home/user/themes/broken";
  private mockThemes = MOCK_THEMES.map((theme) => ({ ...theme }));
  private selectedTheme = "flashcast.theme.arc";
  private appearancePreference: import("./types").ThemeAppearance = "system";
  private themeStyles: Record<string, string> = {};
  private reduceTransparency = false;
  private systemAppearance: Appearance = "light";
  private themeError: string | null = null;

  private themeState(): ThemeState {
    return buildMockThemeState({
      themes: this.mockThemes,
      selected: this.selectedTheme,
      system: this.systemAppearance,
      error: this.themeError,
      preference: this.appearancePreference, styles: this.themeStyles, reduce: this.reduceTransparency,
    });
  }
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
  /** 工作区里的备忘录文件（仓库相对路径）；拉取时按远端内容重新读取。 */
  private memoFiles = ["memos/hello.md"];

  // ---- 备忘录（浏览器模拟） ----
  /** 生效的备忘录内容（与真实宿主一样按标识排序）。 */
  private memoEntries: Memo[] = MOCK_MEMOS.map((memo) => ({
    ...memo,
    tags: [...memo.tags],
  }));
  /** 备忘录插件的启用状态：停用后既不贡献结果也不接受写入。 */
  private memoPluginEnabled = true;
  /** 最近一次生成的模拟备忘录标识。 */
  private memoCounter = MOCK_MEMOS.length;
  /** 远端待拉取的内容（设置快捷键、主题 id、新增备忘录）。 */
  private incoming: { hotkey: string; theme: string; memo: string } | null = null;

  // ---- Chrome 书签（浏览器模拟） ----
  /** Chrome 书签插件的启用状态。 */
  private chromePluginEnabled = true;
  /** 模拟的 profile 列表（关联状态是设备本地数据）。 */
  private chromeProfiles: ChromeProfileView[] = MOCK_CHROME_PROFILES.map((profile) => ({
    ...profile,
  }));
  /** 模拟的书签索引状态：正常、缺失、损坏三种，覆盖 UI 需要区分的分支。 */
  private chromeStatus: BookmarksStatus = { kind: "ok", count: MOCK_BOOKMARKS.length };
  /** 最近一次交给「Chrome」的启动请求（浏览器交互检查据此断言参数向量）。 */
  lastChromeLaunch: { program: string; args: string[] } | null = null;
  /** 模拟 Chrome 是否可用，以及发现 / 关联的问题说明。 */
  private chromeAvailable = true;
  private chromeError: string | null = null;
  private chromeWarnings: string[] = [];
  /** 模拟用户数据目录是否在默认位置之外。 */
  private customChromeUserDataDir = false;

  // ---- 剪贴板历史（浏览器模拟） ----
  /**
   * 剪贴板历史插件的启用状态。默认**关闭**，与真实宿主一致：
   * 后台捕获用户复制的内容是隐私敏感行为，必须由用户显式启用。
   */
  private clipboardPluginEnabled = false;
  private clipboardEntries: MockClipboardEntry[] = MOCK_CLIPBOARD_ENTRIES.map((entry) => ({
    ...entry,
    formats: [...entry.formats],
    // 文件条目要深拷贝：保存副本会就地改写它，不能污染模块级样例。
    files: entry.files.map(
      (file): ClipboardFileView => ({ ...file, kind: { ...file.kind } }),
    ),
  }));
  /** 模拟的存储失败原因；非空时如实展示，而不是假装历史为空。 */
  private clipboardStorageError: string | null = null;
  /** 模拟的最近一次捕获失败原因。 */
  private clipboardLastError: string | null = null;
  /** 模拟的「保存本机副本」失败原因（超限 / 访问失败 / 复制中断）。 */
  private clipboardCopyError: string | null = null;
  /** 模拟的原文件已被删除：按名让引用变为不可恢复。 */
  private clipboardMissingFiles = new Set<string>();
  /** 被自身写入抑制丢弃的次数（诊断用）。 */
  private clipboardSuppressed = 0;

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

    // 插件范围：关键词完整匹配后进入。范围内的查询先剥掉关键词前缀，
    // 之后标题、标签与正文都可检索（与真实宿主一致）。
    if (this.scope.kind === "plugin") {
      const keyword = this.scope.keyword;
      const rest =
        query === keyword
          ? ""
          : query.startsWith(keyword)
            ? query.slice(keyword.length).trim()
            : query;
      if (this.scope.id === MOCK_CHROME_PLUGIN_ID) {
        return this.chromeItems(rest);
      }
      if (this.scope.id === MOCK_CLIPBOARD_PLUGIN_ID) {
        return this.clipboardItems(rest);
      }
      return this.memoItems(rest);
    }

    const commandItems: QueryView["items"] = [
      {
        id: "flashcast.command.rescan",
        title: "重新扫描软件",
        subtitle: "刷新已安装软件列表",
        iconDataUrl: null,
      thumbnailDataUrl: null,
        source: "flashcast",
        kind: "command",
        defaultAction: "open",
        defaultActionLabel: "执行",
        score: { tier: "titlePrefix", relevance: 0 },
      },
      {
        id: "flashcast.command.capabilities",
        title: "查看平台能力",
        subtitle: "显示会话类型与各能力的真实支持状态",
        iconDataUrl: null,
      thumbnailDataUrl: null,
        source: "flashcast",
        kind: "command",
        defaultAction: "open",
        defaultActionLabel: "执行",
        score: { tier: "titlePrefix", relevance: 0 },
      },
    ];
    if (query.length === 0) {
      const quick = MOCK_APPS.slice(0, this.settings.quickAccessLimit).map((app) =>
        this.toItem(app, { tier: "titlePrefix", relevance: 0 }),
      );
      return [...quick, ...commandItems];
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
    // 备忘录在首屏按**完整标签**命中（与宿主一致：标签精确匹配优先于较弱的匹配）。
    const memoScored = this.memoPluginEnabled
      ? this.memoEntries
          .filter((memo) => memo.tags.some((tag) => tag.toLowerCase() === query))
          .map((memo) => ({
            title: memo.title,
            tier: "keywordOrTagExact" as const,
            relevance: 90,
            item: this.memoItem(memo, { tier: "keywordOrTagExact" as const, relevance: 90 }),
          }))
      : [];
    // 关键词与标签冲突（ADR §4）：同时给出插件入口，且入口排在最前——直接回车的行为
    // 与「输入关键词进入范围」一致，标签命中的备忘录就在它下面，不会被静默丢弃。
    const collisionEntry =
      this.memoPluginEnabled && MOCK_MEMO_KEYWORDS.includes(query) && memoScored.length > 0
        ? {
            id: `flashcast.plugin.${MOCK_MEMO_PLUGIN_ID}`,
            title: "备忘录",
            subtitle: `插件 · 回车进入「${query}」范围`,
            iconDataUrl: null,
      thumbnailDataUrl: null,
            source: "flashcast",
            kind: "command" as const,
            defaultAction: "open" as const,
            defaultActionLabel: "打开",
            score: { tier: "keywordOrTagExact" as const, relevance: 255 },
          }
        : null;
    const ranked = [
      ...memoScored,
      ...scored.map(({ app, tier, relevance }) => ({
        title: app.title,
        tier,
        relevance,
        item: this.toItem(app, { tier, relevance }),
      })),
    ];
    ranked.sort(
      (a, b) =>
        tierOrder[a.tier] - tierOrder[b.tier] ||
        b.relevance - a.relevance ||
        a.title.localeCompare(b.title),
    );
    const items = ranked.map((entry) => entry.item);
    return collisionEntry ? [collisionEntry, ...items] : items;
  }

  /** 剪贴板历史范围内的一条结果：默认操作是粘贴（复用备忘录的粘贴路径）。 */
  private clipboardItem(
    entry: ClipboardEntryView,
    score: QueryView["items"][number]["score"],
  ): QueryView["items"][number] {
    const parts: string[] = [];
    if (entry.pinned) {
      parts.push("已置顶");
    }
    parts.push(...entry.formats);
    // 类型与尺寸（ticket 10）：图片没有可检索的文字，这是分辨条目的主要依据。
    if (entry.imageSize) {
      parts.push(entry.imageSize);
    }
    if (entry.files.length > 0) {
      // 与宿主 clipboard_subtitle 同口径：引用数与已保存副本数。
      let filePart = `${entry.references} 个引用`;
      if (entry.fileCopies > 0) {
        filePart += ` · ${entry.fileCopies} 个已保存副本`;
      }
      parts.push(filePart);
    }
    if (entry.source) {
      parts.push(`来自 ${entry.source}`);
    }
    if (entry.copies > 1) {
      parts.push(`复制过 ${entry.copies} 次`);
    }
    return {
      id: `clipboard:${entry.id}`,
      title: entry.summary,
      subtitle: parts.join(" · "),
      iconDataUrl: null,
      thumbnailDataUrl: entry.imageDataUrl ?? null,
      source: MOCK_CLIPBOARD_PLUGIN_ID,
      kind: "clipboardEntry",
      defaultAction: "paste",
      defaultActionLabel: "粘贴",
      score,
    };
  }

  /** 范围内的历史检索：文字与来源（`rest` 已剥掉关键词前缀）。 */
  private clipboardItems(rest: string): QueryView["items"] {
    if (!this.clipboardPluginEnabled) {
      return [];
    }
    // 存储失败时列表为空，原因由设置页与状态字段给出（与真实宿主一致，不伪造结果）。
    if (this.clipboardStorageError) {
      return [];
    }
    const query = rest.trim().toLowerCase();
    const ordered = [...this.clipboardEntries].sort(
      (a, b) =>
        Number(b.pinned) - Number(a.pinned) || b.capturedAtMs - a.capturedAtMs,
    );
    if (query.length === 0) {
      return ordered.map((entry) =>
        this.clipboardItem(entry, { tier: "titlePrefix", relevance: 0 }),
      );
    }
    return ordered
      .map((entry) => {
        const title = entry.summary.toLowerCase();
        const text = (entry.text ?? "").toLowerCase();
        if (title === query) return { entry, tier: "titlePrefix" as const, relevance: 100 };
        if (title.startsWith(query)) return { entry, tier: "titlePrefix" as const, relevance: 80 };
        if (title.includes(query)) return { entry, tier: "titleSubstring" as const, relevance: 55 };
        if (text.includes(query)) {
          return { entry, tier: "metadataSubstring" as const, relevance: 40 };
        }
        if ((entry.source ?? "").toLowerCase().includes(query)) {
          return { entry, tier: "metadataSubstring" as const, relevance: 30 };
        }
        // 文件列表没有可索引文字：名称、类型与种类标签是唯一的检索入口
        // （与宿主 metadata_for 同口径，spec「不承诺 OCR」）。
        const metadata = entry.files.flatMap((file) => [
          file.name.toLowerCase(),
          (file.mime ?? "").toLowerCase(),
          file.kindLabel.toLowerCase(),
        ]);
        if (metadata.some((value) => value.includes(query))) {
          return { entry, tier: "metadataSubstring" as const, relevance: 35 };
        }
        return null;
      })
      .filter((value): value is NonNullable<typeof value> => value !== null)
      .sort(
        (a, b) =>
          MATCH_TIER_ORDER[a.tier] - MATCH_TIER_ORDER[b.tier] ||
          b.relevance - a.relevance ||
          b.entry.capturedAtMs - a.entry.capturedAtMs,
      )
      .map(({ entry, tier, relevance }) => this.clipboardItem(entry, { tier, relevance }));
  }

  /** 当前剪贴板历史状态（含条目列表），与真实宿主的字段一一对应。 */
  clipboardStateView(): ClipboardStateView {
    const paused = this.settings.clipboard?.paused ?? false;
    return {
      enabled: this.clipboardPluginEnabled,
      paused,
      captureActive: this.clipboardPluginEnabled && !paused,
      storageOk: this.clipboardStorageError === null,
      storageError: this.clipboardStorageError,
      storagePath: "/home/user/.local/share/flashcast/clipboard/history.sqlite3",
      entries: this.clipboardEntries.length,
      pinned: this.clipboardEntries.filter((entry) => entry.pinned).length,
      attachments: this.clipboardEntries.reduce((sum, entry) => sum + entry.attachments, 0),
      capacity: this.settings.clipboard?.capacity ?? 500,
      retentionDays: this.settings.clipboard?.retentionDays ?? 30,
      capacityReached: null,
      lastError: this.clipboardLastError ?? this.clipboardStorageError,
      lastCaptureMs: this.clipboardEntries.length > 0 ? Date.now() : null,
      suppressed: this.clipboardSuppressed,
      items: [...this.clipboardEntries]
        .sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.capturedAtMs - a.capturedAtMs)
        // 与本机真实宿主一致：**不**把 HTML/RTF 载荷下发给界面，只给可索引纯文本与格式名。
        .map(({ html: _payload, ...entry }) => ({ ...entry, formats: [...entry.formats] })),
    };
  }

  /** 浏览器交互检查用的钩子：模拟本机存储失败。 */
  simulateClipboardStorageFailure(reason: string | null): void {
    this.clipboardStorageError = reason;
  }

  /** 浏览器交互检查用的钩子：模拟捕获失败（例如拿不到剪贴板选区）。 */
  simulateClipboardCaptureFailure(reason: string | null): void {
    this.clipboardLastError = reason;
  }

  /** 书签范围内的一条结果：默认操作是在 Chrome 打开。 */
  private chromeItem(
    entry: BookmarkEntry,
    score: QueryView["items"][number]["score"],
  ): QueryView["items"][number] {
    const subtitle = entry.folder.trim()
      ? `${entry.url} · 目录：${entry.folder}`
      : entry.url;
    return {
      id: `chrome-bookmark:${entry.id}`,
      title: entry.title,
      subtitle,
      iconDataUrl: null,
      thumbnailDataUrl: null,
      source: MOCK_CHROME_PLUGIN_ID,
      kind: "bookmark",
      defaultAction: "openInChrome",
      defaultActionLabel: "在 Chrome 打开",
      score,
    };
  }

  /** 范围内的书签检索：标题、网址、目录（`rest` 已剥掉关键词前缀）。 */
  private chromeItems(rest: string): QueryView["items"] {
    if (!this.chromePluginEnabled) {
      return [];
    }
    // 文件缺失 / 损坏 / Chrome 不可用时列表为空：原因由设置页与执行反馈给出，
    // 这里不伪造结果（与真实宿主一致）。
    if (this.chromeStatus.kind !== "ok") {
      return [];
    }
    const query = rest.trim().toLowerCase();
    if (query.length === 0) {
      return MOCK_BOOKMARKS.map((entry) =>
        this.chromeItem(entry, { tier: "titlePrefix", relevance: 0 }),
      );
    }
    return MOCK_BOOKMARKS.map((entry) => {
      const title = entry.title.toLowerCase();
      if (title === query) return { entry, tier: "titlePrefix" as const, relevance: 100 };
      if (title.startsWith(query)) return { entry, tier: "titlePrefix" as const, relevance: 80 };
      if (title.includes(query)) return { entry, tier: "titleSubstring" as const, relevance: 55 };
      if (entry.url.toLowerCase().includes(query)) {
        return { entry, tier: "metadataSubstring" as const, relevance: 35 };
      }
      if (entry.folder.toLowerCase().includes(query)) {
        return { entry, tier: "metadataSubstring" as const, relevance: 30 };
      }
      return null;
    })
      .filter((value): value is NonNullable<typeof value> => value !== null)
      .sort(
        (a, b) =>
          MATCH_TIER_ORDER[a.tier] - MATCH_TIER_ORDER[b.tier] ||
          b.relevance - a.relevance ||
          a.entry.id.localeCompare(b.entry.id),
      )
      .map(({ entry, tier, relevance }) => this.chromeItem(entry, { tier, relevance }));
  }

  /** 当前关联的 profile 目录名。 */
  private associatedChromeDir: string | null = "Profile 1";

  /** 当前 Chrome 状态（与真实宿主的字段一一对应）。 */
  private chromeState(): ChromeState {
    return {
      available: this.chromeAvailable,
      brandLabel: "Google Chrome",
      customUserDataDir: false,
      binary: "/usr/bin/google-chrome",
      userDataDir: "/home/user/.config/google-chrome",
      profiles: this.chromeProfiles.map((profile) => ({
        ...profile,
        associated: profile.dir === this.associatedChromeDir,
      })),
      associated: this.associatedChromeDir,
      associatedName:
        this.chromeProfiles.find((profile) => profile.dir === this.associatedChromeDir)?.name ??
        null,
      error: this.chromeError,
      warnings: this.chromeWarnings,
      bookmarks: {
        path:
          this.associatedChromeDir === null
            ? null
            : `/home/user/.config/google-chrome/${this.associatedChromeDir}/Bookmarks`,
        status: this.chromeStatus,
        entries: this.chromeStatus.kind === "ok" ? MOCK_BOOKMARKS.map((e) => ({ ...e })) : [],
      },
      bookmarksLabel: describeBookmarksStatus(this.chromeStatus),
    };
  }

  /** 浏览器交互检查用的钩子：模拟书签文件缺失 / 损坏 / Chrome 未安装。 */
  simulateChromeStatus(kind: "ok" | "missing" | "corrupt" | "unavailable"): void {
    if (kind === "ok") {
      this.chromeStatus = { kind: "ok", count: MOCK_BOOKMARKS.length };
      this.chromeError = null;
      this.chromeAvailable = true;
      return;
    }
    if (kind === "missing") {
      this.chromeStatus = { kind: "missing" };
      this.chromeError = null;
      return;
    }
    if (kind === "corrupt") {
      this.chromeStatus = {
        kind: "corrupt",
        reason: "JSON 解析失败：expected value at line 1 column 9",
      };
      return;
    }
    this.chromeAvailable = false;
    this.chromeError = "没有找到 Chrome 可执行文件；已尝试：/usr/bin/google-chrome";
  }

  /** 浏览器交互检查用的钩子：模拟书签文件被外部追加了一条书签。 */
  simulateChromeBookmarkAdded(): void {
    MOCK_BOOKMARKS.push({
      id: `sim-${MOCK_BOOKMARKS.length + 1}`,
      title: "新增书签",
      url: "https://new.example.com/",
      folder: "书签栏",
    });
    this.chromeStatus = { kind: "ok", count: MOCK_BOOKMARKS.length };
  }

  /** 备忘录范围内的一条结果：默认操作是粘贴（ticket 07 先复制并提示手动粘贴）。 */
  private memoItem(
    memo: Memo,
    score: QueryView["items"][number]["score"],
  ): QueryView["items"][number] {
    return {
      id: `memo:${memo.id}`,
      title: memo.title,
      subtitle: memo.tags.length > 0 ? `标签：${memo.tags.join("、")}` : "无标签",
      iconDataUrl: null,
      thumbnailDataUrl: null,
      source: MOCK_MEMO_PLUGIN_ID,
      kind: "memo",
      defaultAction: "paste",
      defaultActionLabel: "粘贴",
      score,
    };
  }

  /** 范围内的备忘录检索：标题、标签、正文（`query` 已剥掉关键词前缀）。 */
  private memoItems(rest: string): QueryView["items"] {
    if (!this.memoPluginEnabled) {
      return [];
    }
    const query = rest.trim().toLowerCase();
    if (query.length === 0) {
      return this.memoEntries.map((memo) =>
        this.memoItem(memo, { tier: "titlePrefix", relevance: 0 }),
      );
    }
    return this.memoEntries
      .map((memo) => {
        const title = memo.title.toLowerCase();
        if (title === query) return { memo, tier: "titlePrefix" as const, relevance: 100 };
        if (title.startsWith(query)) return { memo, tier: "titlePrefix" as const, relevance: 80 };
        if (title.includes(query)) return { memo, tier: "titleSubstring" as const, relevance: 55 };
        if (memo.tags.some((tag) => tag.toLowerCase().includes(query))) {
          return { memo, tier: "metadataSubstring" as const, relevance: 32 };
        }
        if (memo.body.toLowerCase().includes(query)) {
          return { memo, tier: "metadataSubstring" as const, relevance: 29 };
        }
        return null;
      })
      .filter((value): value is NonNullable<typeof value> => value !== null)
      .sort(
        (a, b) =>
          MATCH_TIER_ORDER[a.tier] - MATCH_TIER_ORDER[b.tier] ||
          b.relevance - a.relevance ||
          a.memo.title.localeCompare(b.memo.title),
      )
      .map(({ memo, tier, relevance }) => this.memoItem(memo, { tier, relevance }));
  }

  private toItem(app: MockApp, score: QueryView["items"][number]["score"]): QueryView["items"][number] {
    return {
      id: `app:${app.id}`,
      title: app.title,
      subtitle: app.subtitle,
      iconDataUrl: null,
      thumbnailDataUrl: null,
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
      scope: this.scope,
      scopeLabel: this.scope.kind === "plugin" ? `${this.scope.keyword} 范围` : "首屏",
      input: this.input,
      items: this.items,
      selection: this.selection,
      notice,
      pluginFailures: [],
    };
  }

  async query(input: string): Promise<QueryView> {
    const sameInput = this.input === input;
    const normalized = input.trim().toLowerCase();
    const isMemoKeyword =
      this.memoPluginEnabled && MOCK_MEMO_KEYWORDS.includes(normalized);
    const isChromeKeyword =
      this.chromePluginEnabled && MOCK_CHROME_KEYWORDS.includes(normalized);
    // 「剪贴板」与「剪切板」都进入同一个插件：两个别名共用一份历史。
    const isClipboardKeyword =
      this.clipboardPluginEnabled && MOCK_CLIPBOARD_KEYWORDS.includes(normalized);
    // 关键词与标签冲突（ADR §4）：输入正好是插件关键词、同时又有备忘录带这个标签时，
    // 留在首屏并同时给出「插件入口 + 标签命中」；已经在范围内则不算冲突。
    const collides =
      isMemoKeyword &&
      this.scope.kind === "home" &&
      this.memoEntries.some((memo) => memo.tags.some((tag) => tag.toLowerCase() === normalized));
    // 关键词完整匹配即进入插件范围；已在范围内改用另一个别名时更新记下的关键词。
    if (isClipboardKeyword || isChromeKeyword || (isMemoKeyword && !collides)) {
      if (this.scope.kind === "home") {
        this.history.push({
          input: this.input,
          selection: this.selection,
          scope: this.scope,
        });
      }
      this.scope = isClipboardKeyword
        ? { kind: "plugin", id: MOCK_CLIPBOARD_PLUGIN_ID, keyword: normalized }
        : isChromeKeyword
          ? { kind: "plugin", id: MOCK_CHROME_PLUGIN_ID, keyword: normalized }
          : { kind: "plugin", id: MOCK_MEMO_PLUGIN_ID, keyword: normalized };
    } else if (this.scope.kind === "plugin" && normalized.length === 0) {
      // 清空输入即离开插件范围。
      this.scope = { kind: "home" };
    }
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
    // 书签的默认操作是在 Chrome 打开。
    if (itemId.startsWith("chrome-bookmark:")) {
      if (!this.chromePluginEnabled) {
        return { status: "failed", message: "插件「Chrome 书签」已停用，已拒绝执行" };
      }
      const entry = MOCK_BOOKMARKS.find(
        (candidate) => itemId === `chrome-bookmark:${candidate.id}`,
      );
      if (!entry) {
        return { status: "failed", message: `找不到这条书签：${itemId}，请重新查询` };
      }
      if (!/^https?:\/\//i.test(entry.url)) {
        return { status: "failed", message: "链接不受支持：只支持 http / https 链接" };
      }
      if (this.associatedChromeDir === null) {
        return { status: "failed", message: "尚未关联 Chrome profile：请在设置里选择一个 profile" };
      }
      const profile =
        this.chromeProfiles.find((candidate) => candidate.dir === this.associatedChromeDir) ??
        null;
      const args = [
        `--profile-directory=${this.associatedChromeDir}`,
        "--no-first-run",
        "--no-default-browser-check",
      ];
      if (this.customChromeUserDataDir) {
        args.push(`--user-data-dir=/home/user/chrome-custom`);
      }
      args.push(entry.url);
      this.lastChromeLaunch = { program: "/usr/bin/google-chrome", args };
      return {
        status: "done",
        message: `已请求 Chrome 用 profile「${profile?.name ?? this.associatedChromeDir}」打开：${entry.url}；Chrome 已运行时由现有进程接管，Flashcast 不等待进程退出，也无法据此确认页面是否已加载`,
      };
    }
    // 首屏插件入口：等价于用户直接输入该插件的关键词（与宿主一致）。
    if (itemId === `flashcast.plugin.${MOCK_MEMO_PLUGIN_ID}`) {
      if (!this.memoPluginEnabled) {
        return { status: "failed", message: "插件「备忘录」已停用，无法进入" };
      }
      this.history.push({
        input: this.input,
        selection: this.selection,
        scope: this.scope,
      });
      const keyword = this.input.trim().toLowerCase();
      this.scope = { kind: "plugin", id: MOCK_MEMO_PLUGIN_ID, keyword };
      this.items = this.buildItems();
      return { status: "done", message: `已进入「备忘录」范围（关键词 ${keyword}）` };
    }
    // 剪贴板历史的默认操作同样是粘贴：复用与备忘录完全相同的路径。
    if (itemId.startsWith("clipboard:")) {
      if (!this.clipboardPluginEnabled) {
        return { status: "failed", message: "插件「剪贴板历史」已停用，已拒绝执行" };
      }
      const id = itemId.slice("clipboard:".length);
      const entry = this.clipboardEntries.find((candidate) => candidate.id === id) ?? null;
      if (!entry) {
        return {
          status: "failed",
          message: `找不到「${itemId}」对应的剪贴板历史，可能已被删除或过期回收，请重新查询`,
        };
      }
      // 文件列表：整份列表要么全部可恢复，要么如实失败（与宿主 restore_paths 一致）。
      // 视频只是文件，没有特殊分支。
      if (entry.files.length > 0) {
        const missing = entry.files.filter((file) => !file.recoverable);
        if (missing.length > 0) {
          const names = missing
            .map((file) => `${file.name}（${file.problem ?? "不可恢复"}）`)
            .join("、");
          return {
            status: "failed",
            message: `这些文件已不可恢复：${names}（原文件已被移动或删除，且没有保存本机副本）`,
          };
        }
        this.clipboardSuppressed += 1;
        this.lastCopiedFiles = entry.files.map((file) => file.name);
        this.lastCopied = null;
      } else {
        // 图片条目没有可索引的文字，但仍然是可恢复的内容：真实宿主读本机附件后把图片
        // 写回剪贴板。附件读不出来（data URL 为空）时如实失败，不假装粘贴成功。
        const payload = entry.text ?? (entry.imageDataUrl ? entry.summary : null);
        if (payload === null) {
          return {
            status: "failed",
            message: `「${entry.summary}」没有可直接粘贴的文字、图片或文件内容`,
          };
        }
        // 自身写入抑制：这次写入不得再被自己捕获成新条目。
        this.clipboardSuppressed += 1;
        this.lastCopied = payload;
        this.lastCopiedFiles = null;
      }
      const target = this.previousApp;
      if (!this.autoPasteSupported || !target) {
        const reason = target
          ? "浏览器模拟宿主没有可注入按键的桌面会话"
          : "没有记录到唤起前的应用，无法确定粘贴目标";
        return {
          status: "copiedNeedsManualPaste",
          message: `已复制「${entry.summary}」到剪贴板；${reason}；请切换到目标应用后按 Ctrl+V 手动粘贴`,
        };
      }
      this.lastPaste = {
        content: this.lastCopied ?? this.lastCopiedFiles?.join("、") ?? "",
        target: target.name,
        sequence: ["copied", "windowHidden", "restored", "pasted"],
      };
      this.hidden = true;
      return { status: "done", message: `已粘贴「${entry.summary}」到「${target.name}」` };
    }
    // 备忘录的默认操作是粘贴：先准备剪贴板，再按能力决定能否自动粘贴。
    const memo = this.memoFromItemId(itemId);
    if (memo) {
      if (!this.memoPluginEnabled) {
        return { status: "failed", message: "插件「备忘录」已停用，已拒绝执行" };
      }
      this.lastCopied = memo.body;
      const target = this.previousApp;
      if (!this.autoPasteSupported || !target) {
        const reason = target
          ? "浏览器模拟宿主没有可注入按键的桌面会话"
          : "没有记录到唤起前的应用，无法确定粘贴目标";
        return {
          status: "copiedNeedsManualPaste",
          message: `已复制「${memo.title}」到剪贴板；${reason}；请切换到目标应用后按 Ctrl+V 手动粘贴`,
        };
      }
      // 与真实外壳同一顺序：准备剪贴板 → 关闭浮窗 → 恢复目标应用 → 注入粘贴。
      this.lastPaste = {
        content: this.lastCopied,
        target: target.name,
        sequence: ["copied", "windowHidden", "restored", "pasted"],
      };
      this.hidden = true;
      return { status: "done", message: `已粘贴「${memo.title}」到「${target.name}」` };
    }
    // 宿主对这两条命令都有实现，而且都返回**带反馈的** done（见
    // `crates/flashcast-core/src/host.rs` 的 `COMMAND_RESCAN` / `COMMAND_CAPABILITIES`）。
    // 这里必须照做：落到下面的兜底会变成 done + 无 message，而 UI 把这个组合读成
    // 「软件启动成功、外壳关窗」——在浏览器里就是点一下整页变白。
    if (itemId === "flashcast.command.rescan") {
      const response = await this.rescan();
      return {
        status: "done",
        message: `已重新扫描软件列表，当前结果 ${response.items.length} 条`,
      };
    }
    if (itemId === "flashcast.command.capabilities") {
      const capabilities = await this.get_capabilities();
      // 与设置页的能力区同一口径：未覆盖表示本环境无法判定，不等于不支持。
      return {
        status: "done",
        message:
          `浏览器模拟宿主：系统 ${capabilities.os} ${capabilities.arch}，` +
          `会话 ${capabilities.session}；全局快捷键、剪贴板与自动粘贴在本环境未覆盖，` +
          `不代表真实桌面行为。`,
      };
    }
    const app = MOCK_APPS.find((candidate) => itemId === `app:${candidate.id}`);
    if (app?.failsToLaunch) {
      return {
        status: "failed",
        message: `无法启动「${app.title}」：浏览器模拟宿主中的失败样例`,
      };
    }
    if (app) {
      // 启动成功且没有反馈语：外壳负责关窗（浏览器里表现为整页隐藏，由替身入口唤起）。
      return { status: "done", message: null };
    }
    // 与宿主一致：不认识的条目要如实失败。这里**绝不能**返回 done + 无 message，
    // 那会被 UI 当成一次成功的软件启动。
    return { status: "failed", message: `未知条目：${itemId}` };
  }

  /** 从结果标识还原备忘录（`memo:<id>`）。 */
  private memoFromItemId(itemId: string): Memo | null {
    if (!itemId.startsWith("memo:")) {
      return null;
    }
    const id = itemId.slice("memo:".length);
    return this.memoEntries.find((memo) => memo.id === id) ?? null;
  }

  async preview(itemId: string): Promise<Preview | null> {
    if (itemId.startsWith("chrome-bookmark:")) {
      const entry = MOCK_BOOKMARKS.find(
        (candidate) => itemId === `chrome-bookmark:${candidate.id}`,
      );
      return entry
        ? { kind: "text", title: entry.title, body: `${entry.url}\n目录：${entry.folder}` }
        : null;
    }
    const memo = this.memoFromItemId(itemId);
    if (memo) {
      return { kind: "text", title: memo.title, body: memo.body };
    }
    if (itemId.startsWith("clipboard:")) {
      const id = itemId.slice("clipboard:".length);
      const entry = this.clipboardEntries.find((candidate) => candidate.id === id);
      if (!entry) {
        return null;
      }
      // 图片条目给出完整图片的 data URL（真实外壳读本机附件后编码），
      // 与 `PreviewView` 的字段一一对应。
      if (entry.imageDataUrl) {
        return { kind: "image", dataUrl: entry.imageDataUrl };
      }
      return {
        kind: "text",
        title: entry.summary,
        body: entry.text ?? "（没有可显示的文字内容）",
      };
    }
    return null;
  }

  async get_clipboard_state(): Promise<ClipboardStateView> {
    return this.clipboardStateView();
  }

  async set_clipboard_paused(paused: boolean): Promise<ClipboardStateView> {
    this.settings = { ...this.settings, clipboard: { ...this.settings.clipboard, paused } };
    return this.clipboardStateView();
  }

  async set_clipboard_limits(
    retentionDays: number,
    capacity: number,
  ): Promise<ClipboardStateView> {
    if (retentionDays < 1 || retentionDays > 3650) {
      throw `剪贴板保留期限必须在 1 到 3650 天之间，当前为 ${retentionDays}`;
    }
    if (capacity < 1 || capacity > 100000) {
      throw `剪贴板容量必须在 1 到 100000 条之间，当前为 ${capacity}`;
    }
    this.settings = {
      ...this.settings,
      clipboard: { ...this.settings.clipboard, retentionDays, capacity },
    };
    // 与真实宿主一致：改小容量立刻回收最旧的未置顶条目。
    const ordered = [...this.clipboardEntries].sort(
      (a, b) => b.capturedAtMs - a.capturedAtMs,
    );
    const keep = new Set<string>();
    let used = 0;
    for (const entry of ordered) {
      if (entry.pinned || used < capacity) {
        keep.add(entry.id);
        used += 1;
      }
    }
    this.clipboardEntries = this.clipboardEntries.filter((entry) => keep.has(entry.id));
    return this.clipboardStateView();
  }

  async pin_clipboard_entry(id: string, pinned: boolean): Promise<ClipboardStateView> {
    const entry = this.clipboardEntries.find((candidate) => candidate.id === id);
    if (!entry) {
      throw `找不到这条剪贴板历史：${id}`;
    }
    entry.pinned = pinned;
    return this.clipboardStateView();
  }

  async delete_clipboard_entry(id: string): Promise<ClipboardStateView> {
    const before = this.clipboardEntries.length;
    this.clipboardEntries = this.clipboardEntries.filter((entry) => entry.id !== id);
    if (this.clipboardEntries.length === before) {
      throw `找不到这条剪贴板历史：${id}`;
    }
    return this.clipboardStateView();
  }

  async clear_clipboard_history(): Promise<ClipboardStateView> {
    this.clipboardEntries = [];
    return this.clipboardStateView();
  }

  /**
   * 显式保存本机副本（浏览器模拟）。
   *
   * 语义与宿主一致：只对**引用**生效，副本不依赖原文件；失败原因如实抛出。
   * 真实行为（原子复制、容量判定、附件回收）由
   * `crates/flashcast-core/tests/clipboard_files.rs` 验证。
   */
  async save_clipboard_file_copy(
    id: string,
    attachmentId: string,
  ): Promise<ClipboardStateView> {
    const entry = this.clipboardEntries.find((candidate) => candidate.id === id);
    if (!entry) {
      throw `找不到这条剪贴板历史或它的文件条目：${id}`;
    }
    const file = entry.files.find((candidate) => candidate.attachmentId === attachmentId);
    if (!file) {
      throw `找不到这条剪贴板历史或它的文件条目：${attachmentId}`;
    }
    if (this.clipboardCopyError) {
      throw this.clipboardCopyError;
    }
    if (file.kind.kind === "fileCopy") {
      throw `「${file.name}」已经是本机副本，不需要再复制一次`;
    }
    if (!file.recoverable) {
      throw `原文件已不存在或被移动：${file.problem ?? file.name}`;
    }
    // 副本就地改写引用行（与宿主一致，标识不变），原文件只被读取。
    file.kind = { kind: "fileCopy" };
    file.kindLabel = "已保存副本";
    file.recoverable = true;
    file.problem = null;
    entry.references = entry.files.filter(
      (candidate) => candidate.kind.kind === "fileReference",
    ).length;
    entry.fileCopies = entry.files.filter(
      (candidate) => candidate.kind.kind === "fileCopy",
    ).length;
    return this.clipboardStateView();
  }

  /** 浏览器交互检查用的钩子：让下一次保存副本失败（超限 / 访问失败 / 复制中断）。 */
  simulateClipboardCopyFailure(reason: string | null): void {
    this.clipboardCopyError = reason;
  }

  /** 浏览器交互检查用的钩子：模拟原文件被删除，按文件名把引用标为不可恢复。 */
  simulateClipboardFileMissing(name: string): void {
    this.clipboardMissingFiles.add(name);
    for (const entry of this.clipboardEntries) {
      for (const file of entry.files) {
        if (file.name === name && file.kind.kind === "fileReference") {
          file.recoverable = false;
          file.problem = `原文件已不存在：/home/user/下载/${name}`;
        }
      }
    }
  }

  async get_chrome_state(): Promise<ChromeState> {
    return this.chromeState();
  }

  async associate_chrome_profile(profileDir: string): Promise<ChromeState> {
    const profile = this.chromeProfiles.find((candidate) => candidate.dir === profileDir);
    if (!profile) {
      throw `profile 目录不存在：${profileDir}（已发现的 profile：${this.chromeProfiles
        .map((candidate) => candidate.dir)
        .join("、")}）`;
    }
    if (!this.chromeAvailable) {
      throw this.chromeError ?? "没有找到 Chrome 可执行文件";
    }
    this.associatedChromeDir = profile.dir;
    this.chromeStatus = { kind: "ok", count: MOCK_BOOKMARKS.length };
    // 与真实宿主一致：换关联后需要重新检索（列表在下一次查询时重算）。
    this.items = this.buildItems();
    return this.chromeState();
  }

  async refresh_chrome_bookmarks(): Promise<ChromeState> {
    if (this.chromeStatus.kind === "corrupt") {
      // 模拟文件仍然是坏的：如实报告，并保留上一次可用的条目。
      this.chromeStatus = {
        kind: "corrupt",
        reason: "JSON 解析失败：expected value at line 1 column 9",
      };
    }
    return this.chromeState();
  }

  async memos(): Promise<Memo[]> {
    return this.memoEntries.map((memo) => ({ ...memo, tags: [...memo.tags] }));
  }

  async memo_problems(): Promise<MemoProblem[]> {
    return [];
  }

  async create_memo(title: string, tags: string[], body: string): Promise<Memo> {
    this.requireMemoWritable();
    if (!title.trim()) throw "备忘录标题不能为空";
    if (!body.trim()) throw "备忘录正文不能为空";
    this.memoCounter += 1;
    const memo: Memo = {
      id: `memo-${this.memoCounter}`,
      title: title.trim(),
      tags: tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0),
      body,
    };
    this.memoEntries = [...this.memoEntries, memo].sort((a, b) => a.id.localeCompare(b.id));
    return { ...memo, tags: [...memo.tags] };
  }

  async update_memo(id: string, title: string, tags: string[], body: string): Promise<Memo> {
    this.requireMemoWritable();
    if (!title.trim()) throw "备忘录标题不能为空";
    if (!body.trim()) throw "备忘录正文不能为空";
    const existing = this.memoEntries.find((memo) => memo.id === id);
    if (!existing) {
      throw `找不到这条备忘录：${id}`;
    }
    const memo: Memo = {
      id,
      title: title.trim(),
      tags: tags.map((tag) => tag.trim()).filter((tag) => tag.length > 0),
      body,
    };
    this.memoEntries = this.memoEntries.map((candidate) => (candidate.id === id ? memo : candidate));
    return { ...memo, tags: [...memo.tags] };
  }

  async delete_memo(id: string): Promise<void> {
    this.requireMemoWritable();
    if (!this.memoEntries.some((memo) => memo.id === id)) {
      throw `找不到这条备忘录：${id}`;
    }
    this.memoEntries = this.memoEntries.filter((memo) => memo.id !== id);
  }

  /** 与宿主一致：插件停用或未关联工作区时拒绝写入。 */
  private requireMemoWritable(): void {
    if (!this.memoPluginEnabled) {
      throw "备忘录插件已停用，无法创建或修改备忘录";
    }
    if (!this.workspace.path) {
      throw "尚未关联配置工作区，无法保存备忘录";
    }
  }

  async set_feature_plugin_enabled(id: string, enabled: boolean): Promise<PluginView[]> {
    if (
      id !== MOCK_MEMO_PLUGIN_ID &&
      id !== MOCK_CHROME_PLUGIN_ID &&
      id !== MOCK_CLIPBOARD_PLUGIN_ID
    ) {
      throw `插件清单里没有这个标识：${id}`;
    }
    if (id === MOCK_CLIPBOARD_PLUGIN_ID) {
      this.clipboardPluginEnabled = enabled;
      // 停用即停止后台活动（真实宿主会停掉轮询线程）；离开该范围并重算。
      if (!enabled && this.scope.kind === "plugin" && this.scope.id === id) {
        this.scope = { kind: "home" };
      }
      this.items = this.buildItems();
      const status = await this.get_status();
      return status.plugins;
    }
    if (id === MOCK_CHROME_PLUGIN_ID) {
      this.chromePluginEnabled = enabled;
      if (!enabled && this.scope.kind === "plugin" && this.scope.id === id) {
        this.scope = { kind: "home" };
      }
      this.items = this.buildItems();
      const status = await this.get_status();
      return status.plugins;
    }
    this.memoPluginEnabled = enabled;
    // 与宿主一致：停用当前所在范围的插件后回到首屏并按当前输入重算。
    if (!enabled && this.scope.kind === "plugin" && this.scope.id === id) {
      this.scope = { kind: "home" };
    }
    this.items = this.buildItems();
    const status = await this.get_status();
    return status.plugins;
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
    this.scope = entry.scope;
    this.items = this.buildItems();
    this.selection = Math.min(entry.selection, Math.max(0, this.items.length - 1));
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
    // 自动粘贴能力与模拟宿主的执行路径保持一致：浏览器里没有可注入按键的桌面会话，
    // 因此默认是「不支持」；交互检查可以临时打开它来验证 UI 的粘贴路径。
    return {
      ...MOCK_CAPABILITIES,
      autoPaste: this.autoPasteSupported
        ? { status: "supported" as const }
        : {
            status: "unsupported" as const,
            reason: "浏览器模拟宿主没有可注入按键的桌面会话",
          },
    };
  }

  /** 交互检查用：模拟「本会话支持自动粘贴」与「记录到了唤起前的应用」。 */
  setAutoPasteSupported(supported: boolean): void {
    this.autoPasteSupported = supported;
  }

  /** 交互检查用：模拟唤起前的前台应用；传 `null` 表示没有记录到目标。 */
  setPreviousApp(app: { id: string; name: string } | null): void {
    this.previousApp = app;
  }

  async get_settings(): Promise<Settings> {
    return this.settings;
  }

  async set_settings(settings: Settings): Promise<StatusView["hotkey"]> {
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
      // 与真实宿主一致：状态里的能力快照与 `get_capabilities` 来自同一次探测。
      capabilities: await this.get_capabilities(),
      plugins: [
        {
          id: MOCK_MEMO_PLUGIN_ID,
          name: "备忘录",
          version: "0.1.0",
          keywords: [...MOCK_MEMO_KEYWORDS],
          enabled: this.memoPluginEnabled,
        },
        {
          id: MOCK_CHROME_PLUGIN_ID,
          name: "Chrome 书签",
          version: "0.1.0",
          keywords: [...MOCK_CHROME_KEYWORDS],
          enabled: this.chromePluginEnabled,
        },
        {
          id: MOCK_CLIPBOARD_PLUGIN_ID,
          name: "剪贴板历史",
          version: "0.1.0",
          keywords: [...MOCK_CLIPBOARD_KEYWORDS],
          enabled: this.clipboardPluginEnabled,
        },
      ],
    };
  }

  async get_workspace(): Promise<WorkspaceStatus> {
    return this.workspace;
  }

  async get_theme(): Promise<ThemeState> {
    return this.themeState();
  }

  async select_theme(id: string): Promise<ThemeState> {
    const theme = this.mockThemes.find((candidate) => candidate.id === id);
    if (!theme) {
      throw `找不到主题：${id}`;
    }
    if (!theme.enabled) {
      throw `主题「${theme.name}」已停用，请先启用后再选择`;
    }
    this.selectedTheme = id;
    this.themeError = null;
    return this.themeState();
  }

  async set_plugin_enabled(id: string, enabled: boolean): Promise<ThemeState> {
    if (id === "flashcast.theme.arc" && !enabled) throw new Error("内置电弧是恢复基线，不能停用");
    const theme = this.mockThemes.find((candidate) => candidate.id === id);
    if (!theme) {
      // 功能插件（备忘录 / Chrome 书签）：走与真实宿主相同的入口，外观保持不变。
      if (id === MOCK_MEMO_PLUGIN_ID || id === MOCK_CHROME_PLUGIN_ID) {
        await this.set_feature_plugin_enabled(id, enabled);
        return this.themeState();
      }
      throw `插件清单里没有这个标识：${id}`;
    }
    theme.enabled = enabled;
    if (!enabled && this.selectedTheme === id) {
      this.selectedTheme = "flashcast.theme.arc";
      this.themeError = `主题「${theme.name}」已停用，已切换回「电弧」`;
    }
    return this.themeState();
  }

  async install_theme(path: string): Promise<ThemeState> {
    const target = path.trim();
    if (target.length === 0) {
      throw "主题包不存在：路径为空";
    }
    if (!this.workspace.path) {
      throw "尚未关联配置工作区，无法安装主题包";
    }
    if (target === MockHost.BROKEN_THEME_PACKAGE) {
      // 与宿主一致：校验失败给出可读中文原因，且不改动已安装内容与当前外观。
      throw "主题无效：主题 JSON 解析失败：expected value at line 3 column 1";
    }
    if (target !== MockHost.THEME_PACKAGE) {
      throw `主题包不存在：${target}`;
    }
    const existing = this.mockThemes.find(
      (theme) => theme.id === MOCK_INSTALLED_THEME.id,
    );
    if (!existing) {
      this.mockThemes.push({ ...MOCK_INSTALLED_THEME });
    }
    return this.themeState();
  }

  async remove_theme(id: string): Promise<ThemeState> {
    const theme = this.mockThemes.find((candidate) => candidate.id === id);
    if (!theme) {
      throw `找不到主题：${id}`;
    }
    if (theme.builtin) {
      throw `内置主题不能移除：${theme.name}`;
    }
    this.mockThemes = this.mockThemes.filter((candidate) => candidate.id !== id);
    if (this.selectedTheme === id) {
      this.selectedTheme = "flashcast.theme.arc";
      this.themeError = `主题「${theme.name}」已移除，已切换回「电弧」`;
    }
    return this.themeState();
  }

  async set_appearance_preferences(appearance: import("./types").ThemeAppearance, style: string, reduceTransparency: boolean): Promise<ThemeState> {
    const state = this.themeState();
    if (!state.styles.some(s => s.id === style)) throw new Error("主题没有提供该表面风格");
    this.appearancePreference = appearance;
    this.themeStyles[this.selectedTheme] = style;
    this.reduceTransparency = reduceTransparency;
    return this.themeState();
  }
  async sync_window_material() { return { supported: true, reason: null }; }

  async set_system_appearance(appearance: Appearance): Promise<ThemeState> {
    this.systemAppearance = appearance;
    return this.themeState();
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
      theme: this.themeState(),
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
      theme: this.themeState(),
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

  /**
   * 模拟外部编辑工作区里的主题配置：可用则切换，不可用则保留上一次可用外观
   * 并给出原因。与 `Host::apply_workspace_config` 的语义一致。
   */
  simulateExternalThemeEdit(selected: string): void {
    const theme = this.mockThemes.find((candidate) => candidate.id === selected);
    let error: string | null = null;
    if (!theme || !theme.enabled) {
      error = `主题「${selected}」不可用（不存在、已停用或无法解析），继续使用上一次可用外观`;
    } else {
      this.selectedTheme = selected;
    }
    this.themeError = error;
    const reload = {
      path: `${this.workspace.path ?? MOCK_REPO}/theme.json`,
      applied: error === null,
      settings: this.settings,
      theme: this.themeState(),
      error,
    };
    this.emit("flashcast://theme", this.themeState());
    this.emit("flashcast://workspace", {
      status: this.workspace,
      settings: this.settings,
      theme: this.themeState(),
      reload,
    } as WorkspaceEvent);
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
    this.incoming = {
      hotkey,
      theme: "flashcast.theme.dark",
      memo: "memos/remote-note.md",
    };
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
          this.selectedTheme = incoming.theme;
          this.memoFiles = [...this.memoFiles, incoming.memo];
          // 主题变化要像宿主一样推送给 UI（`flashcast://theme`）。
          this.emit("flashcast://theme", this.themeState());
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
          reload: {
            path,
            applied: this.incoming !== null,
            settings: this.settings,
            theme: this.themeState(),
            error: null,
          },
          theme: this.themeState().selected,
          memos: this.memoFiles,
          message: "已快进拉取到 8b2d4e1，共 1 个提交",
        };
      }
      this.syncProgress = { ...IDLE_SYNC_PROGRESS, phase: "done", updates: 8, message: "已是最新" };
      return {
        result: { kind: "upToDate" },
        status: this.syncStatusSnapshot(),
        reload: {
          path,
          applied: false,
          settings: this.settings,
          theme: this.themeState(),
          error: null,
        },
        theme: this.themeState().selected,
        memos: this.memoFiles,
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
