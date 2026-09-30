//! # flashcast-core
//!
//! Flashcast 的无头宿主。**对外的查询入口与命令入口是唯一的集成测试边界**
//! （ADR §3）：`Host::query` 与 `Host::execute`，以及 `select` /
//! `move_selection` / `set_selection` / `back` / `rescan` / 设置读写。
//!
//! 本 crate 不依赖 Tauri，因此可以在没有桌面会话的 CI runner 上完整测试。

pub mod device;
pub mod host;
pub mod manifest;
pub mod model;
pub mod plugin;
pub mod ranking;
pub mod registry;
pub mod settings;
pub mod theme;
pub mod watch;
pub mod workspace;

pub use device::{DeviceError, DeviceStore, DEVICE_STATE_FILE, KEY_WORKSPACE_PATH};
pub use host::{quick_access_commands, Host, HostDeps};
pub use manifest::{
    ManifestEntry, ManifestError, PluginManifestFile, PluginOrigin, MANIFEST_SCHEMA_VERSION,
};
pub use model::{
    ActionOutcome, ActionStatus, BackOutcome, DefaultAction, ItemKind, MatchTier, Notice,
    NoticeLevel, PluginFailure, PluginFailureKind, Preview, QueryResponse, QueryScope, Score,
    SearchItem, SourceId, COMMAND_CAPABILITIES, COMMAND_PREFIX, COMMAND_RESCAN, HOST_SOURCE,
};
pub use plugin::{
    is_valid_plugin_id, FeaturePlugin, Keyword, PluginError, PluginKind, PluginManifest, PluginScope,
    SearchContext,
};
pub use ranking::{score_match, sort_ranked, RankedItem};
pub use registry::{PluginRegistry, PluginSearchOutcome};
pub use settings::{Settings, SettingsError};
pub use theme::{
    builtin_themes, dark_tokens, light_tokens, Appearance, ColorTokens, CssVar, DisabledState,
    ErrorState, FocusState, FontTokens, RadiusTokens, Rgba, SelectedState, ShadowTokens,
    SpaceTokens, StateTokens, ThemeAppearance, ThemeDocument, ThemeEntry, ThemeError, ThemeLibrary,
    ThemePalettes, ThemeSelection, ThemeState, ThemeTokens, THEME_DARK, THEME_LIGHT, THEME_SYSTEM,
};
pub use watch::{ChangeFilter, WatchError, WorkspaceWatcher, DEBOUNCE, QUIET_WINDOW};
pub use workspace::{
    Workspace, WorkspaceError, WorkspaceReload, WorkspaceStatus, MANIFEST_FILE, MEMOS_DIR,
    SETTINGS_FILE, THEMES_DIR, THEME_FILE, WORKSPACE_FILES,
};
