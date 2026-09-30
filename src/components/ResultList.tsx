import type { ItemView } from "../types";

interface Props {
  items: ItemView[];
  selection: number;
  onActivate: (item: ItemView) => void;
}

/** 首字符占位图标。真实图标由宿主以 data URL 提供。 */
function FallbackIcon({ title }: { title: string }) {
  const initial = Array.from(title.trim())[0] ?? "?";
  return (
    <span className="icon icon-fallback" aria-hidden="true">
      {initial}
    </span>
  );
}

export function ResultList({ items, selection, onActivate }: Props) {
  if (items.length === 0) {
    return (
      <div className="result-list empty" data-testid="empty-state" role="listbox" aria-label="结果">
        <p className="empty-title">没有匹配的结果</p>
        <p className="empty-hint">试试其他关键词，或按 Escape 关闭</p>
      </div>
    );
  }
  return (
    <ul
      id="result-list"
      className="result-list"
      data-testid="result-list"
      role="listbox"
      aria-label="结果"
    >
      {items.map((item, index) => {
        const selected = index === selection;
        return (
          <li
            key={item.id}
            id={`item-${item.id}`}
            className="result-item"
            data-testid="result-item"
            data-item-id={item.id}
            data-selected={selected ? "true" : "false"}
            data-kind={item.kind}
            role="option"
            aria-selected={selected}
            /* 鼠标悬停只改样式，不改变宿主持有的键盘选择。 */
            onClick={() => onActivate(item)}
          >
            {item.iconDataUrl ? (
              <img className="icon" src={item.iconDataUrl} alt="" aria-hidden="true" />
            ) : (
              <FallbackIcon title={item.title} />
            )}
            <span className="result-text">
              <span className="result-title">{item.title}</span>
              {item.subtitle ? <span className="result-subtitle">{item.subtitle}</span> : null}
            </span>
          </li>
        );
      })}
    </ul>
  );
}
