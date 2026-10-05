import type { ItemView } from "../types";

interface Props {
  selected: ItemView | null;
  count: number;
}

/** 操作栏：始终显示当前选中结果的默认操作（打开 / 在 Chrome 打开 / 粘贴）。 */
export function ActionBar({ selected, count }: Props) {
  return (
    <footer className="action-bar" data-testid="action-bar">
      <span className="product-signature"><svg viewBox="0 0 16 20" width="12" height="15" fill="currentColor" aria-hidden="true"><path d="M9 0 1 11h6l-1 9 9-12H9z"/></svg> FLASHCAST</span>
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
      <span className="action-hint">
        <kbd>Esc</kbd> 关闭
      </span>
      <span className="action-count" data-testid="result-count">
        {count} 条
      </span>
    </footer>
  );
}
