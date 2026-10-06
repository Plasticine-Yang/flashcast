import { useEffect, useState } from "react";
import type { PluginCommandView } from "../types";
export function CommandShortcuts({
  commands,
  busy,
  onSave,
}: {
  commands: PluginCommandView[];
  busy: boolean;
  onSave: (id: string, shortcut: string | null) => void;
}) {
  return (
    <section className="settings-section command-shortcuts">
      <h3>插件快捷键</h3>
      {commands.map((command) => (
        <CommandShortcut
          key={command.id}
          command={command}
          busy={busy}
          onSave={onSave}
        />
      ))}
    </section>
  );
}
function CommandShortcut({
  command: c,
  busy,
  onSave,
}: {
  command: PluginCommandView;
  busy: boolean;
  onSave: (id: string, shortcut: string | null) => void;
}) {
  const [draft, setDraft] = useState(c.shortcut);
  useEffect(() => setDraft(c.shortcut), [c.shortcut]);
  return (
    <div className="command-shortcut">
      <label>
        {c.title}
        <input
          aria-label={c.title + "快捷键"}
          data-testid={`command-shortcut-${c.pluginId}`}
          value={draft}
          placeholder="点击后按组合键"
          onChange={(e) => setDraft(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Tab") return;
            if (["Control", "Shift", "Alt", "Meta"].includes(e.key)) {
              e.preventDefault();
              return;
            }
            if (e.ctrlKey || e.altKey || e.metaKey) {
              e.preventDefault();
              e.stopPropagation();
              setDraft(
                [
                  e.ctrlKey ? "Ctrl" : null,
                  e.altKey ? "Alt" : null,
                  e.shiftKey ? "Shift" : null,
                  e.metaKey ? "Command" : null,
                  e.key.length === 1 ? e.key.toUpperCase() : e.key,
                ]
                  .filter(Boolean)
                  .join("+"),
              );
            }
          }}
        />
      </label>
      <div className="shortcut-actions">
        <button
          className="primary-button"
          disabled={busy}
          onClick={() => onSave(c.id, draft)}
        >
          保存
        </button>
        <button
          className="ghost-button"
          disabled={busy}
          onClick={() => onSave(c.id, null)}
        >
          恢复默认
        </button>
        <button
          className="ghost-button"
          disabled={busy}
          onClick={() => onSave(c.id, "")}
        >
          清除
        </button>
      </div>
      <p className="settings-hint" role={c.error ? "alert" : "status"}>
        {!c.enabled
          ? "插件停用时不注册快捷键"
          : c.pending
            ? "等待系统快捷键授权…"
            : c.error
              ? c.error +
                (c.effectiveShortcut
                  ? `；原快捷键仍生效：${c.effectiveShortcut}`
                  : "")
              : c.registered
                ? `当前生效：${c.effectiveShortcut}`
                : "未设置快捷键"}
      </p>
    </div>
  );
}
