import type { ActionOutcome, QueryView, StatusView } from "../types";

interface Props {
  status: StatusView | null;
  response: QueryView;
  feedback: ActionOutcome | null;
}

/**
 * 提示区：错误、快捷键冲突、插件失败与启动反馈。
 * 错误状态与普通提示在视觉上必须可区分（见 styles.css 的 .banner-error）。
 */
export function StatusBanner({ status, response, feedback }: Props) {
  const hotkeyError = status?.hotkey.error ?? null;
  const notice = response.notice;
  const failures = response.pluginFailures;

  return (
    <div className="banners">
      {feedback?.status === "failed" ? (
        <div className="banner banner-error" data-testid="notice" role="alert">
          {feedback.message ?? "操作失败"}
        </div>
      ) : null}
      {feedback?.status === "copiedNeedsManualPaste" ? (
        <div className="banner banner-warning" data-testid="notice" role="status">
          {feedback.message ?? "已复制，请手动粘贴"}
        </div>
      ) : null}
      {notice ? (
        <div
          className={`banner banner-${notice.level}`}
          data-testid="notice"
          role={notice.level === "error" ? "alert" : "status"}
        >
          {notice.message}
        </div>
      ) : null}
      {hotkeyError ? (
        <div className="banner banner-warning" data-testid="hotkey-warning" role="status">
          全局快捷键 {status?.hotkey.label} 注册失败：{hotkeyError}
          <span className="banner-hint">可用托盘图标「打开 Flashcast」或应用菜单进入。</span>
        </div>
      ) : null}
      {failures.map((failure) => (
        <div
          className="banner banner-error"
          data-testid="plugin-failure"
          role="status"
          key={`${failure.pluginId}-${failure.kind}`}
        >
          插件 {failure.pluginId} {failure.kind === "timeout" ? "超时" : failure.kind === "panic" ? "崩溃" : "出错"}
          ：{failure.reason}（其他结果仍可使用）
        </div>
      ))}
    </div>
  );
}
