//! 设置模型。ticket 01 只保存在内存中并支持 JSON / TOML 序列化，
//! 真实持久化到配置工作区由 ticket 05 完成。

use flashcast_platform::hotkey::{HotkeySpec, DEFAULT_HOTKEY};
use serde::{Deserialize, Serialize};

/// 宿主设置。
///
/// 这是配置工作区里 `settings.toml` 的格式，键名与 UI / JSON 一致使用 camelCase：
///
/// ```toml
/// hotkey = "Alt+Space"
/// launchAtStartup = false
/// quickAccessLimit = 6
/// pluginTimeoutMs = 400
/// disabledPlugins = []
/// ```
///
/// `deny_unknown_fields` 让拼错的键名直接报错，而不是被静默忽略成默认值：
/// 「无效配置保留上次有效状态并指出问题」要求错误可见。
///
/// `disabledPlugins` 是 ticket 01 的历史字段：插件启停的**唯一权威**是工作区的
/// `manifest.json`（ticket 06 建立，ticket 07 收口）。这里保留字段是为了仍能读取
/// 旧工作区，仅在清单里还没有该插件条目时用作一次性迁移，不再覆盖清单里的选择；
/// 应用也不会再写它。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct Settings {
    /// 全局快捷键，采用 `Alt+Space` 这类写法。
    pub hotkey: String,
    /// 是否随系统启动。ticket 01 只保存该偏好，实际生效由后续 ticket 完成。
    pub launch_at_startup: bool,
    /// 空查询时最多显示多少个快速访问软件。
    pub quick_access_limit: usize,
    /// 每个插件单次搜索的超时（毫秒）。
    pub plugin_timeout_ms: u64,
    /// 历史字段：旧工作区记录的停用插件。见类型文档。
    pub disabled_plugins: Vec<String>,
    /// 剪贴板历史的记录范围（暂停、保留期限、容量）。见 [`ClipboardSettings`]。
    pub clipboard: ClipboardSettings,
    /// 按稳定命令标识及平台保存；缺省用插件默认值，空串关闭。
    pub command_shortcuts: std::collections::BTreeMap<String, crate::CommandShortcuts>,
}

/// 剪贴板历史的用户控制项。
///
/// 这些都是**可迁移的偏好**，因此与其它设置一起写在工作区的 `settings.toml` 里；
/// 历史内容与附件是本机数据，绝不进入工作区（ADR §8、spec 用户故事 30）。
///
/// ```toml
/// [clipboard]
/// paused = false
/// retentionDays = 30
/// capacity = 500
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct ClipboardSettings {
    /// 暂停记录：暂停期间复制的内容不会被保存（轮询仍然推进，恢复后不会补记旧内容）。
    pub paused: bool,
    /// 保留期限（天）。超过期限的非置顶条目会被回收。
    pub retention_days: u32,
    /// 容量上限（条目数，含置顶条目）。超出时先回收最旧的未置顶条目。
    pub capacity: usize,
}

impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            paused: false,
            retention_days: 30,
            capacity: 500,
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // 默认快捷键见 `flashcast_platform::hotkey::DEFAULT_HOTKEY`。
            hotkey: DEFAULT_HOTKEY.to_string(),
            launch_at_startup: false,
            quick_access_limit: 6,
            plugin_timeout_ms: 400,
            disabled_plugins: Vec::new(),
            clipboard: ClipboardSettings::default(),
            command_shortcuts: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("快捷键无效：{0}")]
    InvalidHotkey(String),
    #[error("插件快捷键冲突：{0}")]
    CommandShortcut(String),
    #[error("快速访问项数量必须在 1 到 20 之间，当前为 {0}")]
    InvalidQuickAccessLimit(usize),
    #[error("插件超时必须在 10 到 5000 毫秒之间，当前为 {0}")]
    InvalidPluginTimeout(u64),
    #[error("剪贴板保留期限必须在 1 到 3650 天之间，当前为 {0}")]
    InvalidClipboardRetention(u32),
    #[error("剪贴板容量必须在 1 到 100000 条之间，当前为 {0}")]
    InvalidClipboardCapacity(usize),
    #[error("设置序列化失败：{0}")]
    Serialize(String),
    #[error("设置无法写入配置工作区：{0}")]
    Workspace(String),
}

impl Settings {
    /// 校验设置。无效设置必须被拒绝，并保留上一次可用状态。
    pub fn validate(&self) -> Result<(), SettingsError> {
        HotkeySpec::parse(&self.hotkey)
            .map_err(|error| SettingsError::InvalidHotkey(error.to_string()))?;
        for shortcuts in self.command_shortcuts.values() {
            for value in [&shortcuts.linux, &shortcuts.windows, &shortcuts.macos]
                .into_iter()
                .flatten()
            {
                if !value.is_empty() {
                    HotkeySpec::parse(value)
                        .map_err(|e| SettingsError::InvalidHotkey(e.to_string()))?;
                }
            }
        }
        if !(1..=20).contains(&self.quick_access_limit) {
            return Err(SettingsError::InvalidQuickAccessLimit(
                self.quick_access_limit,
            ));
        }
        if !(10..=5000).contains(&self.plugin_timeout_ms) {
            return Err(SettingsError::InvalidPluginTimeout(self.plugin_timeout_ms));
        }
        if !(1..=3650).contains(&self.clipboard.retention_days) {
            return Err(SettingsError::InvalidClipboardRetention(
                self.clipboard.retention_days,
            ));
        }
        if !(1..=100_000).contains(&self.clipboard.capacity) {
            return Err(SettingsError::InvalidClipboardCapacity(
                self.clipboard.capacity,
            ));
        }
        Ok(())
    }

    pub fn plugin_timeout(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.plugin_timeout_ms)
    }

    /// 已解析的快捷键规格。
    pub fn hotkey_spec(&self) -> Result<HotkeySpec, SettingsError> {
        HotkeySpec::parse(&self.hotkey)
            .map_err(|error| SettingsError::InvalidHotkey(error.to_string()))
    }

    pub fn to_json(&self) -> Result<String, SettingsError> {
        serde_json::to_string_pretty(self).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    pub fn from_json(value: &str) -> Result<Self, SettingsError> {
        serde_json::from_str(value).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    pub fn to_toml(&self) -> Result<String, SettingsError> {
        toml::to_string_pretty(self).map_err(|e| SettingsError::Serialize(e.to_string()))
    }

    pub fn from_toml(value: &str) -> Result<Self, SettingsError> {
        toml::from_str(value).map_err(|e| SettingsError::Serialize(e.to_string()))
    }
}
