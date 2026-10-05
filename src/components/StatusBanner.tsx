import type { ActionOutcome, QueryView, StatusView } from "../types";

interface Props {
  status: StatusView | null;
  response: QueryView;
  feedback: ActionOutcome | null;
  /** 配置工作区的问题（配置无效、关联失效等）。 */
  workspaceAlert?: string | null;
}

/**
 * 提示区：错误、快捷键冲突、插件失败、工作区问题与启动反馈。
 * 错误状态与普通提示在视觉上必须可区分（见 styles.css 的 .banner-error）。
 */
export function StatusBanner({ status, response, feedback, workspaceAlert }: Props) {
  const hotkeyError = status?.hotkey.error ?? null;
  const notice = response.notice;
  const failures = response.pluginFailures;

  return (
    <div className="banners">
      {workspaceAlert ? (
        <div className="banner banner-error" data-testid="workspace-alert" role="alert">
          {workspaceAlert}
          <span className="banner-hint">按 Ctrl+, 或点击「设置」检查配置工作区。</span>
        </div>
      ) : null}
      {feedback?.status === "failed" ? (
        <div className="banner banner-error" data-testid="notice" role="alert">
          {feedback.message ?? "操作失败"}
        </div>
      ) : null}
      {feedback?.status === "done" && feedback.message ? (
        /* 有反馈语的成功操作（例如「已请求 Chrome 打开」）也要让用户看见：
           浏览器交接、页面是否加载都无法由我们确认，反馈是唯一的可见结果。 */
        <div className="banner banner-info" data-testid="notice" role="status">
          {feedback.message}
        </div>
      ) : null}
      {feedback?.status === "copiedNeedsManualPaste" ? (
        <div className="banner banner-warning" data-testid="notice" role="status">
          {feedback.message ?? "已复制，请手动粘贴"}
        </div>
      ) : null}
      {feedback?.status === "pastePending" ? (
        <div className="banner banner-info" data-testid="notice" role="status">
          {feedback.message ?? "已复制，正在粘贴…"}
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
      {status?.hotkey.pending ? (
        <div className="banner banner-info" data-testid="hotkey-pending" role="status">
          正在等待系统快捷键授权，请在系统弹窗中确认「打开 Flashcast」。
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
