import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type {
  ActionOutcome,
  Appearance,
  CloneProgress,
  ItemView,
  QueryView,
  Settings,
  StatusView,
  SyncProgress,
  SyncStatus,
  ThemeState,
  WorkspaceChanges,
  WorkspaceEvent,
  WorkspaceStatus,
} from "./types";
import { ActionBar } from "./components/ActionBar";
import { ResultList } from "./components/ResultList";
import { SettingsScreen, type SettingsMessage } from "./components/SettingsScreen";
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
  const [settingsMessage, setSettingsMessage] = useState<SettingsMessage | null>(null);
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
    void api.get_status().then(setStatus);
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
    if (theme) {
      applyThemeVars(theme);
    }
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
  };

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
  const runSettingsAction = async <T,>(action: () => Promise<T>, done: (result: T) => void) => {
    setSettingsBusy(true);
    try {
      done(await action());
    } catch (error) {
      // 宿主返回的中文原因直接展示；不吞掉、不改写。
      setSettingsMessage({ level: "error", text: String(error) });
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
          text: `已安装主题包：${next.themes.map((entry) => entry.name).join("、")}`,
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
      // 启动失败必须给出可理解的中文反馈。
      setFeedback(outcome);
      return;
    }
    setFeedback(outcome.message ? outcome : null);
    if (outcome.status === "done" && !outcome.message) {
      // 成功启动真实软件后窗口由外壳隐藏。
      setVisible(false);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    // 中文输入法组合期间不执行、不移动选择：回车是确认候选词。
    if (composingRef.current || event.nativeEvent.isComposing || composing) {
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
      case ",": {
        // Ctrl+, 打开设置（与常见桌面应用一致）。
        if (event.ctrlKey) {
          event.preventDefault();
          openSettings();
        }
        break;
      }
      default:
        break;
    }
  };

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
    if (!back.restored) {
      await api.hide_window();
      setVisible(false);
    }
  };

  const selected = response.items[response.selection] ?? null;
  const scopeLabel = useMemo(() => response.scopeLabel, [response.scopeLabel]);

  return (
    <div
      className="app"
      data-testid="app-root"
      data-window-visible={visible ? "true" : "false"}
      data-focused={focused ? "true" : "false"}
      data-screen={screen}
    >
      <header className="search-row">
        <input
          id="search-input"
          data-testid="search-input"
          className="search-input"
          type="text"
          autoComplete="off"
          autoCorrect="off"
          spellCheck={false}
          placeholder="搜索软件…"
          value={input}
          aria-label="搜索"
          aria-controls="result-list"
          aria-activedescendant={selected ? `item-${selected.id}` : undefined}
          onChange={(event) => runQuery(event.target.value)}
          onKeyDown={handleKeyDown}
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
          设置
        </button>
      </header>

      {screen === "settings" ? (
        <SettingsScreen
          workspace={workspace}
          settings={settings}
          theme={theme}
          hotkey={status?.hotkey ?? null}
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
        />
      ) : (
        <>
          <StatusBanner
            status={status}
            response={response}
            feedback={feedback}
            workspaceAlert={workspaceAlert}
          />

          <ResultList
            items={response.items}
            selection={response.selection}
            onActivate={(item) => void runExecute(item)}
          />

          <ActionBar selected={selected} count={response.items.length} />
        </>
      )}
    </div>
  );
}
