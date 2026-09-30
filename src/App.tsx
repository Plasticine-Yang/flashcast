import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type {
  ActionOutcome,
  ItemView,
  QueryView,
  Settings,
  StatusView,
  WorkspaceEvent,
  WorkspaceStatus,
} from "./types";
import { ActionBar } from "./components/ActionBar";
import { ResultList } from "./components/ResultList";
import { SettingsScreen, type SettingsMessage } from "./components/SettingsScreen";
import { StatusBanner } from "./components/StatusBanner";

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
  const [settingsMessage, setSettingsMessage] = useState<SettingsMessage | null>(null);
  const [workspaceAlert, setWorkspaceAlert] = useState<string | null>(null);
  const [settingsBusy, setSettingsBusy] = useState(false);

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
      // 工作区外部修改：有效则已生效，无效则保留上次有效状态并显示原因。
      unlisteners.push(
        await api.on("flashcast://workspace", (payload) => {
          const event = payload as WorkspaceEvent;
          setWorkspace(event.status);
          setSettings(event.settings);
          setWorkspaceAlert(event.status.error);
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
    return () => {
      for (const unlisten of unlisteners) {
        unlisten();
      }
    };
  }, []);

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
          hotkey={status?.hotkey ?? null}
          message={settingsMessage}
          busy={settingsBusy}
          onBack={closeSettings}
          onSelectWorkspace={handleSelectWorkspace}
          onInitWorkspace={handleInitWorkspace}
          onSaveHotkey={handleSaveHotkey}
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
