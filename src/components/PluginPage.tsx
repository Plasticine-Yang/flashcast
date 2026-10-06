import { useEffect, useState } from "react";
import type {
  ClipboardStateView,
  ItemView,
  Memo,
  MemoProblem,
  Preview,
} from "../types";
import { parseTags } from "../memoTags";
import { ResultList } from "./ResultList";
import { MemoPreview } from "./MemoPreview";
import { Glyph } from "./Glyph";
import { memoSummary } from "../memoSummary";

interface Props {
  id: string;
  title: string;
  items: ItemView[];
  selection: number;
  query: string;
  preview: Preview | null;
  memos: Memo[];
  problems: MemoProblem[];
  clipboard: ClipboardStateView | null;
  workspaceKey: string | null;
  busy: boolean;
  menuOpen: boolean;
  message: { level: string; text: string } | null;
  onMenu: (open: boolean) => void;
  onBack: () => void;
  onSelect: (index: number) => void;
  onExecute: (item: ItemView) => void;
  onEditing: (value: boolean) => void;
  onCreate: (title: string, tags: string[], body: string) => Promise<boolean>;
  onUpdate: (
    id: string,
    title: string,
    tags: string[],
    body: string,
  ) => Promise<boolean>;
  onDelete: (id: string) => Promise<boolean>;
  onPin: (id: string, pinned: boolean) => void;
  onDeleteClipboard: (id: string) => void;
  onClear: () => void;
  onWorkspace: () => void;
}
interface Draft {
  id: string | null;
  tags: string;
  body: string;
}
export function PluginPage(p: Props) {
  const [draft, setDraft] = useState<Draft | null>(null);
  const [confirm, setConfirm] = useState<"delete" | "clear" | null>(null);
  useEffect(() => {
    // 等父级解除搜索框的 disabled，再恢复键盘入口。
    const frame = requestAnimationFrame(() => {
      if (p.menuOpen)
        document.querySelector<HTMLElement>(
          '.action-tray [data-testid="confirm-delete"], .action-tray [role=menuitem]:not(:disabled)',
        )?.focus();
      else if (!draft) document.getElementById("search-input")?.focus();
    });
    return () => cancelAnimationFrame(frame);
  }, [p.menuOpen, !!draft, confirm]);
  const selected = p.items[p.selection];
  const memo = p.memos.find((m) => `memo:${m.id}` === selected?.id);
  const entry = p.clipboard?.items.find(
    (e) => `clipboard:${e.id}` === selected?.id,
  );
  const writable = p.id === "memo" && !!p.workspaceKey && !p.busy;
  useEffect(() => {
    setDraft(null);
    setConfirm(null);
    p.onMenu(false);
  }, [p.id, p.workspaceKey]);
  useEffect(() => {
    setConfirm(null);
  }, [selected?.id]);
  useEffect(() => {
    p.onEditing(!!draft);
    return () => p.onEditing(false);
  }, [!!draft, p.onEditing]);
  const startNew = () => {
    p.onMenu(false);
    setDraft({ id: null, tags: "", body: "" });
  };
  const startEdit = () => {
    if (memo && writable) {
      p.onMenu(false);
      setDraft({
        id: memo.id,
        tags: memo.tags.join("、"),
        body: memo.body,
      });
    }
  };
  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if (e.isComposing) return;
      if (e.key === "Escape" && (draft || p.menuOpen || confirm)) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (!p.busy) {
          if (draft) setDraft(null);
          else {
            setConfirm(null);
            p.onMenu(false);
          }
        }
      }
      if (
        !draft &&
        (e.ctrlKey || e.metaKey) &&
        writable &&
        ["n", "e"].includes(e.key.toLowerCase())
      ) {
        e.preventDefault();
        e.stopImmediatePropagation();
        if (e.key.toLowerCase() === "n") startNew();
        else startEdit();
      }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [draft, p.menuOpen, confirm, memo, writable, p.busy]);
  const save = async () => {
    if (!draft || p.busy || !draft.body.trim()) return;
    const ok = draft.id
      ? await p.onUpdate(
          draft.id,
          memoSummary(draft.body),
          parseTags(draft.tags),
          draft.body,
        )
      : await p.onCreate(
          memoSummary(draft.body),
          parseTags(draft.tags),
          draft.body,
        );
    if (ok) setDraft(null);
  };
  const remove = async () => {
    if (confirm !== "delete") {
      setConfirm("delete");
      return;
    }
    if (memo) {
      if (await p.onDelete(memo.id)) {
        setConfirm(null);
        p.onMenu(false);
      }
    } else if (entry) {
      p.onDeleteClipboard(entry.id);
      setConfirm(null);
      p.onMenu(false);
    }
  };
  return (
    <section
      className="plugin-page"
      data-testid="plugin-page"
      data-plugin={p.id}
    >
      <header className="plugin-page-header">
        <button
          className="ghost-button navigation-back plugin-back"
          aria-label="返回主搜索"
          title="返回主搜索"
          onClick={p.onBack}
        >
          <Glyph name="back" />
        </button>
        <h1>{p.title}</h1>
        <span>{p.items.length} 条</span>
        {p.id === "memo" ? (
          <button
            className="primary-button"
            data-testid="memo-new"
            disabled={!writable}
            onClick={startNew}
          >
            新建
          </button>
        ) : null}
      </header>
      {p.id === "clipboard" &&
      (p.clipboard?.storageError ||
        p.clipboard?.lastError ||
        p.clipboard?.capacityReached) ? (
        <details className="clipboard-warning" data-testid="clipboard-warning">
          <summary>
            {p.clipboard.storageError
              ? "历史存储不可用"
              : p.clipboard.lastError
                ? "后台记录不可用"
                : "历史容量已满"}
          </summary>
          <p>
            {p.clipboard.storageError ??
              p.clipboard.lastError ??
              p.clipboard.capacityReached}
          </p>
        </details>
      ) : null}
      {p.message ? (
        <p
          className={`plugin-message ${p.message.level}`}
          role={p.message.level === "error" ? "alert" : "status"}
        >
          {p.message.text}
        </p>
      ) : null}
      {p.id === "memo" && !p.workspaceKey ? (
        <div className="memo-workspace-notice">
          关联配置工作区后，创建和保存备忘录。
          <button className="secondary-button" onClick={p.onWorkspace}>
            关联工作区
          </button>
        </div>
      ) : null}
      <div className="plugin-columns">
        <ResultList
          items={p.items}
          memos={p.memos}
          selection={p.selection}
          query={p.query}
          onSelect={p.onSelect}
          onActivate={p.onExecute}
        />
        <aside className="plugin-detail" data-testid="stage">
          {selected ? (
            <>
              <MemoPreview
                item={selected}
                preview={p.preview}
                open={true}
                onToggle={() => {}}
              />
              {p.id === "memo" ? (
                <div className="memo-detail-actions">
                  <button
                    className="secondary-button"
                    data-testid="memo-edit"
                    disabled={!memo || !writable}
                    onClick={startEdit}
                  >
                    <Glyph name="edit" />编辑
                  </button>
                  <button
                    className="ghost-button danger-button"
                    data-testid="memo-delete"
                    disabled={!memo || !writable}
                    onClick={() => {
                      p.onMenu(true);
                      setConfirm("delete");
                    }}
                  >
                    <Glyph name="trash" />删除
                  </button>
                </div>
              ) : (
                <div className="plugin-primary">
                  <button
                    className="primary-button"
                    disabled={p.busy}
                    onClick={() => p.onExecute(selected)}
                  >
                    {selected.defaultActionLabel} <kbd>↵</kbd>
                  </button>
                </div>
              )}
            </>
          ) : (
            <div className="content-empty">
              <strong>
                {p.id === "clipboard"
                  ? "复制的内容，会出现在这里"
                  : "没有匹配的内容"}
              </strong>
              <p>
                {p.query
                  ? "试试其他关键词"
                  : p.id === "memo"
                    ? "新建第一条备忘录"
                    : "选择左侧内容，查看完整预览"}
              </p>
            </div>
          )}
        </aside>
      </div>
      {p.problems.length > 0 && p.id === "memo" ? (
        <details className="memo-read-problems">
          <summary>{p.problems.length} 个文件无法读取</summary>
          {p.problems.map((v) => (
            <p key={v.path}>
              {v.path}：{v.reason}
            </p>
          ))}
        </details>
      ) : null}
      {p.menuOpen ? (
        <>
          <button
            className="tray-dismiss"
            aria-label="关闭动作"
            onClick={() => {
              setConfirm(null);
              p.onMenu(false);
            }}
          />
          <div
            className="action-tray"
            role="menu"
            onKeyDown={(e) => {
              if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key))
                return;
              e.preventDefault();
              e.stopPropagation();
              const buttons = Array.from(
                e.currentTarget.querySelectorAll<HTMLButtonElement>(
                  '[role="menuitem"]:not(:disabled)',
                ),
              );
              const at = buttons.indexOf(
                document.activeElement as HTMLButtonElement,
              );
              buttons[
                e.key === "Home"
                  ? 0
                  : e.key === "End"
                    ? buttons.length - 1
                    : (at + (e.key === "ArrowDown" ? 1 : -1) + buttons.length) %
                      buttons.length
              ]?.focus();
            }}
            aria-label="动作"
            data-testid="action-tray"
          >
            <div className="tray-heading">
              {confirm ? "确认操作" : "动作"}
              <button
                className="ghost-button"
                onClick={() => {
                  setConfirm(null);
                  p.onMenu(false);
                }}
              >
                Esc
              </button>
            </div>
            {confirm ? (
              <>
                <p>
                  {confirm === "clear"
                    ? "清空全部剪切板历史？此操作无法撤销。"
                    : `删除「${memo ? memoSummary(memo.body) : selected?.title}」？此操作无法撤销。`}
                </p>
                <button
                  className="danger-button secondary-button"
                  disabled={p.busy}
                  data-testid="confirm-delete"
                  onClick={() => {
                    if (confirm === "clear") {
                      p.onClear();
                      setConfirm(null);
                      p.onMenu(false);
                    } else void remove();
                  }}
                >
                  确认{confirm === "clear" ? "清空" : "删除"}
                </button>
                <button
                  className="ghost-button"
                  onClick={() => setConfirm(null)}
                >
                  取消
                </button>
              </>
            ) : (
              <>
                {selected ? (
                  <button
                    role="menuitem"
                    onClick={() => {
                      p.onMenu(false);
                      p.onExecute(selected);
                    }}
                  >
                    {selected.defaultActionLabel}
                    <kbd>↵</kbd>
                  </button>
                ) : null}
                {p.id === "memo" ? (
                  <>
                    <button
                      role="menuitem"
                      disabled={!writable}
                      onClick={startNew}
                    >
                      新建备忘录<kbd>⌃ N</kbd>
                    </button>
                    <button
                      role="menuitem"
                      data-testid="memo-edit-menu"
                      disabled={!memo || !writable}
                      onClick={startEdit}
                    >
                      编辑备忘录<kbd>⌃ E</kbd>
                    </button>
                  </>
                ) : null}
                {entry ? (
                  <button
                    role="menuitem"
                    disabled={p.busy}
                    onClick={() => {
                      p.onPin(entry.id, !entry.pinned);
                      p.onMenu(false);
                    }}
                  >
                    {entry.pinned ? "取消置顶" : "置顶"}
                  </button>
                ) : null}
                {memo || entry ? (
                  <button
                    className="danger-button"
                    role="menuitem"
                    disabled={p.busy || (!!memo && !writable)}
                    data-testid="delete-menu"
                    onClick={() => void remove()}
                  >
                    删除
                  </button>
                ) : null}
                {p.id === "clipboard" ? (
                  <button
                    className="danger-button"
                    role="menuitem"
                    disabled={p.busy || !p.items.length}
                    onClick={() => setConfirm("clear")}
                  >
                    清空历史
                  </button>
                ) : null}
              </>
            )}
          </div>
        </>
      ) : null}
      {draft ? (
        <div className="memo-modal-backdrop">
          <form
            className="memo-modal"
            role="dialog"
            aria-modal="true"
            aria-label={draft.id ? "编辑备忘录" : "新建备忘录"}
            data-testid="memo-editor"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
            onKeyDown={(e) => {
              if (
                (e.ctrlKey || e.metaKey) &&
                e.key === "Enter" &&
                !e.nativeEvent.isComposing
              ) {
                e.preventDefault();
                e.stopPropagation();
                void save();
              }
              if (e.key === "Tab") {
                const nodes = Array.from(
                  e.currentTarget.querySelectorAll<HTMLElement>(
                    "input,textarea,button:not(:disabled)",
                  ),
                );
                const first = nodes[0],
                  last = nodes[nodes.length - 1];
                if (e.shiftKey && document.activeElement === first) {
                  e.preventDefault();
                  last.focus();
                } else if (!e.shiftKey && document.activeElement === last) {
                  e.preventDefault();
                  first.focus();
                }
              }
            }}
          >
            <header>
              <h2>{draft.id ? "编辑备忘录" : "新建备忘录"}</h2>
              <button
                className="ghost-button"
                type="button"
                disabled={p.busy}
                onClick={() => setDraft(null)}
              >
                取消
              </button>
            </header>
            {p.message?.level === "error" ? (
              <p role="alert" className="plugin-message error">
                {p.message.text}
              </p>
            ) : null}
            <label className="memo-body-field">
              正文
              <textarea
                disabled={p.busy}
                autoFocus
                required
                placeholder="写下要记住的内容…"
                data-testid="memo-body"
                value={draft.body}
                onChange={(e) => setDraft({ ...draft, body: e.target.value })}
              />
            </label>
            <label>
              <span>标签 <small>可选</small></span>
              <input
                disabled={p.busy}
                data-testid="memo-tags"
                placeholder="工作、常用回复"
                value={draft.tags}
                onChange={(e) => setDraft({ ...draft, tags: e.target.value })}
              />
            </label>
            <footer>
              <span>
                <kbd>Esc</kbd> 取消
              </span>
              <button
                className="primary-button"
                data-testid="memo-save"
                disabled={p.busy || !draft.body.trim()}
              >
                {p.busy ? "保存中…" : "保存备忘录"}
              </button>
            </footer>
          </form>
        </div>
      ) : null}
    </section>
  );
}
