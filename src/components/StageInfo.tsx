import type { ItemView } from "../types";

interface Props {
  /** 当前选中的结果。 */
  item: ItemView | null;
}

const KIND_LABEL: Record<string, string> = {
  application: "软件",
  command: "命令",
  memo: "备忘录",
  clipboardEntry: "剪贴板",
  bookmark: "书签",
};

/**
 * 常驻预览的「非完整内容」形态：选中的是软件或命令时，右栏给出条目本身的信息。
 *
 * 备忘录、剪贴板、书签由 `MemoPreview` 负责完整正文 / 图片；这里只补上其余条目类型，
 * 让「选中即见内容」的右栏在任何选中项下都不空着。
 */
export function StageInfo({ item }: Props) {
  if (!item) {
    return (
      <div className="stage-info" data-testid="stage-empty">
        <span className="stage-kind">预览</span>
        <span className="stage-sub">选择一条结果查看内容。</span>
      </div>
    );
  }
  const initial = Array.from(item.title.trim())[0] ?? "?";
  return (
    <div className="stage-info" data-testid="stage-info">
      <span className="stage-icon" aria-hidden="true">
        {item.iconDataUrl ? (
          <img src={item.iconDataUrl} alt="" />
        ) : (
          <span>{initial}</span>
        )}
      </span>
      <span className="stage-kind">{KIND_LABEL[item.kind] ?? "结果"}</span>
      <span className="stage-name">{item.title}</span>
      {item.subtitle ? <span className="stage-sub">{item.subtitle}</span> : null}
      <span className="stage-action">
        <kbd>↵</kbd>
        {item.defaultActionLabel}
      </span>
    </div>
  );
}
