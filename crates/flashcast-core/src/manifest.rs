//! 插件清单：插件标识、种类、版本与启用状态（ADR §6、§8）。
//!
//! 清单是**唯一**记录「有哪些插件、是否启用」的地方，保存在配置工作区的
//! `manifest.json`（人类可读的 JSON，原子写入）。功能插件（ticket 07 起的官方插件）
//! 与主题插件共用这张清单，因此默认主题的加载路径是
//!
//! ```text
//! manifest.json 里的条目（kind = theme，enabled = true）
//!     → ThemeLibrary 按 id 提供已解析的主题文档
//!     → 宿主解析出具名 token
//! ```
//!
//! 而不是「按 id 硬编码分支」。清单里出现但无法加载的主题（文件缺失、JSON 损坏、
//! 校验失败）不会让宿主失去可用外观：宿主保留上一次可用主题并在主题状态里给出
//! 中文原因（见 [`crate::theme`] 与 `Host::theme_state`）。

use serde::{Deserialize, Serialize};

use crate::plugin::{is_valid_plugin_id, PluginContract, PluginKind, PluginManifest};
use crate::theme::{ThemeAppearance, ThemeDocument};

/// 清单文件的格式版本。
pub const MANIFEST_SCHEMA_VERSION: u32 = 1;

/// 插件来源，决定它能不能被移除。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum PluginOrigin {
    /// 随应用提供（默认主题）。
    #[default]
    Builtin,
    /// 由宿主代码注册的功能插件（ticket 07 起的官方插件）。
    Registered,
    /// 用户从本地主题包安装。
    Installed,
}

impl PluginOrigin {
    pub fn label_zh(self) -> &'static str {
        match self {
            PluginOrigin::Builtin => "内置",
            PluginOrigin::Registered => "随应用提供",
            PluginOrigin::Installed => "已安装",
        }
    }

    /// 是否可以移除。
    pub fn removable(self) -> bool {
        matches!(self, PluginOrigin::Installed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("插件清单无效：{0}")]
    Invalid(String),
    #[error("插件清单读写失败：{0}")]
    Io(String),
}

impl ManifestError {
    pub fn invalid(reason: impl Into<String>) -> Self {
        ManifestError::Invalid(reason.into())
    }
}

/// 清单里的一条插件记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestEntry {
    pub id: String,
    pub name: String,
    pub kind: PluginKind,
    pub version: String,
    pub enabled: bool,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub origin: PluginOrigin,
    /// 主题插件专有：声明的外观偏好。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub appearance: Option<ThemeAppearance>,
    #[serde(default)]
    pub contract: PluginContract,
}

impl ManifestEntry {
    /// 由功能插件的运行时清单生成记录。
    pub fn from_feature(manifest: &PluginManifest, enabled: bool) -> Self {
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            kind: PluginKind::Feature,
            version: manifest.version.clone(),
            enabled,
            keywords: manifest.keywords.clone(),
            capabilities: manifest.capabilities.clone(),
            origin: PluginOrigin::Registered,
            appearance: None,
            contract: manifest.contract.clone(),
        }
    }

    /// 由主题文档生成记录。内置主题与本地安装的主题包都走这里。
    pub fn from_theme(document: &ThemeDocument, enabled: bool) -> Self {
        Self {
            id: document.id.clone(),
            name: document.name.clone(),
            kind: PluginKind::Theme,
            version: document.version.clone(),
            enabled,
            keywords: Vec::new(),
            capabilities: Vec::new(),
            origin: if document.is_builtin() {
                PluginOrigin::Builtin
            } else {
                PluginOrigin::Installed
            },
            appearance: Some(document.appearance),
            contract: document.contract.clone().unwrap_or_default(),
        }
    }

    pub fn is_theme(&self) -> bool {
        self.kind == PluginKind::Theme
    }

    /// 还原成宿主内部使用的功能插件清单。
    pub fn to_feature_manifest(&self) -> PluginManifest {
        PluginManifest {
            id: self.id.clone(),
            name: self.name.clone(),
            kind: self.kind,
            version: self.version.clone(),
            keywords: self.keywords.clone(),
            capabilities: self.capabilities.clone(),
            contract: self.contract.clone(),
        }
    }

    fn validate(&self) -> Result<(), ManifestError> {
        self.contract.validate().map_err(ManifestError::invalid)?;
        if self.id == crate::theme::THEME_ARC && !self.enabled {
            return Err(ManifestError::invalid("内置电弧是恢复基线，不能停用"));
        }
        if !is_valid_plugin_id(&self.id) {
            return Err(ManifestError::invalid(format!(
                "插件标识不合法（{}）：只能使用小写字母、数字、点、下划线与连字符，最长 64 个字符",
                self.id
            )));
        }
        if self.name.trim().is_empty() {
            return Err(ManifestError::invalid(format!(
                "插件「{}」缺少名称",
                self.id
            )));
        }
        if self.version.trim().is_empty() {
            return Err(ManifestError::invalid(format!(
                "插件「{}」缺少版本",
                self.id
            )));
        }
        if self.kind == PluginKind::Feature && self.appearance.is_some() {
            return Err(ManifestError::invalid(format!(
                "功能插件「{}」不应带 appearance 字段",
                self.id
            )));
        }
        if self.kind == PluginKind::Theme && self.appearance.is_none() {
            return Err(ManifestError::invalid(format!(
                "主题插件「{}」缺少 appearance 字段",
                self.id
            )));
        }
        Ok(())
    }
}

