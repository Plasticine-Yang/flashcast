import { useEffect, useState } from "react";
import type { ItemView, Memo, MemoProblem } from "../types";
import { parseTags } from "./MemoPanel";

interface Props {
  items: ItemView[]; selection: number; memos: Memo[]; problems: MemoProblem[];
  workspaceKey: string | null; enabled: boolean; busy: boolean;
  message: { level: string; text: string } | null;
  onSelect: (index: number) => void; onPaste: (item: ItemView) => void;
  onCreate: (title: string, tags: string[], body: string) => Promise<boolean>;
  onUpdate: (id: string, title: string, tags: string[], body: string) => Promise<boolean>;
  onDelete: (id: string) => Promise<boolean>; onOpenWorkspace: () => void;
  onEditingChange: (editing: boolean) => void;
}
interface Draft { id: string | null; title: string; tags: string; body: string }
export function MemoWorkbench(props: Props) {
  const { items, selection, memos, workspaceKey, enabled, busy } = props;
  const [draft, setDraft] = useState<Draft | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const selected = items[selection];
  const memo = memos.find(m => `memo:${m.id}` === selected?.id);
  const writable = workspaceKey !== null && enabled && !busy;
  useEffect(() => { setDraft(null); setDeleteId(null); }, [workspaceKey, enabled]);
  useEffect(() => { setDeleteId(null); }, [selected?.id]);
  const editing = draft !== null;
  useEffect(() => {
    props.onEditingChange(editing);
    return () => props.onEditingChange(false);
  }, [editing, props.onEditingChange]);
  useEffect(() => {
    if (!draft) return;
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy && !event.isComposing) {
        event.preventDefault(); event.stopImmediatePropagation(); setDraft(null);
      }
    };
    window.addEventListener("keydown", escape, true);
    return () => window.removeEventListener("keydown", escape, true);
  }, [draft, busy]);
  const save = async () => {
    if (!draft || busy) return;
    const saved = draft.id
      ? await props.onUpdate(draft.id, draft.title, parseTags(draft.tags), draft.body)
      : await props.onCreate(draft.title, parseTags(draft.tags), draft.body);
    if (saved) setDraft(null);
  };
  return <section className="memo-workbench" data-testid="memo-workbench" data-editing={!!draft}>
    <header className="memo-toolbar"><span>备忘录 <small>{items.filter(i => i.kind === "memo").length} 条</small></span><button className="primary-button" data-testid="memo-new" disabled={!writable || !!draft} onClick={() => setDraft({id:null,title:"",tags:"",body:""})}>＋ 新建</button></header>
    {props.message ? <p className={`memo-feedback ${props.message.level}`} role={props.message.level === "error" ? "alert" : "status"}>{props.message.text}</p> : null}
    {workspaceKey === null ? <div className="memo-workspace-notice"><span>关联配置工作区后，创建和保存备忘录。</span><button className="secondary-button" onClick={props.onOpenWorkspace}>关联工作区</button></div> : null}
    <div className="memo-columns">
      <ul className="memo-results" id="result-list" role="listbox" aria-label="结果">
        {items.map((item,index) => <li key={item.id} id={`item-${item.id}`} role="option" aria-selected={selection === index} data-selected={selection === index} data-testid="memo-search-item" onClick={() => { if (!draft) props.onSelect(index); }} onDoubleClick={() => { if (!draft) props.onPaste(item); }}><strong>{item.title}</strong><span>{item.subtitle ?? (item.kind === "memo" ? "无标签" : "插件入口")}</span></li>)}
        {items.length === 0 ? <li className="memo-list-empty">{workspaceKey === null ? "尚未关联工作区" : "没有匹配的备忘录"}</li> : null}
      </ul>
      <div className="memo-detail">
        {draft ? <form className="memo-editor" data-testid="memo-inline-editor" onSubmit={event => {event.preventDefault(); void save();}} onKeyDown={event => {
          if (event.ctrlKey && event.key === "Enter" && !event.nativeEvent.isComposing) {
            event.preventDefault(); event.stopPropagation(); void save();
          }
        }}>
          <div className="memo-detail-heading"><strong>{draft.id ? "编辑备忘录" : "新建备忘录"}</strong><button type="button" className="ghost-button" disabled={busy} onClick={() => setDraft(null)}>取消</button></div>
          <label>标题<input autoFocus value={draft.title} disabled={busy} data-testid="inline-memo-title" placeholder="常用回复" onChange={event => setDraft({...draft,title:event.target.value})} /></label>
          <label>标签<input value={draft.tags} disabled={busy} data-testid="inline-memo-tags" placeholder="回复、工作" onChange={event => setDraft({...draft,tags:event.target.value})} /></label>
          <label className="memo-body-field">正文<textarea value={draft.body} disabled={busy} data-testid="inline-memo-body" placeholder="输入要粘贴的内容…" onChange={event => setDraft({...draft,body:event.target.value})} /></label>
          <button className="primary-button" type="submit" data-testid="inline-memo-save" disabled={!writable}>{busy ? "保存中…" : "保存备忘录"}</button>
        </form> : memo ? <>
          <div className="memo-detail-heading"><h2>{memo.title}</h2><button className="ghost-button" data-testid="inline-memo-edit" disabled={!writable} onClick={() => setDraft({id:memo.id,title:memo.title,tags:memo.tags.join("、"),body:memo.body})}>编辑</button></div>
          <div className="memo-tag-row">{memo.tags.map(tag => <span key={tag}>#{tag}</span>)}</div>
          <pre className="memo-full-body" data-testid="inline-memo-preview">{memo.body}</pre>
          <div className="memo-detail-actions"><button className="primary-button" disabled={busy} onClick={() => props.onPaste(selected)}>粘贴正文 ↵</button><button className="ghost-button danger-button" data-testid="inline-memo-delete" disabled={!writable} onClick={async () => { if (deleteId !== memo.id) {setDeleteId(memo.id); return;} if (await props.onDelete(memo.id)) setDeleteId(null); }}>{deleteId === memo.id ? "确认删除" : "删除"}</button>{deleteId === memo.id ? <button className="ghost-button" onClick={() => setDeleteId(null)}>取消</button> : null}</div>
        </> : <div className="content-empty"><strong>{selected?.title ?? "常用内容，随时取用"}</strong><p>{selected?.kind === "command" ? "回车进入插件范围。" : selected?.kind === "application" ? (selected.subtitle ?? "回车打开软件。") : items.length > 0 ? "正在读取正文…" : "新建一条备忘录，或换一个关键词。"}</p>{selected && selected.kind !== "memo" ? <button className="primary-button" onClick={() => props.onPaste(selected)}>{selected.defaultActionLabel} ↵</button> : null}</div>}
      </div>
    </div>
    {props.problems.length > 0 ? <details className="memo-read-problems"><summary>{props.problems.length} 个文件无法读取</summary>{props.problems.map(p => <p key={p.path}>{p.path}：{p.reason}</p>)}</details> : null}
  </section>;
}
