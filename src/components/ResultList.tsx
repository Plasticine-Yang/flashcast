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

/**
 * 书签结果的浏览器图标。
 *
 * 用内联 SVG 而不是图片文件或图标字体：颜色取当前文字色（主题切换自动跟随），
 * 尺寸与占位图标一致，不引入新的依赖或资源。
 */
function BrowserIcon() {
  return (
    <span className="icon icon-browser" aria-hidden="true" data-testid="bookmark-icon">
      <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor">
        <rect x="1.5" y="2.5" width="13" height="11" rx="2" strokeWidth="1.3" />
        <path d="M1.5 6h13" strokeWidth="1.3" />
        <circle cx="4" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
        <circle cx="6.25" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
        <circle cx="8.5" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
      </svg>
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
            ) : item.kind === "bookmark" ? (
              /* 书签结果必须有浏览器图标：它来自哪个浏览器是用户要认出的信息。 */
              <BrowserIcon />
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
