import type { ChangedFile, WorkspaceChanges } from "../types";

interface Props {
  changes: WorkspaceChanges | null;
  /** 提交说明草稿。 */
  message: string;
  /** 当前勾选的路径。 */
  selected: string[];
  /** 正在展示差异的路径。 */
  diffPath: string | null;
  /** 正在执行 Git 操作，按钮暂时禁用。 */
  busy: boolean;
  onToggle: (path: string) => void;
  onToggleAll: () => void;
  onSelectDiff: (path: string) => void;
  onMessageChange: (value: string) => void;
  onCommit: () => void;
  onRefresh: () => void;
}

/**
 * 变更与提交区段：展示分支、差异基准、每个文件的状态与真实差异，
 * 并让用户显式勾选提交范围后创建提交。
 *
 * 透明性要求（ticket 15）：差异基准、提交范围与「已暂存改动不会自动纳入」
 * 都必须写清楚，绝不默认放宽范围。高频操作不做过渡动画。
 */
export function ChangesPanel({
  changes,
  message,
  selected,
  diffPath,
  busy,
  onToggle,
  onToggleAll,
  onSelectDiff,
  onMessageChange,
  onCommit,
  onRefresh,
}: Props) {
  const current = changes?.files.find((file) => file.path === diffPath) ?? null;
  const allSelected =
    changes != null && changes.files.length > 0 && selected.length === changes.files.length;

  return (
    <section className="settings-section" data-testid="changes-section">
      <h2 className="settings-section-title">变更与提交</h2>

      {changes === null ? (
        <p className="settings-hint" data-testid="changes-loading">
          正在读取工作区变更…
        </p>
      ) : !changes.repository ? (
        <>
          <div className="banner banner-warning" data-testid="changes-unavailable" role="status">
            {changes.error ?? "当前工作区不是 Git 仓库，无法查看变更或创建提交。"}
          </div>
          {/* Git 环境恢复后需要重新检测入口：外部处理完再点这里。 */}
          <div className="settings-row">
            <button
              type="button"
              className="ghost-button"
              data-testid="changes-refresh"
              disabled={busy}
              onClick={onRefresh}
            >
              重新检测
            </button>
          </div>
        </>
      ) : (
        <>
          <dl className="settings-facts">
            <div className="settings-fact">
              <dt>分支</dt>
              <dd data-testid="changes-branch">
                {changes.detached ? "分离 HEAD" : (changes.branch ?? "未知")}
              </dd>
            </div>
            <div className="settings-fact">
              <dt>差异基准</dt>
              <dd data-testid="changes-diff-base">{changes.diffBase}</dd>
            </div>
            <div className="settings-fact">
              <dt>待提交</dt>
              <dd data-testid="changes-count">
                {changes.hasChanges
                  ? `${changes.files.length} 个文件有改动`
                  : "没有可提交的变更"}
              </dd>
            </div>
          </dl>

          {changes.error ? (
            <div className="banner banner-error" data-testid="changes-error" role="alert">
              {changes.error}
            </div>
          ) : null}

          {changes.state || changes.detached ? (
            <div className="banner banner-warning" data-testid="changes-state" role="status">
              {changes.state ??
                "工作区处于分离 HEAD 状态（HEAD 未指向任何分支），无法在分支上创建提交。"}
            </div>
          ) : null}

          {changes.hasChanges ? (
            <>
              <div className="changes-list" data-testid="changes-list">
                {changes.files.map((file) => (
                  <div
                    key={file.path}
                    className={
                      selected.includes(file.path) ? "change-row change-row-selected" : "change-row"
                    }
                    data-testid="change-row"
                    data-path={file.path}
                    data-selected={selected.includes(file.path) ? "true" : "false"}
                    data-staged={file.staged ? "true" : "false"}
                    data-untracked={file.untracked ? "true" : "false"}
                    data-code={file.code}
                  >
                    <input
                      type="checkbox"
                      className="change-check"
                      data-testid="change-check"
                      checked={selected.includes(file.path)}
                      aria-label={`选择 ${file.path}`}
                      onChange={() => onToggle(file.path)}
                    />
                    <button
                      type="button"
                      className="change-path"
                      data-testid="change-path"
                      onClick={() => onSelectDiff(file.path)}
                    >
                      <span className="change-code">{file.code}</span>
                      <span className="change-name">{file.path}</span>
                    </button>
                    <span className="change-status" data-testid="change-status">
                      {file.statusLabel}
                    </span>
                  </div>
                ))}
              </div>

              <div className="settings-row">
                <button
                  type="button"
                  className="secondary-button"
                  data-testid="changes-select-all"
                  disabled={busy}
                  onClick={onToggleAll}
                >
                  {allSelected ? "取消全选" : "全选"}
                </button>
                <button
                  type="button"
                  className="ghost-button"
                  data-testid="changes-refresh"
                  disabled={busy}
                  onClick={onRefresh}
                >
                  重新检测
                </button>
              </div>

              <ChangeDiff file={current} />

              <label className="settings-label" htmlFor="commit-message">
                提交说明
              </label>
              <textarea
                id="commit-message"
                className="path-input commit-message"
                data-testid="commit-message"
                rows={2}
                spellCheck={false}
                placeholder="说明这次配置改动"
                value={message}
                onChange={(event) => onMessageChange(event.target.value)}
              />

              <div className="settings-row">
                <button
                  type="button"
                  className="primary-button"
                  data-testid="commit-submit"
                  disabled={busy}
                  onClick={onCommit}
                >
                  创建提交
                </button>
                <span className="settings-hint" data-testid="commit-scope">
                  提交范围：勾选的 {selected.length} / {changes.files.length} 个文件（取其当前工作区内容）
                </span>
              </div>
              <p className="settings-hint">
                已暂存的改动不会自动纳入提交；只有勾选的路径会进入这次提交，未勾选的改动原样保留。
              </p>
            </>
          ) : (
            <>
              <p className="settings-hint" data-testid="changes-empty">
                工作区没有可提交的变更。
              </p>
              <div className="settings-row">
                <button
                  type="button"
                  className="ghost-button"
                  data-testid="changes-refresh"
                  disabled={busy}
                  onClick={onRefresh}
                >
                  重新检测
                </button>
              </div>
            </>
          )}
        </>
      )}
    </section>
  );
}

/** 单个文件的真实差异。 */
function ChangeDiff({ file }: { file: ChangedFile | null }) {
  if (file === null) {
    return (
      <pre className="change-diff" data-testid="change-diff">
        选择一个文件查看它与基准之间的真实差异。
      </pre>
    );
  }
  return (
    <pre className="change-diff" data-testid="change-diff" data-path={file.path}>
      {file.diff.trimEnd()}
      {file.diffTruncated ? "\n…（差异过大，已截断）" : ""}
    </pre>
  );
}
