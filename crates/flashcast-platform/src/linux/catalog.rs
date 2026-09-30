//! Linux 软件发现：按 XDG 目录优先级扫描 freedesktop `.desktop` 条目。

use crate::catalog::{AppCatalog, AppEntry, CatalogError};
use crate::freedesktop::desktop_entry::locale_candidates_from_env;
use crate::freedesktop::scan::{scan_desktop_dirs, ScanOptions, ScanOutcome};
use crate::freedesktop::xdg::XdgDirs;

pub struct LinuxAppCatalog {
    dirs: XdgDirs,
}

impl Default for LinuxAppCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxAppCatalog {
    pub fn new() -> Self {
        Self {
            dirs: XdgDirs::from_env(),
        }
    }

    /// 使用显式 XDG 目录构造，供真实平台检查与测试使用。
    pub fn with_dirs(dirs: XdgDirs) -> Self {
        Self { dirs }
    }

    /// 扫描并返回完整诊断信息（跳过的条目、图标索引状态等）。
    pub fn scan_detailed(&self) -> ScanOutcome {
        let locale_candidates = locale_candidates_from_env(
            std::env::var("LANG").ok().as_deref(),
            std::env::var("LC_MESSAGES").ok().as_deref(),
            std::env::var("LC_ALL").ok().as_deref(),
        );
        let icon_themes = self.dirs.icon_themes(
            std::env::var("FLASHCAST_ICON_THEME").ok().as_deref(),
            std::env::var("GTK_THEME").ok().as_deref(),
        );
        let options = ScanOptions {
            applications_dirs: self.dirs.application_dirs(),
            icon_theme_roots: self.dirs.icon_theme_roots(),
            icon_flat_dirs: self.dirs.icon_flat_dirs(),
            icon_themes,
            locale_candidates,
            path_env: std::env::var("PATH").ok(),
        };
        scan_desktop_dirs(&options)
    }

    /// 实际存在的应用目录，用于诊断报告。
    pub fn existing_application_dirs(&self) -> Vec<std::path::PathBuf> {
        self.dirs.existing_application_dirs()
    }
}

impl AppCatalog for LinuxAppCatalog {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError> {
        let existing = self.dirs.existing_application_dirs();
        if existing.is_empty() {
            return Err(CatalogError::Unreadable(
                "未找到任何应用条目目录（XDG_DATA_HOME / XDG_DATA_DIRS）".to_string(),
            ));
        }
        Ok(self.scan_detailed().entries)
    }
}
