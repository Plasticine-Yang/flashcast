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
    return "尚未关联 profile";
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
          <dt>已关联 profile</dt>
          <dd data-testid="chrome-association">{associationLabel(chrome ?? EMPTY_CHROME)}</dd>
        </div>
        <div className="settings-fact">
          <dt>书签索引</dt>
          <dd data-testid="chrome-bookmarks-status">
            {chrome?.bookmarksLabel ?? "尚未读取"}{" "}
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
        {bookmarks?.path ? (
          <div className="settings-fact">
            <dt>书签文件</dt>
            <dd data-testid="chrome-bookmarks-path">{bookmarks.path}</dd>
          </div>
        ) : null}
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

      <ul className="theme-list" data-testid="chrome-profile-list">
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
              </span>
            </span>
            {profile.associated ? (
              <span className="theme-badge" data-testid="chrome-associated-badge">
                已关联
              </span>
            ) : null}
            {profile.managed ? (
              <span className="theme-badge" data-testid="chrome-managed-badge">
                企业管理
              </span>
            ) : null}
            {!profile.bookmarksReadable ? (
              <span className="theme-badge theme-badge-error" data-testid="chrome-unreadable">
                不可读
              </span>
            ) : null}
            <button
              type="button"
              className="primary-button"
              data-testid="chrome-associate"
              disabled={busy || profile.associated}
              onClick={() => onAssociate(profile.dir)}
            >
              {profile.associated ? "当前使用" : "关联"}
            </button>
          </li>
        ))}
      </ul>
      {chrome && chrome.profiles.length === 0 ? (
        <p className="settings-hint" data-testid="chrome-no-profiles">
          {available
            ? "没有发现任何 profile：启动一次 Chrome 后这里会列出它的 profile。"
            : "没有找到 Chrome，无法列出 profile。请安装 Chrome，或确认当前用户能读取它的用户数据目录。"}
        </p>
      ) : null}

      <p className="settings-hint">
        关联只记录在本机（设备本地存储），不进入配置工作区，也不会随配置同步；书签索引
        可从 Chrome 的 Bookmarks 文件随时重建。Flashcast 只读取这些文件，绝不修改或删除；
        结果里按标题、网址与目录检索，回车在关联的 profile 里打开链接——参数以 argv 数组
        传给 Chrome，链接不经 shell 拼接。
      </p>
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
  bookmarksLabel: "尚未关联 Chrome profile",
};
