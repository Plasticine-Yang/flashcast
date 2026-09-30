import { useEffect, useMemo, useRef, useState } from "react";
import { api } from "./api";
import type { ActionOutcome, ItemView, QueryView, StatusView } from "./types";
import { ActionBar } from "./components/ActionBar";
import { ResultList } from "./components/ResultList";
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
    void api.refresh_state().then(apply);
    void api.get_status().then(setStatus);
    const unlisteners: (() => void)[] = [];
    const register = async () => {
      unlisteners.push(
        await api.on("flashcast://summoned", () => {
          setVisible(true);
          setFeedback(null);
          setInput("");
          void api.refresh_state().then(apply);
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
      </header>

      <StatusBanner status={status} response={response} feedback={feedback} />

      <ResultList
        items={response.items}
        selection={response.selection}
        onActivate={(item) => void runExecute(item)}
      />

      <ActionBar selected={selected} count={response.items.length} />
    </div>
  );
}
