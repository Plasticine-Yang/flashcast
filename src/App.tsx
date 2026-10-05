import { useEffect, useMemo, useRef, useState } from "react";
import { api, isBrowserMock } from "./api";
import type {
  ActionOutcome,
  Appearance,
  Capabilities,
  ChromeState,
  CloneProgress,
  ItemView,
  Memo,
  MemoProblem,
  PluginView,
  Preview,
  QueryView,
  Settings,
  StatusView,
  SyncProgress,
  SyncStatus,
  ThemeState,
  WorkspaceChanges,
  WorkspaceEvent,
  WorkspaceStatus,
  ClipboardStateView,
} from "./types";
import { ActionBar } from "./components/ActionBar";
import { MemoPreview } from "./components/MemoPreview";
import { ResultList } from "./components/ResultList";
import {
  SettingsScreen,
  type SettingsMessage,
  type SettingsSectionId,
} from "./components/SettingsScreen";
import { StatusBanner } from "./components/StatusBanner";


/** 把宿主下发的语义 token 写成根元素上的 CSS 自定义属性。
 *
 * UI 不认识任何具体主题：主题只改这些属性，组件与布局保持不变。
 */
export function applyThemeVars(theme: ThemeState) {
  const root = document.documentElement;
  for (const { name, value } of theme.cssVars) {
    root.style.setProperty(name, value);
  }
  // 供浏览器交互检查与 CSS 读取当前实际外观。
  root.dataset.themeAppearance = theme.appearance;
  root.dataset.themeSelected = theme.selected;
  root.dataset.surfaceRenderer = theme.renderer;
  root.style.setProperty("--fc-fill-opacity", String(theme.surface.fillOpacity));
  root.style.setProperty("--fc-blur", `${theme.surface.blur}px`);
  root.style.setProperty("--fc-saturation", String(theme.surface.saturation));
  root.style.setProperty("--fc-rim", `${theme.surface.rim}px`);
}

