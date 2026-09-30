//! 已安装软件的发现。平台实现按 Linux freedesktop / Windows 开始菜单 / macOS
//! LaunchServices 各自实现，宿主只消费 [`AppEntry`]。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 软件条目来自哪个安装来源。用于排序时的来源优先级与用户可见的来源标注。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppSource {
    /// 发行版或用户手动安装的 freedesktop `.desktop` 条目。
    Desktop,
    Flatpak,
    Snap,
}

impl AppSource {
    pub fn as_str(self) -> &'static str {
        match self {
            AppSource::Desktop => "desktop",
            AppSource::Flatpak => "flatpak",
            AppSource::Snap => "snap",
        }
    }
}

/// 图标引用。
///
/// `path` 为解析后的本地文件路径（可能是 `.png`、`.svg` 或 `.xpm`）；
/// 无法在图标主题目录中找到时保留原始名称，供 UI 回退到内置占位图标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IconRef {
    pub name: String,
    pub path: Option<PathBuf>,
}

impl IconRef {
    pub fn unresolved(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            path: None,
        }
    }
}

/// 一个可启动的软件条目。`id` 必须跨查询与重启稳定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    /// 稳定标识。Linux 上为 desktop file ID，例如 `org.gnome.Nautilus.desktop`。
    pub id: String,
    pub name: String,
    pub comment: Option<String>,
    pub icon: Option<IconRef>,
    /// 已按 freedesktop 规则分词并展开字段码的 argv。绝不会经过 shell。
    pub exec: Vec<String>,
    /// 该条目定义所在的文件，用于工作目录与 `%k` 展开。
    pub desktop_file: Option<PathBuf>,
    /// `Path=` 指定的工作目录；未指定时为 `None`。
    pub working_dir: Option<PathBuf>,
    /// 窗口类名，用于后续把焦点恢复到正确窗口；未知时为 `None`。
    pub wm_class: Option<String>,
    pub terminal: bool,
    /// `Keywords=` 中的关键词，参与元数据匹配。
    pub keywords: Vec<String>,
    pub source: AppSource,
}

impl AppEntry {
    /// 启动该软件所需的 argv；空 `exec` 表示条目不可启动。
    pub fn argv(&self) -> Option<&[String]> {
        if self.exec.is_empty() {
            None
        } else {
            Some(&self.exec)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("软件目录不可读：{0}")]
    Unreadable(String),
    #[error("当前平台尚未实现软件发现")]
    Unsupported,
}

/// 发现已安装软件。`scan` 必须可以在没有桌面会话的环境中调用。
pub trait AppCatalog: Send + Sync {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError>;
}