/// `manifest.json` 的内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginManifestFile {
    pub schema_version: u32,
    pub plugins: Vec<ManifestEntry>,
}

impl Default for PluginManifestFile {
    fn default() -> Self {
        Self::defaults()
    }
}

impl PluginManifestFile {
    /// 随应用提供的默认清单：浅色、深色、跟随系统三个默认主题。
    ///
    /// 「默认主题经清单加载」的入口就在这里：主题解析路径不出现按 id 的硬编码分支。
    pub fn defaults() -> Self {
        Self {
            schema_version: MANIFEST_SCHEMA_VERSION,
            plugins: crate::theme::builtin_themes()
                .iter()
                .map(|document| ManifestEntry::from_theme(document, true))
                .collect(),
        }
    }

    pub fn from_json(text: &str) -> Result<Self, ManifestError> {
        let file: PluginManifestFile = serde_json::from_str(text)
            .map_err(|error| ManifestError::invalid(format!("JSON 解析失败：{error}")))?;
        file.validate()?;
        Ok(file)
    }

    pub fn to_json(&self) -> Result<String, ManifestError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ManifestError::Io(format!("序列化失败：{error}")))
    }

    pub fn validate(&self) -> Result<(), ManifestError> {
        if self.schema_version != MANIFEST_SCHEMA_VERSION {
            return Err(ManifestError::invalid(format!(
                "格式版本必须是 {MANIFEST_SCHEMA_VERSION}，当前为 {}",
                self.schema_version
            )));
        }
        let mut seen: Vec<&str> = Vec::new();
        for entry in &self.plugins {
            entry.validate()?;
            if seen.contains(&entry.id.as_str()) {
                return Err(ManifestError::invalid(format!(
                    "出现重复的插件标识：{}",
                    entry.id
                )));
            }
            seen.push(&entry.id);
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&ManifestEntry> {
        self.plugins.iter().find(|entry| entry.id == id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut ManifestEntry> {
        self.plugins.iter_mut().find(|entry| entry.id == id)
    }

    pub fn entries(&self) -> &[ManifestEntry] {
        &self.plugins
    }

    /// 设置启用状态。条目不存在时返回 `false`。
    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        match self.get_mut(id) {
            Some(entry) => {
                entry.enabled = enabled;
                true
            }
            None => false,
        }
    }

    /// 按 id 新增或覆盖一条记录，保持原有顺序。
    pub fn upsert(&mut self, entry: ManifestEntry) {
        match self.get_mut(&entry.id) {
            Some(existing) => *existing = entry,
            None => self.plugins.push(entry),
        }
    }

    pub fn remove(&mut self, id: &str) -> Option<ManifestEntry> {
        let index = self.plugins.iter().position(|entry| entry.id == id)?;
        Some(self.plugins.remove(index))
    }

    /// 缺什么补什么：清单文件里没有的默认条目 / 已注册功能插件追加进来。
    ///
    /// 手写清单里删掉的默认主题会被补回（默认主题始终可用），而已有条目的启用状态
    /// 以文件为准。返回是否发生了变化，调用方据此决定要不要落盘。
    pub fn merged_with(&self, extra: Vec<ManifestEntry>) -> (Self, bool) {
        let mut merged = self.clone();
        let mut changed = false;
        for entry in extra {
            if merged.get(&entry.id).is_none() {
                merged.plugins.push(entry);
                changed = true;
            }
        }
        (merged, changed)
    }
}