/** 系统外观：`prefers-color-scheme` 是 OS 外观在 webview 里的可靠信号。 */
export function systemAppearance(): Appearance {
  if (typeof window === "undefined" || !window.matchMedia) {
    return "light";
  }
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

/** 最近一次已应用的响应序号。seq 更小的响应必须被丢弃（ADR §3）。 */
const EMPTY_RESPONSE: QueryView = {
  seq: 0,
  scope: { kind: "home" },
  scopeLabel: "首屏",
  input: "",
  items: [],
  selection: 0,
  notice: null,
  pluginFailures: [],
};

/** 界面：搜索首屏或设置页。 */
type Screen = "search" | "settings";

export default function App() {
  const [input, setInput] = useState("");
  const [response, setResponse] = useState<QueryView>(EMPTY_RESPONSE);
  const [feedback, setFeedback] = useState<ActionOutcome | null>(null);
  const [status, setStatus] = useState<StatusView | null>(null);
  /** 运行环境与能力状态：设置页每次打开时重新探测一次。 */
  const [capabilities, setCapabilities] = useState<Capabilities | null>(null);
  const [focused, setFocused] = useState(true);
  const [visible, setVisible] = useState(true);
  const [composing, setComposing] = useState(false);
  // 「菜单」状态：ticket 01 没有菜单，Escape 的优先级顺序在这里已经预留。
  const [menuOpen] = useState(false);

  // 设置页状态。
  const [screen, setScreen] = useState<Screen>("search");
  const [workspace, setWorkspace] = useState<WorkspaceStatus | null>(null);
  const [settings, setSettings] = useState<Settings | null>(null);
  const [theme, setTheme] = useState<ThemeState | null>(null);
  const [materialNotice, setMaterialNotice] = useState<string | null>(null);
  const [settingsMessage, setSettingsMessage] = useState<SettingsMessage | null>(null);
  /** 当前显示的设置区块。放在 App 里，所以在会话内切走再回来会回到同一区块
   * （SettingsScreen 每次进设置页都会重新挂载，放它内部就记不住）。 */
  const [settingsSection, setSettingsSection] = useState<SettingsSectionId>("hotkey");
  const [workspaceAlert, setWorkspaceAlert] = useState<string | null>(null);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [cloneProgress, setCloneProgress] = useState<CloneProgress | null>(null);
  // 变更与提交状态。提交范围完全由用户勾选的路径决定。
  const [changes, setChanges] = useState<WorkspaceChanges | null>(null);
  const [commitMessage, setCommitMessage] = useState("");
  const [selectedPaths, setSelectedPaths] = useState<string[]>([]);
  const [diffPath, setDiffPath] = useState<string | null>(null);
  const [sync, setSync] = useState<SyncStatus | null>(null);
  const [syncProgress, setSyncProgress] = useState<SyncProgress | null>(null);
  // 备忘录（ticket 07）：设置页的管理列表与搜索页的完整预览。
  const [plugins, setPlugins] = useState<PluginView[]>([]);
  const [memos, setMemos] = useState<Memo[]>([]);
  const [memoProblems, setMemoProblems] = useState<MemoProblem[]>([]);
  const [chrome, setChrome] = useState<ChromeState | null>(null);
  /** 剪贴板历史状态与管理列表（ticket 09）。 */
  const [clipboard, setClipboard] = useState<ClipboardStateView | null>(null);
  const [preview, setPreview] = useState<Preview | null>(null);
  const [previewOpen, setPreviewOpen] = useState(true);

  const inputRef = useRef<HTMLInputElement>(null);
  const appliedSeq = useRef(0);
  const composingRef = useRef(false);

  const apply = (next: QueryView) => {
    // 旧查询结果不得覆盖新结果。
    if (next.seq <= appliedSeq.current) {
      return;
    }
    appliedSeq.current = next.seq;
    setResponse(next);
  };

  useEffect(() => {
    // 首屏必须是一次空查询：宿主的 snapshot() 只回放当前状态，在还没有查询过时
    // 它是空的，只有 query("") 才会给出快速访问项。
    void api.query("").then(apply);
    void api.get_status().then(next => { setStatus(next); setPlugins(next.plugins); });
    // 设置页的运行环境与能力报告直接来自平台层的 CapabilityProbe（`get_capabilities`）。
    void api.get_capabilities().then(setCapabilities);
    void api.get_workspace().then((next) => {
      setWorkspace(next);
      setWorkspaceAlert(next.error);
    });
    void api.get_settings().then(setSettings);
    // 主题：先按当前系统外观解析一次（跟随系统的主题据此落定），再订阅后续变化。
    void api
      .set_system_appearance(systemAppearance())
      .then(setTheme)
      .catch(() => api.get_theme().then(setTheme));
    const unlisteners: (() => void)[] = [];
    const register = async () => {
      unlisteners.push(
        await api.on("flashcast://summoned", () => {
          setVisible(true);
          setFeedback(null);
          setInput("");
          setScreen("search");
          // 重新唤起同样回到空查询的首屏，否则会出现「输入框为空但列表还是上次过滤结果」。
          void api.query("").then(apply);
          focusInput();
        }),
      );
      unlisteners.push(
        await api.on("flashcast://dismissed", () => {
          setVisible(false);
        }),
      );
      unlisteners.push(
        await api.on("flashcast://state", (payload) => {
          apply(payload as QueryView);
        }),
      );
      unlisteners.push(
        await api.on("flashcast://hotkey-status", (payload) => {
          setStatus((current) =>
            current
              ? { ...current, hotkey: payload as StatusView["hotkey"] }
              : current,
          );
        }),
      );
      // 宿主推送的主题状态（选择主题、外部改 theme.json、启停主题）。
      unlisteners.push(
        await api.on("flashcast://theme", (payload) => {
          setTheme(payload as ThemeState);
        }),
      );
      // 工作区外部修改：有效则已生效，无效则保留上次有效状态并显示原因。
      unlisteners.push(
        await api.on("flashcast://workspace", (payload) => {
          const event = payload as WorkspaceEvent;
          setWorkspace(event.status);
          setSettings(event.settings);
          if (event.theme) {
            setTheme(event.theme);
          }
          setWorkspaceAlert(event.status.error);
          // 工作区被外部改动后，变更视图必须按仓库真实状态重新读取。
          void loadChanges();
          void loadSync();
          void loadPlugins();
          if (event.reload?.error) {
            setSettingsMessage({ level: "error", text: event.reload.error });
          } else if (event.reload?.applied) {
            setSettingsMessage({
              level: "info",
              text: "已重新加载工作区中的设置",
            });
          }
        }),
      );
    };
    void register();
    focusInput();

    // 跟随系统：OS 外观在运行中变化时重新解析主题，不需要重启应用。
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onSystemAppearanceChange = () => {
      void api.set_system_appearance(systemAppearance()).then(setTheme);
    };
    media.addEventListener("change", onSystemAppearanceChange);

    return () => {
      media.removeEventListener("change", onSystemAppearanceChange);
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

  // 主题只通过 CSS 自定义属性生效：切换主题不改变任何布局。
  useEffect(() => {
    if (!theme) return;
    applyThemeVars(theme);
    let cancelled = false;
    document.documentElement.dataset.surfaceEffective = "solid";
    void api.sync_window_material().then(result => {
      if (cancelled) return;
      document.documentElement.dataset.surfaceEffective = result.supported ? theme.renderer : "solid";
      setMaterialNotice(result.reason);
    }).catch(() => {
      if (!cancelled) setMaterialNotice("透明材质暂不可用，已使用实底");
    });
    return () => { cancelled = true; };
  }, [theme]);

  function focusInput() {
    // 唤起后立即进入输入状态。
    requestAnimationFrame(() => {
      inputRef.current?.focus();
      inputRef.current?.select();
    });
  }

  const openSettings = () => {
    setSettingsMessage(null);
    setScreen("settings");
    void api.get_workspace().then(setWorkspace);
    void api.get_settings().then(setSettings);
    void api.get_theme().then(setTheme);
    void loadChanges();
    void loadSync();
    void loadPlugins();
    void loadMemos();
    void loadChrome();
    void loadClipboard();
    // 每次打开设置页都重新探测一次：会话类型与权限可能在应用运行期间变化。
    void api.get_capabilities().then(setCapabilities);
  };

  /** 读取随应用提供的功能插件与启用状态（备忘录的启停入口用）。 */
  const loadPlugins = async () => {
    try {
      const status = await api.get_status();
      setPlugins(status.plugins);
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  /** 读取工作区里的备忘录与无法读取的文件（保留可用内容并如实报告）。 */
  const loadMemos = async () => {
    try {
      const [next, problems] = await Promise.all([api.memos(), api.memo_problems()]);
      setMemos(next);
      setMemoProblems(problems);
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  /** 读取剪贴板历史状态与管理列表（含存储 / 容量 / 最近失败的准确状态）。 */
  const loadClipboard = async () => {
    try {
      setClipboard(await api.get_clipboard_state());
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  /** 读取 Chrome 状态：发现结果、profile 关联与书签索引（含变化后重建）。 */
  const loadChrome = async () => {
    try {
      setChrome(await api.get_chrome_state());
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  /** 关联一个已发现的 profile；失败时给出中文原因且不改变原关联。 */
  const handleAssociateChromeProfile = (profileDir: string) => {
    void runSettingsAction(
      () => api.associate_chrome_profile(profileDir),
      (next) => {
        setChrome(next);
        setSettingsMessage({
          level: "info",
          text: `已关联 Chrome profile：${next.associatedName ?? profileDir}`,
        });
        // 换关联后当前查询需要重算（书签集合变了）。
        void api.query(input).then(apply);
      },
    );
  };

  /** 显式重新读取书签文件（外部改动后的兜底入口）。 */
  const handleRefreshChromeBookmarks = () => {
    void runSettingsAction(
      () => api.refresh_chrome_bookmarks(),
      (next) => {
        setChrome(next);
        setSettingsMessage({ level: "info", text: next.bookmarksLabel });
        void api.query(input).then(apply);
      },
    );
  };

  const memoPlugin = plugins.find((plugin) => plugin.id === "memo") ?? null;

  /** 启用 / 停用功能插件：状态写在清单里，列表与搜索随之刷新。 */
  const handleToggleFeaturePlugin = (id: string, enabled: boolean) => {
    void runSettingsAction(
      () => api.set_feature_plugin_enabled(id, enabled),
      (next) => {
        setPlugins(next);
        setSettingsMessage({
          level: "info",
          text: `${enabled ? "已启用" : "已停用"}插件：${
            next.find((plugin) => plugin.id === id)?.name ?? id
          }`,
        });
        // 停用后工作区内容不变，但管理入口与搜索结果都要按新状态重算。
        void loadMemos();
        void loadChrome();
        void loadClipboard();
        void api.query(input).then(apply);
      },
    );
  };

  /** 暂停 / 恢复剪贴板记录。 */
  const handleToggleClipboardPaused = (paused: boolean) => {
    void runSettingsAction(
      () => api.set_clipboard_paused(paused),
      (next) => {
        setClipboard(next);
        setSettingsMessage({
          level: "info",
          text: paused ? "已暂停记录剪贴板" : "已恢复记录剪贴板",
        });
      },
    );
  };

  /** 保存保留期限与容量；改小容量会立刻回收超出的条目。 */
  const handleSaveClipboardLimits = (retentionDays: number, capacity: number) => {
    void runSettingsAction(
      () => api.set_clipboard_limits(retentionDays, capacity),
      (next) => {
        setClipboard(next);
        setSettingsMessage({
          level: "info",
          text: `剪贴板历史范围已更新：保留 ${next.retentionDays} 天，容量 ${next.capacity} 条`,
        });
        void api.query(input).then(apply);
      },
    );
  };

  const handlePinClipboardEntry = (id: string, pinned: boolean) => {
    void runSettingsAction(
      () => api.pin_clipboard_entry(id, pinned),
      (next) => {
        setClipboard(next);
        // 置顶会影响搜索结果的排序，当前查询要重算。
        void api.query(input).then(apply);
      },
    );
  };

  const handleDeleteClipboardEntry = (id: string) => {
    void runSettingsAction(
      () => api.delete_clipboard_entry(id),
      (next) => {
        setClipboard(next);
        setSettingsMessage({ level: "info", text: "已删除这条剪贴板历史" });
        void api.query(input).then(apply);
      },
    );
  };

  const handleClearClipboardHistory = () => {
    void runSettingsAction(
      () => api.clear_clipboard_history(),
      (next) => {
        setClipboard(next);
        setSettingsMessage({ level: "info", text: "已清空剪贴板历史" });
        void api.query(input).then(apply);
      },
    );
  };

  /**
   * 显式为一个文件引用保存本机副本（ticket 12）。
   *
   * 只有用户点击才会复制原文件内容；失败原因（原文件失效、访问失败、超限、复制中断、
   * 不支持的类型）由宿主给出并原样展示。原文件只被读取，不会被移动或删除。
   */
  const handleSaveClipboardFileCopy = (id: string, attachmentId: string) => {
    void runSettingsAction(
      () => api.save_clipboard_file_copy(id, attachmentId),
      (next) => {
        setClipboard(next);
        setSettingsMessage({
          level: "info",
          text: "已保存本机副本：原文件删除后仍可恢复",
        });
        // 引用与副本的数量会影响副标题，当前查询要重算。
        void api.query(input).then(apply);
      },
    );
  };

  const handleCreateMemo = (title: string, tags: string[], body: string) =>
    runSettingsAction(
      () => api.create_memo(title, tags, body),
      async (memo) => {
        await loadMemos();
        setSettingsMessage({ level: "info", text: `已创建备忘录：${memo.title}` });
      },
    );

  const handleUpdateMemo = (id: string, title: string, tags: string[], body: string) =>
    runSettingsAction(
      () => api.update_memo(id, title, tags, body),
      async (memo) => {
        await loadMemos();
        setSettingsMessage({ level: "info", text: `已保存备忘录：${memo.title}` });
      },
    );

  const handleDeleteMemo = (id: string) =>
    runSettingsAction(
      () => api.delete_memo(id),
      async () => {
        await loadMemos();
        setSettingsMessage({ level: "info", text: `已删除备忘录：${id}` });
      },
    );

  /**
   * 应用一份变更快照：勾选与差异展示都收敛到仍然存在的路径上，
   * 避免提交后残留指向已消失文件的勾选。
   */
  const applyChanges = (next: WorkspaceChanges) => {
    setChanges(next);
    setSelectedPaths((current) =>
      current.filter((path) => next.files.some((file) => file.path === path)),
    );
    setDiffPath((current) =>
      current && next.files.some((file) => file.path === current)
        ? current
        : (next.files[0]?.path ?? null),
    );
  };

  /** 读取工作区 Git 变更。失败只影响这一区段，不影响设置与备忘录。 */
  const loadChanges = async () => {
    try {
      applyChanges(await api.get_git_changes());
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  const handleTogglePath = (path: string) => {
    setSelectedPaths((current) =>
      current.includes(path) ? current.filter((item) => item !== path) : [...current, path],
    );
    setDiffPath(path);
  };

  const handleToggleAllPaths = () => {
    if (!changes) {
      return;
    }
    setSelectedPaths((current) =>
      current.length === changes.files.length ? [] : changes.files.map((file) => file.path),
    );
  };

  const handleRefreshChanges = () => {
    void runSettingsAction(loadChanges, () => {
      setSettingsMessage({ level: "info", text: "已按仓库当前状态重新读取变更" });
    });
  };

  /** 读取同步状态。失败只影响这一区段，不影响设置与备忘录。 */
  const loadSync = async () => {
    try {
      setSync(await api.get_sync_status());
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
    }
  };

  /**
   * 重新检测同步状态：用户在应用外部处理完阻塞（提交、合并、中止变基、
   * git remote add…）之后点击它，宿主按真实 Git 状态重建状态与指引。
   */
  const handleRedetectSync = () => {
    void runSettingsAction(
      () => api.redetect_sync_state(),
      (next) => {
        setSync(next);
        setSettingsMessage({
          level: "info",
          text: next.blocking
            ? `重新检测完成，仍有阻塞：${next.blocking.label}`
            : "重新检测完成：可以进行无冲突的快进拉取与推送",
        });
      },
    );
    // 外部也可能顺带改了配置：一并重新读取设置与变更，避免界面停留在旧值。
    void api.get_settings().then(setSettings);
    void loadChanges();
  };

  /** 轮询同步进度快照（网络操作在宿主侧阻塞执行）。 */
  const pollSyncProgress = () => {
    setSyncProgress(null);
    return window.setInterval(() => {
      void api
        .sync_progress()
        .then(setSyncProgress)
        .catch(() => {});
    }, 100);
  };

  const stopSyncPolling = (timer: number) => {
    window.clearInterval(timer);
    void api
      .sync_progress()
      .then(setSyncProgress)
      .catch(() => {});
  };

  /**
   * 拉取（仅快进）：成功后宿主已重新加载设置、主题与备忘录，这里重新读取
   * 设置与变更视图；阻塞与失败都只展示中文原因与指引，不改动任何内容。
   */
  const handlePull = () => {
    setSettingsBusy(true);
    const timer = pollSyncProgress();
    void api
      .pull_workspace()
      .then((outcome) => {
        setSync(outcome.status);
        setTheme(outcome.reload.theme);
        const notes = [outcome.message];
        if (outcome.theme) {
          notes.push(`生效主题：${outcome.reload.theme.selectedName}`);
        }
        if (outcome.memos.length > 0) {
          notes.push(`备忘录 ${outcome.memos.length} 篇（已重新读取）`);
        }
        setSettingsMessage({ level: "info", text: notes.join("；") });
      })
      .catch((error) => {
        setSettingsMessage({ level: "error", text: String(error) });
      })
      .finally(() => {
        stopSyncPolling(timer);
        setSettingsBusy(false);
        void api.get_settings().then(setSettings);
        void api.get_theme().then(setTheme);
        void loadChanges();
        void loadSync();
      });
  };

  /** 推送：只搬运已提交对象；「没有需要推送的提交」与鉴权失败分别提示。 */
  const handlePush = () => {
    setSettingsBusy(true);
    const timer = pollSyncProgress();
    void api
      .push_workspace()
      .then((outcome) => {
        setSync(outcome.status);
        setSettingsMessage({
          level: "info",
          text: `${outcome.message}（${outcome.updated
            .map((update) => `${update.local} → ${update.remote}`)
            .join("、")}）`,
        });
      })
      .catch((error) => {
        setSettingsMessage({ level: "error", text: String(error) });
      })
      .finally(() => {
        stopSyncPolling(timer);
        setSettingsBusy(false);
        void loadSync();
      });
  };

  const handleCancelSync = () => {
    void api.cancel_sync();
  };

  /**
   * 创建提交：范围只包含勾选的路径。成功与失败都以仓库真实状态刷新变更视图，
   * 失败原因原样展示，用户的修改不会因此丢失。
   */
  const handleCommit = async () => {
    setSettingsBusy(true);
    try {
      const outcome = await api.commit_changes(commitMessage, selectedPaths);
      applyChanges(outcome.changes);
      setCommitMessage("");
      setSettingsMessage({
        level: "info",
        text: `已创建提交 ${outcome.short}，包含 ${outcome.paths.length} 个文件：${outcome.paths.join("、")}`,
      });
    } catch (error) {
      setSettingsMessage({ level: "error", text: String(error) });
      // 失败后不猜测仓库状态，重新读取一次。
      await loadChanges();
    } finally {
      setSettingsBusy(false);
    }
  };

  const closeSettings = () => {
    setSettingsMessage(null);
    setScreen("search");
    focusInput();
  };

  /** 设置页的入口动作：进入前先建立一次基线。 */
  const runSettingsAction = async <T,>(action: () => Promise<T>, done: (result: T) => void | Promise<void>): Promise<boolean> => {
    setSettingsBusy(true);
    try {
      await done(await action());
      return true;
    } catch (error) {
      // 宿主返回的中文原因直接展示；不吞掉、不改写。
      setSettingsMessage({ level: "error", text: String(error) });
      return false;
    } finally {
      setSettingsBusy(false);
    }
  };

  const handleSelectWorkspace = (path: string) => {
    void runSettingsAction(
      () => api.select_workspace(path),
      (next) => {
        setWorkspace(next);
        setWorkspaceAlert(next.error);
        setSettingsMessage({
          level: "info",
          text: next.gitDir
            ? `已关联 Git 仓库：${next.path}`
            : `已关联目录（不是 Git 仓库）：${next.path}`,
        });
        void loadChanges();
        void loadSync();
      },
    );
  };

  const handleInitWorkspace = (path: string) => {
    void runSettingsAction(
      () => api.init_workspace(path),
      (next) => {
        setWorkspace(next);
        setWorkspaceAlert(next.error);
        setSettingsMessage({
          level: "info",
          text: `已初始化工作区与 Git 仓库：${next.path}`,
        });
        void loadChanges();
        void loadSync();
      },
    );
  };

  /**
   * 从远端克隆配置工作区：克隆在宿主侧阻塞执行，这里轮询进度快照，
   * 完成后刷新工作区与设置；失败、取消都只显示中文原因，不动当前工作区。
   */
  const handleCloneWorkspace = (
    url: string,
    target: string,
    token: { username: string; token: string } | null,
  ) => {
    setSettingsBusy(true);
    setCloneProgress(null);
    const timer = window.setInterval(() => {
      void api
        .clone_progress()
        .then((progress) => setCloneProgress(progress))
        .catch(() => {});
    }, 120);
    void api
      .clone_workspace(url, target, token)
      .then((outcome) => {
        setWorkspace(outcome.workspace);
        setWorkspaceAlert(outcome.workspace.error);
        void api.get_settings().then(setSettings);
        void loadChanges();
        void loadSync();
        const notes: string[] = [];
        if (outcome.recordedTheme) {
          notes.push(`工作区记录的主题：${outcome.recordedTheme}（主题支持由后续版本提供）`);
        }
        if (outcome.unavailablePlugins.length > 0) {
          notes.push(`本机没有这些插件：${outcome.unavailablePlugins.join("、")}`);
        }
        setSettingsMessage({
          level: "info",
          text:
            `已从远端克隆并关联工作区：${outcome.workspace.path}` +
            `（远端 ${outcome.remote.name}，分支 ${outcome.remote.branch}` +
            `${outcome.remote.upstream ? `，上游 ${outcome.remote.upstream}` : ""}）` +
            (notes.length > 0 ? `。${notes.join("；")}` : ""),
        });
      })
      .catch((error) => {
        setSettingsMessage({ level: "error", text: String(error) });
      })
      .finally(() => {
        window.clearInterval(timer);
        void api
          .clone_progress()
          .then((progress) => setCloneProgress(progress))
          .catch(() => {});
        setSettingsBusy(false);
      });
  };

  const handleCancelClone = () => {
    void api.cancel_clone();
  };

  const handleAppearanceChange = (appearance: import("./types").ThemeAppearance, style: string, reduce: boolean) => {
    void runSettingsAction(() => api.set_appearance_preferences(appearance, style, reduce), next => {
      setTheme(next);
      setSettingsMessage({ level: "info", text: "外观已保存" });
    });
  };

  const handleSelectTheme = (id: string) => {
    void runSettingsAction(
      () => api.select_theme(id),
      (next) => {
        setTheme(next);
        setSettingsMessage({
          level: "info",
          text: `已切换主题：${next.selectedName}（${
            next.appearance === "dark" ? "深色" : "浅色"
          }）`,
        });
      },
    );
  };

  const handleToggleTheme = (id: string, enabled: boolean) => {
    void runSettingsAction(
      () => api.set_plugin_enabled(id, enabled),
      (next) => {
        setTheme(next);
        setSettingsMessage({
          level: "info",
          text: `${enabled ? "已启用" : "已停用"}主题：${next.selectedName}`,
        });
      },
    );
  };

  const handleInstallTheme = (path: string) => {
    void runSettingsAction(
      () => api.install_theme(path),
      (next) => {
        setTheme(next);
        setSettingsMessage({
          level: "info",
          text: "主题包已安装",
        });
      },
    );
  };

  const handleRemoveTheme = (id: string) => {
    void runSettingsAction(
      () => api.remove_theme(id),
      (next) => {
        setTheme(next);
        setSettingsMessage({
          level: next.error ? "error" : "info",
          text: next.error ?? `已移除主题：${id}`,
        });
      },
    );
  };

  const handleSaveHotkey = (hotkey: string) => {
    if (!settings) {
      setSettingsMessage({ level: "error", text: "设置尚未加载完成" });
      return;
    }
    void runSettingsAction(
      () => api.set_settings({ ...settings, hotkey }),
      (hotkeyStatus) => {
        setSettings({ ...settings, hotkey: hotkeyStatus.label });
        setStatus((current) => (current ? { ...current, hotkey: hotkeyStatus } : current));
        setSettingsMessage({
          level: "info",
          text: hotkeyStatus.registered
            ? `快捷键已立即生效：${hotkeyStatus.label}`
            : `快捷键 ${hotkeyStatus.label} 注册失败：${hotkeyStatus.error ?? "原因未知"}`,
        });
      },
    );
  };

  const runQuery = (value: string) => {
    setInput(value);
    void api.query(value).then(apply);
  };

  const runExecute = async (item: ItemView) => {
    const outcome = await api.execute(item.id);
    if (outcome.status === "failed") {
      // 失败必须给出可理解的中文反馈。
      setFeedback(outcome);
      return;
    }
    setFeedback(outcome.message ? outcome : null);
    if (outcome.status === "done" && !outcome.message) {
      // 成功启动真实软件后窗口由外壳隐藏。
      setVisible(false);
      return;
    }
    if (item.kind === "command") {
      // 命令条目会改变宿主的查询状态（重新扫描软件、进入插件范围）：
      // 必须按当前输入重新读取结果，否则列表与宿主状态不一致。
      void api.query(input).then(apply);
    }
    // `pastePending`：宿主已准备好剪贴板，正在等待外壳关窗并注入粘贴。窗口由外壳关闭，
    // 这里只显示反馈、不隐藏（真实外壳在关窗后会以最终状态更新同一块反馈区）。
  };

  const switchScope = async (plugin: PluginView | null) => {
    const keyword = plugin ? plugin.keywords[0] ?? plugin.name : "";
    setInput(keyword);
    focusInput();
    const next = await api.query(keyword);
    apply(next);
    if (plugin && next.scope.kind === "home" && next.seq === appliedSeq.current) {
      const entry = next.items.find(item => item.id === `flashcast.plugin.${plugin.id}`);
      if (entry) {
        const outcome = await api.execute(entry.id);
        if (outcome.status === "failed") setFeedback(outcome);
        else apply(await api.query(keyword));
      }
    }
  };

  /**
   * 搜索界面的键盘入口：挂在 `window` 上，**不是**挂在输入框上。
   *
   * 鼠标点过任何一行之后焦点就落到 body（结果行不可聚焦），这时绑在输入框上的
   * `onKeyDown` 不会再触发——↑↓ 选择、Enter 执行、Escape 关闭会一起失效，而这是
   * 键盘优先的启动器最不该发生的事。设置页早就因为同样的原因把 Escape 挂到了
   * `window` 上（见 `SettingsScreen` 里的注释），这里与它保持一致。
   *
   * 处理函数存在 ref 里：监听器只订阅一次，不会因为每次按键都重挂。
   */
  const keyHandlerRef = useRef<(event: KeyboardEvent) => void>(() => {});
  keyHandlerRef.current = (event: KeyboardEvent) => {
    // 中文输入法组合期间不执行、不移动选择：回车是确认候选词。
    if (composingRef.current || event.isComposing || composing) {
      return;
    }
    // 设置页有自己的 window 级 Escape（返回搜索首屏），这里完全不接管。
    if (screen !== "search") {
      return;
    }
    // Ctrl+, 打开设置（与常见桌面应用一致）。
    if (event.ctrlKey && event.key === ",") {
      event.preventDefault();
      openSettings();
      return;
    }
    const target = event.target as HTMLElement | null;
    // 除搜索框以外的可编辑控件自己处理按键（设置页的路径、正文等，搜索页目前没有）。
    const editable =
      target !== null &&
      (/^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName) || target.isContentEditable);
    if (editable && target !== inputRef.current) {
      return;
    }
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        void api.move_selection(1).then(apply);
        break;
      case "ArrowUp":
        event.preventDefault();
        void api.move_selection(-1).then(apply);
        break;
      case "Enter": {
        // 焦点在按钮上时 Enter 是原生的「按下这个按钮」，不要再执行选中的结果。
        if (target?.closest("button, a, [role='button']") != null) {
          break;
        }
        event.preventDefault();
        const item = response.items[response.selection];
        if (item) {
          void runExecute(item);
        }
        break;
      }
      case "Escape": {
        event.preventDefault();
        void handleEscape();
        break;
      }
      default:
        break;
    }
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => keyHandlerRef.current(event);
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, []);

  const handleEscape = async () => {
    // 优先级：关闭菜单 → 返回上一查询范围 → 关闭窗口。
    if (menuOpen) {
      return;
    }
    if (feedback) {
      setFeedback(null);
      return;
    }
    const back = await api.back();
    apply(back.response);
    if (back.restored) {
      // 返回后的查询由宿主决定（例如从插件范围退回首屏会回到进入范围前的输入）：
      // 输入框必须回显它，否则界面上的查询与列表会对不上。
      setInput(back.response.input);
      focusInput();
      return;
    }
    await api.hide_window();
    setVisible(false);
  };

  const selected = response.items[response.selection] ?? null;
  const selectedId = selected?.id ?? null;

  // 预览按选中项按需请求：备忘录给出完整正文，其它条目类型返回 null。
  // 依赖只有选中项的 **id**：同一输入的重复渲染不会重复请求，也不会重置展开状态。
  useEffect(() => {
    if (selectedId === null) {
      setPreview(null);
      return;
    }
    let cancelled = false;
    setPreviewOpen(true);
    void api
      .preview(selectedId)
      .then((next) => {
        if (!cancelled) {
          setPreview(next);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setPreview(null);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const scopeLabel = useMemo(() => response.scopeLabel, [response.scopeLabel]);

  return (
    <div
      className="app"
      data-testid="app-root"
      data-window-visible={visible ? "true" : "false"}
      data-focused={focused ? "true" : "false"}
      data-screen={screen}
      data-browser-preview={isBrowserMock ? "true" : "false"}
    >
      <header className="search-row">
        <svg className="search-symbol" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.6"><circle cx="10.5" cy="10.5" r="6.5"/><path d="m16 16 4.5 4.5"/></svg>
        <input
          ref={inputRef}
          id="search-input"
          data-testid="search-input"
          className="search-input"
          type="text"
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          placeholder="搜索软件、备忘录、剪贴板…"
          value={input}
          aria-label="搜索"
          aria-controls="result-list"
          aria-activedescendant={selected ? `item-${selected.id}` : undefined}
          onChange={(event) => runQuery(event.target.value)}
          onFocus={() => setFocused(true)}
          onBlur={() => setFocused(false)}
          onCompositionStart={() => {
            composingRef.current = true;
            setComposing(true);
          }}
          onCompositionEnd={() => {
            composingRef.current = false;
            setComposing(false);
          }}
        />
        <span className="scope-label" data-testid="scope-label">
          {scopeLabel}
        </span>
        <button
          type="button"
          className="ghost-button"
          data-testid="open-settings"
          aria-label="设置"
          title="设置（Ctrl+,）"
          onClick={openSettings}
        >
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" aria-hidden="true"><path d="m9 3-1 3-3 1v4l-2 1 2 1v4l3 1 1 3h6l1-3 3-1v-4l2-1-2-1V7l-3-1-1-3Z"/><circle cx="12" cy="12" r="3"/></svg>
        </button>
      </header>

      {screen === "settings" ? (
        <SettingsScreen
          workspace={workspace}
          settings={settings}
          theme={theme}
          materialNotice={materialNotice}
          onAppearanceChange={handleAppearanceChange}
          hotkey={status?.hotkey ?? null}
          capabilities={capabilities}
          message={settingsMessage}
          busy={settingsBusy}
          cloneProgress={cloneProgress}
          onBack={closeSettings}
          changes={changes}
          sync={sync}
          syncProgress={syncProgress}
          commitMessage={commitMessage}
          selectedPaths={selectedPaths}
          diffPath={diffPath}
          plugins={plugins}
          memos={memos}
          memoProblems={memoProblems}
          memoEnabled={memoPlugin?.enabled ?? false}
          chrome={chrome}
          clipboard={clipboard}
          onSelectWorkspace={handleSelectWorkspace}
          onInitWorkspace={handleInitWorkspace}
          onSaveHotkey={handleSaveHotkey}
          onCloneWorkspace={handleCloneWorkspace}
          onCancelClone={handleCancelClone}
          onSelectTheme={handleSelectTheme}
          onToggleTheme={handleToggleTheme}
          onInstallTheme={handleInstallTheme}
          onRemoveTheme={handleRemoveTheme}
          onTogglePath={handleTogglePath}
          onToggleAllPaths={handleToggleAllPaths}
          onSelectDiff={setDiffPath}
          onCommitMessageChange={setCommitMessage}
          onCommit={() => void handleCommit()}
          onRefreshChanges={handleRefreshChanges}
          onPull={handlePull}
          onPush={handlePush}
          onRedetectSync={handleRedetectSync}
          onCancelSync={handleCancelSync}
          onToggleFeaturePlugin={handleToggleFeaturePlugin}
          onAssociateChromeProfile={handleAssociateChromeProfile}
          onRefreshChromeBookmarks={handleRefreshChromeBookmarks}
          onCreateMemo={handleCreateMemo}
          onUpdateMemo={handleUpdateMemo}
          onDeleteMemo={handleDeleteMemo}
          onToggleClipboardPaused={handleToggleClipboardPaused}
          onSaveClipboardLimits={handleSaveClipboardLimits}
          onPinClipboardEntry={handlePinClipboardEntry}
          onDeleteClipboardEntry={handleDeleteClipboardEntry}
          onClearClipboardHistory={handleClearClipboardHistory}
          onSaveClipboardFileCopy={handleSaveClipboardFileCopy}
          current={settingsSection}
          onSectionChange={setSettingsSection}
        />
      ) : (
        <>
          <StatusBanner
            status={status}
            response={response}
            feedback={feedback}
            workspaceAlert={workspaceAlert}
          />

          <div className="scope-strip" aria-label="查询范围">
            <button type="button" aria-pressed={response.scope.kind === "home"} onClick={() => void switchScope(null)}>快速访问</button>
            {plugins.filter(plugin => plugin.enabled).map(plugin => <button key={plugin.id} type="button"
              aria-pressed={response.scope.kind === "plugin" && response.scope.id === plugin.id}
              onClick={() => void switchScope(plugin)}>{plugin.name}</button>)}
          </div>
          <div className="search-body" data-has-preview={selected && ["memo", "bookmark", "clipboardEntry"].includes(selected.kind) ? "true" : "false"}>
            <ResultList
              items={response.items}
              selection={response.selection}
              query={response.input}
              onActivate={(item) => void runExecute(item)}
            />

            <aside className="stage" data-testid="stage">
              {selected &&
              (selected.kind === "memo" ||
                selected.kind === "bookmark" ||
                selected.kind === "clipboardEntry") ? (
                <MemoPreview
                  item={selected}
                  preview={preview}
                  open={previewOpen}
                  onToggle={() => setPreviewOpen((current) => !current)}
                />
              ) : null}
            </aside>
          </div>

          <ActionBar selected={selected} count={response.items.length} />
        </>
      )}

      {isBrowserMock ? (
        /* 只在浏览器替身里渲染（Tauri 内 `isBrowserMock` 为 false）。
           真实外壳在启动软件后关窗，用户靠全局快捷键或托盘再唤起；浏览器里没有唤回
           入口，窗口一旦隐藏就是一整页空白。这里留一条可发现的回程，方便手动检查。 */
        <button
          type="button"
          className="browser-recall"
          data-testid="browser-recall"
          onClick={() => window.__flashcastMock?.summon()}
        >
          模拟全局快捷键：唤起窗口
        </button>
      ) : null}
    </div>
  );
}
