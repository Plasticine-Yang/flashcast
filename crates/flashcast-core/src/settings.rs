//! 设置模型。ticket 01 只保存在内存中并支持 JSON / TOML 序列化，
//! 真实持久化到配置工作区由 ticket 05 完成。

use flashcast_platform::hotkey::{HotkeySpec, DEFAULT_HOTKEY};
use serde::{Deserialize, Serialize};

/// 宿主设置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// 全局快捷键，采用 `Ctrl+Alt+Space` 这类写法。
    pub hotkey: String,
    /// 是否随系统启动。ticket 01 只保存该偏好，实际生效由后续 ticket 完成。
    pub launch_at_startup: bool,
    /// 空查询时最多显示多少个快速访问软件。
    pub quick_access_limit: usize,
    /// 每个插件单次搜索的超时（毫秒）。
    pub plugin_timeout_ms: u64,
    /// 已停用的插件 id。
    pub disabled_plugins: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            // 默认快捷键的选择理由见 `flashcast_platform::hotkey::DEFAULT_HOTKEY`。
            hotkey: DEFAULT_HOTKEY.to_string(),
            launch_at_startup: false,
            quick_access_limit: 6,
            plugin_timeout_ms: 400,
            disabled_plugins: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SettingsError {
    #[error("快捷键无效：{0}")]
    InvalidHotkey(String),
    #[error("快速访问项数量必须在 1 到 20 之间，当前为 {0}")]
    InvalidQuickAccessLimit(usize),
    #[error("插件超时必须在 10 到 5000 毫秒之间，当前为 {0}")]
    InvalidPluginTimeout(u64),
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
        if !(1..=20).contains(&self.quick_access_limit) {
            return Err(SettingsError::InvalidQuickAccessLimit(
                self.quick_access_limit,
            ));
        }
        if !(10..=5000).contains(&self.plugin_timeout_ms) {
            return Err(SettingsError::InvalidPluginTimeout(self.plugin_timeout_ms));
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
