//! 插件页面命令、平台默认快捷键与用户覆盖。注册结果属于外壳，不写入工作区。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default, deny_unknown_fields)]
pub struct CommandShortcuts {
    pub linux: Option<String>,
    pub windows: Option<String>,
    pub macos: Option<String>,
}
impl CommandShortcuts {
    pub fn for_platform(&self, platform: &str) -> Option<&str> {
        match platform {
            "windows" => self.windows.as_deref(),
            "macos" => self.macos.as_deref(),
            _ => self.linux.as_deref(),
        }
    }
    pub fn set(&mut self, platform: &str, value: Option<String>) {
        match platform {
            "windows" => self.windows = value,
            "macos" => self.macos = value,
            _ => self.linux = value,
        }
    }
}

/// 稳定命令标识与页面目标相分离，插件可以贡献多个进入自身页面的命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommand {
    pub id: String,
    pub title: String,
    pub plugin_id: String,
    pub defaults: CommandShortcuts,
}
impl PluginCommand {
    pub fn open_page(manifest: &crate::PluginManifest) -> Self {
        Self {
            id: format!("{}{}", crate::PLUGIN_ENTRY_PREFIX, manifest.id),
            title: format!("打开{}", manifest.name),
            plugin_id: manifest.id.clone(),
            defaults: CommandShortcuts::default(),
        }
    }
    pub fn shortcut(
        &self,
        overrides: &BTreeMap<String, CommandShortcuts>,
        platform: &str,
    ) -> String {
        overrides
            .get(&self.id)
            .and_then(|s| s.for_platform(platform))
            .or_else(|| self.defaults.for_platform(platform))
            .unwrap_or("")
            .to_string()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCommandView {
    pub id: String,
    pub title: String,
    pub plugin_id: String,
    pub platform: String,
    pub default_shortcut: String,
    pub shortcut: String,
    pub enabled: bool,
}
