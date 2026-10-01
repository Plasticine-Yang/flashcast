//! # flashcast-core
//!
//! Flashcast 的无头宿主。**对外的查询入口与命令入口是唯一的集成测试边界**
//! （ADR §3）：`Host::query` 与 `Host::execute`，以及 `select` /
//! `move_selection` / `set_selection` / `back` / `rescan` / 设置读写。
//!
//! 本 crate 不依赖 Tauri，因此可以在没有桌面会话的 CI runner 上完整测试。

pub mod chrome;
pub mod clipboard;
pub mod clone;
pub mod device;
pub mod git;
pub mod host;
pub mod manifest;
pub mod memo;
pub mod model;
pub mod plugin;
pub mod plugins;
pub mod ranking;
pub mod registry;
pub mod settings;
pub mod sync;
pub mod theme;
pub mod watch;
pub mod workspace;

pub use chrome::{
    parse_bookmarks, search_bookmarks, BookmarkEntry, BookmarkIndex, BookmarkRefresh,
    BookmarkSnapshot, BookmarksFile, BookmarksStatus, ChromeAssociation, ChromeBookmarkError,
    ChromeProfileView, ChromeState, Node as BookmarkNode, Roots as BookmarkRoots,
    BOOKMARK_ITEM_PREFIX, KEY_CHROME_ASSOCIATION, PARSE_RETRY_DELAY,
};
pub use clipboard::{
    content_hash_text, event_from_capture, event_id_from_item_id, new_event_id, now_ms,
    summary_for_text, AttachmentKind, ClipboardAttachment, ClipboardEvent, ClipboardFormat,
    ClipboardPayload, ClipboardStats, ClipboardStore, ClipboardStoreError, InsertOutcome,
    PayloadRole, ReclaimReport, ATTACHMENTS_DIR, CLIPBOARD_DB_FILE, CLIPBOARD_DIR,
    CLIPBOARD_ITEM_PREFIX, CLIPBOARD_PLUGIN_ID, SCHEMA_VERSION, SUMMARY_MAX_CHARS,
};
pub use clone::{
    redact, strip_userinfo, CloneControl, CloneOutcome, ClonePhase, CloneProgress,
    CredentialProvider,
};
pub use device::{
    CredentialStore, DeviceError, DeviceStore, StoredToken, DEVICE_STATE_FILE,
    GIT_CREDENTIALS_FILE, KEY_WORKSPACE_PATH,
};
pub use git::{ChangedFile, CommitOutcome, GitError, WorkspaceChanges, MAX_DIFF_CHARS};
pub use host::{plugin_entry_item, quick_access_commands, Host, HostDeps, PLUGIN_ENTRY_PREFIX};
pub use manifest::{
    ManifestEntry, ManifestError, PluginManifestFile, PluginOrigin, MANIFEST_SCHEMA_VERSION,
};
pub use memo::{
    is_valid_memo_id, new_memo_id, Memo, MemoBook, MemoError, MemoProblem, MemoSnapshot,
    MEMO_EXTENSION,
};
pub use model::{
    ActionOutcome, ActionStatus, BackOutcome, DefaultAction, ItemKind, MatchTier, Notice,
    NoticeLevel, PluginFailure, PluginFailureKind, Preview, QueryResponse, QueryScope, Score,
    SearchItem, SourceId, COMMAND_CAPABILITIES, COMMAND_PREFIX, COMMAND_RESCAN, HOST_SOURCE,
};
pub use plugin::{
    is_valid_plugin_id, strip_keyword, FeaturePlugin, Keyword, PluginError, PluginKind,
    PluginManifest, PluginScope, SearchContext, CAP_CLIPBOARD_READ, CAP_CLIPBOARD_WRITE,
};
pub use plugins::{
    ClipboardPlugin, MemoPlugin, CLIPBOARD_KEYWORD_ALT_ZH, CLIPBOARD_KEYWORD_EN,
    CLIPBOARD_KEYWORD_ZH, MEMO_PLUGIN_ID,
};
pub use ranking::{score_match, sort_ranked, RankedItem};
pub use registry::{PluginRegistry, PluginSearchOutcome};
pub use settings::{ClipboardSettings, Settings, SettingsError};
pub use sync::{
    DirtyDetail, PullOutcome, PullReport, PullResult, PushOutcome, PushReport, PushUpdateView,
    SyncBlock, SyncBlockKind, SyncControl, SyncError, SyncPhase, SyncProgress, SyncStatus,
};
pub use theme::{
    builtin_themes, dark_tokens, light_tokens, Appearance, ColorTokens, CssVar, DisabledState,
    ErrorState, FocusState, FontTokens, RadiusTokens, Rgba, SelectedState, ShadowTokens,
    SpaceTokens, StateTokens, ThemeAppearance, ThemeDocument, ThemeEntry, ThemeError, ThemeLibrary,
    ThemePalettes, ThemeSelection, ThemeState, ThemeTokens, THEME_DARK, THEME_LIGHT, THEME_SYSTEM,
};
pub use watch::{
    ChangeFilter, WatchError, WatchEventTrace, WorkspaceWatcher, DEBOUNCE, QUIET_WINDOW,
};
pub use workspace::{
    workspace_remote_of, Workspace, WorkspaceError, WorkspaceReload, WorkspaceRemote,
    WorkspaceStatus, MANIFEST_FILE, MEMOS_DIR, SETTINGS_FILE, THEMES_DIR, THEME_FILE,
    WORKSPACE_FILES,
};
