import type { ItemView, Preview } from "../types";

interface Props {
  /** 当前选中的结果。 */
  item: ItemView | null;
  /** 宿主给出的预览内容（备忘录是完整正文）。 */
  preview: Preview | null;
  /** 是否展开正文。 */
  open: boolean;
  onToggle: () => void;
}

/**
 * 内容预览：紧凑列表下方的按需展开区。
 *
 * 默认列表保持紧凑；选中备忘录或剪贴板历史时这里给出**完整正文**，粘贴前可以确认
 * 结果；选中书签时给出完整链接与目录，打开前可以确认目标。
 * 展开 / 收起只改这一块的高度，不动窗口尺寸，也不做动画（高频键盘操作）。
 */
export function MemoPreview({ item, preview, open, onToggle }: Props) {
  const text = preview && preview.kind === "text" ? preview : null;
  // 剪贴板历史与备忘录、书签一样：粘贴 / 打开之前要能确认完整内容。
  if (
    !item ||
    (item.kind !== "memo" && item.kind !== "bookmark" && item.kind !== "clipboardEntry") ||
    !text
  ) {
    return null;
  }
  return (
    <section className="preview" data-testid="memo-preview" data-open={open ? "true" : "false"}>
      <header className="preview-header">
        <span className="preview-title" data-testid="memo-preview-title">
          {text.title ?? item.title}
        </span>
        <span className="preview-meta" data-testid="memo-preview-action">
          默认操作：{item.defaultActionLabel} · 完整内容
        </span>
        <button
          type="button"
          className="ghost-button"
          data-testid="memo-preview-toggle"
          aria-expanded={open}
          onClick={onToggle}
        >
          {open ? "收起预览" : "展开预览"}
        </button>
      </header>
      {open ? (
        <pre className="preview-body" data-testid="memo-preview-body">
          {text.body}
        </pre>
      ) : null}
    </section>
  );
}
