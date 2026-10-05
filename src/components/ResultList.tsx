import type { ItemView } from "../types";
import { Glyph } from "./Glyph";

interface Props {
  items: ItemView[];
  selection: number;
  onActivate: (item: ItemView) => void;
  /** 当前查询：用于突出命中的字符。 */
  query?: string;
}

/** 语义占位图标。真实图标由宿主以 data URL 提供。 */
function FallbackIcon({ kind, title }: { kind: string; title: string }) {
  const glyph = kind !== "application" ? kind : /终端|terminal/i.test(title) ? "command" : /文件|files|finder/i.test(title) ? "workspace" : /浏览器|firefox|chrome|safari/i.test(title) ? "browser" : /code|编辑器/i.test(title) ? "editor" : /计算器|calculator/i.test(title) ? "calculator" : kind;
  return (
    <span className="icon icon-fallback" aria-hidden="true">
      <Glyph name={glyph} />
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
      <svg viewBox="0 0 16 16" width="15" height="15" fill="none" stroke="currentColor">
        <rect x="1.5" y="2.5" width="13" height="11" rx="2" strokeWidth="1.3" />
        <path d="M1.5 6h13" strokeWidth="1.3" />
        <circle cx="4" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
        <circle cx="6.25" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
        <circle cx="8.5" cy="4.25" r="0.75" fill="currentColor" stroke="none" />
      </svg>
    </span>
  );
}

/**
 * 标题里的命中高亮：把查询串第一次出现的位置包成 `<mark>`。
 *
 * 只处理纯文本标题：返回的是 React 节点，宿主下发的内容仍会被转义，不存在注入面。
 * 空查询（首屏快速访问项）不做高亮。
 */
function highlight(title: string, query: string | undefined) {
  const needle = (query ?? "").trim();
  if (needle.length === 0) {
    return title;
  }
  const at = title.toLowerCase().indexOf(needle.toLowerCase());
  if (at < 0) {
    return title;
  }
  return (
    <>
      {title.slice(0, at)}
      <mark>{title.slice(at, at + needle.length)}</mark>
      {title.slice(at + needle.length)}
    </>
  );
}

export function ResultList({ items, selection, onActivate, query }: Props) {
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
            {item.thumbnailDataUrl ? (
              /* 图片剪贴板历史：缩略图是用户分辨条目的主要依据（ticket 10）。 */
              <img
                className="icon thumbnail"
                data-testid="result-thumbnail"
                src={item.thumbnailDataUrl}
                alt=""
                aria-hidden="true"
              />
            ) : item.iconDataUrl ? (
              <img className="icon" src={item.iconDataUrl} alt="" aria-hidden="true" />
            ) : item.kind === "bookmark" ? (
              /* 书签结果必须有浏览器图标：它来自哪个浏览器是用户要认出的信息。 */
              <BrowserIcon />
            ) : (
              <FallbackIcon kind={item.kind} title={item.title} />
            )}
            <span className="result-text">
              <span className="result-title">{highlight(item.title, query)}</span>
              {item.subtitle ? <span className="result-subtitle">{item.subtitle}</span> : null}
            </span>
            <span className="result-kind">{{ application: "软件", command: "命令", memo: "备忘录", clipboardEntry: "剪贴板", bookmark: "书签" }[item.kind]}</span>
            <span className="result-enter" aria-hidden="true">↵</span>
          </li>
        );
      })}
    </ul>
  );
}
