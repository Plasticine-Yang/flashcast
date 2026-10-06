import type { ItemView } from "../types";

interface Props {
  selected: ItemView | null;
  count: number;
  editing?: boolean;
  plugin?: boolean;
  onActions?: () => void;
  menuOpen?: boolean;
}

/** 操作栏：始终显示当前选中结果的默认操作（打开 / 在 Chrome 打开 / 粘贴）。 */
export function ActionBar({
  selected,
  count,
  editing = false,
  plugin = false,
  onActions,
  menuOpen,
}: Props) {
  return (
    <footer className="action-bar" data-testid="action-bar">
      <span className="product-signature">
        <svg
          viewBox="42 25 46 78"
          width="12"
          height="15"
          fill="currentColor"
          aria-hidden="true"
        >
          <path d="M77 25 42 59h23Z M88 65H66l-11 38Z" />
        </svg>{" "}
        FLASHCAST
      </span>
      {editing ? (
        <span className="action-hint">
          <kbd>Ctrl</kbd>
          <kbd>Enter</kbd> 保存
        </span>
      ) : (
        <>
          <span className="action-hint">
            <kbd>↑</kbd>
            <kbd>↓</kbd> 选择
          </span>
          <span className="action-hint">
            <kbd>Enter</kbd>
            <span data-testid="default-action-label">
              {selected ? selected.defaultActionLabel : "打开"}
            </span>
          </span>
        </>
      )}
      <span className="action-hint">
        <kbd>Esc</kbd> {editing ? "取消编辑" : plugin ? "返回" : "关闭"}
      </span>
      {plugin && !editing ? (
        <button
          className="action-toggle"
          aria-expanded={menuOpen}
          data-testid="actions-toggle"
          onClick={onActions}
        >
          动作 <kbd>Ctrl K</kbd>
        </button>
      ) : null}
      <span className="action-count" data-testid="result-count">
        {count} 条
      </span>
    </footer>
  );
}
