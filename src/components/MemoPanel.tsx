import { useEffect, useState } from "react";
import type { Memo, MemoProblem } from "../types";

/** 表单草稿：标签用界面上的分隔符写法（逗号或顿号）输入。 */
interface Draft {
  id: string | null;
  title: string;
  tags: string;
  body: string;
}

const EMPTY_DRAFT: Draft = { id: null, title: "", tags: "", body: "" };

/** 标签分隔符：中英文逗号与顿号都接受，与工作区 Markdown 的写法一致。 */
export function parseTags(value: string): string[] {
  const tags: string[] = [];
  for (const part of value.split(/[,，、]/)) {
    const tag = part.trim();
    if (tag.length > 0 && !tags.includes(tag)) {
      tags.push(tag);
    }
  }
  return tags;
}

interface Props {
  /** 是否已关联配置工作区：未关联时不能保存。 */
  linked: boolean;
  /** 备忘录插件是否启用：停用时管理入口不提供写入。 */
  enabled: boolean;
  memos: Memo[];
  /** 无法读取的备忘录文件：如实展示原因，不假装不存在。 */
  problems: MemoProblem[];
  busy: boolean;
  onCreate: (title: string, tags: string[], body: string) => Promise<void>;
  onUpdate: (id: string, title: string, tags: string[], body: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

/**
 * 设置页里的备忘录管理：列出工作区 `memos/` 下的备忘录，创建、编辑、删除。
 *
 * 延续紧凑列表的视觉语言（`styles.css` 的设计令牌、无 CSS 框架、无动画）：
 * 保存与删除都是低频操作，但这里连过渡也不加，保持与其它设置区段一致。
 */
export function MemoPanel({
  linked,
  enabled,
  memos,
  problems,
  busy,
  onCreate,
  onUpdate,
  onDelete,
}: Props) {
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);

  // 工作区或插件状态变化（例如切换工作区、停用插件）时放弃未保存的草稿，
  // 避免把上一个工作区的输入写到新工作区。
  useEffect(() => {
    setDraft(EMPTY_DRAFT);
  }, [linked, enabled]);

  const writable = linked && enabled && !busy;
  const editing = draft.id !== null;

  const startCreate = () => setDraft(EMPTY_DRAFT);

  const startEdit = (memo: Memo) =>
    setDraft({
      id: memo.id,
      title: memo.title,
      tags: memo.tags.join("、"),
      body: memo.body,
    });

  const save = async () => {
    const tags = parseTags(draft.tags);
    if (editing && draft.id) {
      await onUpdate(draft.id, draft.title, tags, draft.body);
    } else {
      await onCreate(draft.title, tags, draft.body);
    }
    setDraft(EMPTY_DRAFT);
  };

  return (
    <section className="settings-section" data-testid="memo-section">
      <h2 className="settings-section-title">备忘录</h2>

      {!enabled ? (
        <div className="banner banner-warning" data-testid="memo-disabled" role="status">
          备忘录插件已停用：搜索里不会出现备忘录结果，也不能创建或修改备忘录。
          在上方「功能插件」区段重新启用后即可管理。
        </div>
      ) : null}
      {!linked ? (
        <div className="banner banner-warning" data-testid="memo-no-workspace" role="status">
          尚未关联配置工作区：备忘录保存在工作区的 <code>memos/&lt;标识&gt;.md</code> 里，
          请先在「配置工作区」区段关联或初始化一个工作区。
        </div>
      ) : null}

      <ul className="memo-list" data-testid="memo-list">
        {memos.length === 0 ? (
          <li className="memo-empty" data-testid="memo-empty">
            工作区里还没有备忘录。
          </li>
        ) : (
          memos.map((memo) => (
            <li
              className="memo-item"
              data-testid="memo-item"
              data-memo-id={memo.id}
              data-editing={draft.id === memo.id ? "true" : "false"}
              key={memo.id}
            >
              <span className="memo-text">
                <span className="memo-title" data-testid="memo-item-title">
                  {memo.title}
                </span>
                <span className="memo-meta" data-testid="memo-item-tags">
                  {memo.tags.length > 0 ? `标签：${memo.tags.join("、")}` : "无标签"}
                  {` · ${memo.id}`}
                </span>
                <span className="memo-body-preview" data-testid="memo-item-body">
                  {memo.body}
                </span>
              </span>
              <button
                type="button"
                className="secondary-button"
                data-testid="memo-edit"
                disabled={!writable}
                onClick={() => startEdit(memo)}
              >
                编辑
              </button>
              <button
                type="button"
                className="secondary-button"
                data-testid="memo-delete"
                disabled={!writable}
                onClick={() => void onDelete(memo.id)}
              >
                删除
              </button>
            </li>
          ))
        )}
      </ul>

      {problems.length > 0 ? (
        <ul className="memo-problems" data-testid="memo-problems">
          {problems.map((problem) => (
            <li className="memo-problem" data-testid="memo-problem" key={problem.path}>
              {problem.path}：{problem.reason}
            </li>
          ))}
        </ul>
      ) : null}

      <h3 className="settings-section-title">
        {editing ? `编辑备忘录：${draft.id}` : "新建备忘录"}
      </h3>
      <label className="settings-label" htmlFor="memo-title">
        标题
      </label>
      <div className="settings-row">
        <input
          id="memo-title"
          className="path-input"
          data-testid="memo-title-input"
          type="text"
          autoComplete="off"
          placeholder="常用回复"
          value={draft.title}
          onChange={(event) => setDraft({ ...draft, title: event.target.value })}
        />
      </div>
      <label className="settings-label" htmlFor="memo-tags">
        标签（用逗号或顿号分隔多个标签；首屏输入完整标签即可命中）
      </label>
      <div className="settings-row">
        <input
          id="memo-tags"
          className="path-input"
          data-testid="memo-tags-input"
          type="text"
          autoComplete="off"
          placeholder="回复、工作"
          value={draft.tags}
          onChange={(event) => setDraft({ ...draft, tags: event.target.value })}
        />
      </div>
      <label className="settings-label" htmlFor="memo-body">
        正文（保存为工作区里的 Markdown，可以直接用编辑器维护）
      </label>
      <div className="settings-row">
        <textarea
          id="memo-body"
          className="path-input memo-body-input"
          data-testid="memo-body-input"
          rows={4}
          spellCheck={false}
          placeholder="收到，我看一下再回复你。"
          value={draft.body}
          onChange={(event) => setDraft({ ...draft, body: event.target.value })}
        />
      </div>
      <div className="settings-row">
        <button
          type="button"
          className="primary-button"
          data-testid="memo-save"
          disabled={!writable}
          onClick={() => void save()}
        >
          {editing ? "保存修改" : "创建备忘录"}
        </button>
        <button
          type="button"
          className="secondary-button"
          data-testid="memo-cancel"
          disabled={busy}
          onClick={startCreate}
        >
          {editing ? "取消编辑" : "清空"}
        </button>
      </div>
      <p className="settings-hint">
        每条备忘录是工作区 <code>memos/&lt;标识&gt;.md</code> 里的一个文件：
        front matter 记录标识、标题与标签，正文就是内容本身。标识在创建时生成、
        编辑时保持不变，因此可以用 Git 追踪它的变化。
      </p>
    </section>
  );
}
