import type { ChromeState } from "../types";

interface Props {
  /** 当前 Chrome 状态；`null` 表示还没读到。 */
  chrome: ChromeState | null;
  busy: boolean;
  onAssociate: (profileDir: string) => void;
  onRefresh: () => void;
}

/** 关联状态的简短说明。 */
function associationLabel(chrome: ChromeState): string {
  if (chrome.associated === null) {
    return "尚未关联 Chrome 用户";
  }
  return `${chrome.associatedName ?? chrome.associated}（目录名 ${chrome.associated}）`;
}

/**
 * Chrome 书签插件区段：发现结果、profile 关联与索引状态。
 *
 * 本机路径只在本机显示（它们保存在设备本地存储里，不进入配置工作区，也不随配置同步）。
 * 关联只能选宿主**发现到的** profile 目录名：Chrome 在 `--profile-directory` 指向不存在的
 * 目录时会静默新建一个空 profile，因此宿主在关联与打开前都会校验。
 */
export function ChromePanel({ chrome, busy, onAssociate, onRefresh }: Props) {
  const bookmarks = chrome?.bookmarks ?? null;
  const available = chrome?.available ?? false;
  return (
    <section className="settings-section" data-testid="chrome-section">
      <h2 className="settings-section-title">Chrome 书签</h2>

      <dl className="settings-facts chrome-index">
        <div className="settings-fact">
          <dt>书签索引</dt>
          <dd className="chrome-index-status" data-testid="chrome-bookmarks-status">
            <span>{chrome?.bookmarksLabel ?? "尚未读取"}</span>
            <button
              type="button"
              className="secondary-button"
              data-testid="chrome-refresh"
              disabled={busy}
              onClick={onRefresh}
            >
              重新读取
            </button>
          </dd>
        </div>
      </dl>

      {chrome?.error ? (
        <div className="banner banner-error" data-testid="chrome-error" role="alert">
          {chrome.error}
        </div>
      ) : null}
      {(chrome?.warnings ?? []).map((warning) => (
        <div className="banner banner-warning" data-testid="chrome-warning" key={warning}>
          {warning}
        </div>
      ))}

      <h3 className="settings-section-title">Chrome 用户</h3>
      <ul className="theme-list chrome-profiles" data-testid="chrome-profile-list">
        {(chrome?.profiles ?? []).map((profile) => (
          <li
            className="theme-item"
            data-testid="chrome-profile"
            data-profile-dir={profile.dir}
            data-associated={profile.associated ? "true" : "false"}
            data-managed={profile.managed ? "true" : "false"}
            key={profile.dir}
          >
            <span className="theme-name">
              {profile.name}
              <span className="theme-meta">
                目录 {profile.dir}
                {profile.userName ? ` · ${profile.userName}` : " · 未登录账号"}
                {profile.hasBookmarks ? "" : " · 还没有书签文件"}
                {profile.managed ? (
                  <span data-testid="chrome-managed-badge"> · 企业管理</span>
                ) : null}
              </span>
            </span>
            {!profile.bookmarksReadable ? (
              <span className="theme-badge theme-badge-error" data-testid="chrome-unreadable">
                不可读
              </span>
            ) : null}
            {profile.associated ? (
              <span className="theme-badge chrome-current" data-testid="chrome-associated-badge">
                当前使用
              </span>
            ) : (
              <button
                type="button"
                className="primary-button"
                data-testid="chrome-associate"
                disabled={busy}
                onClick={() => onAssociate(profile.dir)}
              >
                关联
              </button>
            )}
          </li>
        ))}
      </ul>
      {chrome && chrome.profiles.length === 0 ? (
        <p className="settings-hint" data-testid="chrome-no-profiles">
          {available
            ? "尚未发现 Chrome 用户，启动一次 Chrome 后重新读取。"
            : "未找到 Chrome。请安装 Chrome，并确认可以读取用户数据目录。"}
        </p>
      ) : null}

      {bookmarks?.status.kind === "missing" ? <div className="content-empty" data-testid="chrome-missing-explanation"><strong>这个用户目录没有书签文件</strong><p>Flashcast 读取 Chrome 的原生书签。请先在该用户中收藏一个网页，再重新读取。</p><p>若 Chrome 中已有书签，请在 chrome://version 核对「个人资料路径」是否与上方目录一致。扩展中保存的链接不属于原生书签。</p></div> : null}
      <details className="storage-details chrome-source">
        <summary>来源与本机路径</summary>
        <dl className="settings-facts">
          <div className="settings-fact">
            <dt>Chrome</dt>
            <dd data-testid="chrome-availability">
              {available
                ? `${chrome?.brandLabel ?? "Chrome"}（${chrome?.binary ?? ""}）`
                : "未找到 Chrome 可执行文件"}
            </dd>
          </div>
          <div className="settings-fact">
            <dt>用户数据目录</dt>
            <dd data-testid="chrome-user-data-dir">
              {chrome?.userDataDir ?? "未知"}
              {chrome?.customUserDataDir ? "（默认位置之外，打开时会传 --user-data-dir）" : ""}
            </dd>
          </div>
          <div className="settings-fact">
            <dt>已关联用户</dt>
            <dd data-testid="chrome-association">{associationLabel(chrome ?? EMPTY_CHROME)}</dd>
          </div>
          {bookmarks?.path ? (
            <div className="settings-fact">
              <dt>书签文件</dt>
              <dd data-testid="chrome-bookmarks-path">{bookmarks.path}</dd>
            </div>
          ) : null}
        </dl>

        <p className="settings-hint">关联只保存在本机。Flashcast 只读取 Chrome 的原生书签。</p>
      </details>
    </section>
  );
}
/** 未关联时的占位状态，避免在渲染里到处判空。 */
const EMPTY_CHROME: ChromeState = {
  available: false,
  brandLabel: null,
  customUserDataDir: false,
  binary: null,
  userDataDir: null,
  profiles: [],
  associated: null,
  associatedName: null,
  error: null,
  warnings: [],
  bookmarks: { path: null, status: { kind: "notAssociated" }, entries: [] },
  bookmarksLabel: "尚未关联 Chrome 用户",
};
