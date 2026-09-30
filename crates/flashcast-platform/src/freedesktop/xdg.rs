//! XDG Base Directory 解析。
//!
//! 所有取值都从传入的环境变量计算，便于在任意平台用固定环境做测试。

use std::path::{Path, PathBuf};

/// 解析后的 XDG 目录集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XdgDirs {
    pub home: PathBuf,
    pub data_home: PathBuf,
    pub data_dirs: Vec<PathBuf>,
    pub config_home: PathBuf,
    pub cache_home: PathBuf,
}

impl XdgDirs {
    /// 从当前进程环境读取。
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("HOME").ok().as_deref(),
            std::env::var("XDG_DATA_HOME").ok().as_deref(),
            std::env::var("XDG_DATA_DIRS").ok().as_deref(),
            std::env::var("XDG_CONFIG_HOME").ok().as_deref(),
            std::env::var("XDG_CACHE_HOME").ok().as_deref(),
        )
    }

    /// 从显式取值计算，方便测试。
    pub fn from_values(
        home: Option<&str>,
        data_home: Option<&str>,
        data_dirs: Option<&str>,
        config_home: Option<&str>,
        cache_home: Option<&str>,
    ) -> Self {
        let home = home
            .filter(|h| !h.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let data_home = data_home
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        let data_dirs = data_dirs
            .filter(|v| !v.is_empty())
            .map(|value| {
                value
                    .split(':')
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share"),
                    PathBuf::from("/usr/share"),
                ]
            });
        let config_home = config_home
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let cache_home = cache_home
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        Self {
            home,
            data_home,
            data_dirs,
            config_home,
            cache_home,
        }
    }

    /// 应用条目目录，按优先级从高到低。
    ///
    /// 顺序为：`$XDG_DATA_HOME/applications` → 各 `$XDG_DATA_DIRS/*/applications`
    /// → 明确列出的系统目录 → flatpak 与 snap 导出目录。重复路径只保留第一次出现。
    pub fn application_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        push_unique(&mut dirs, self.data_home.join("applications"));
        for data_dir in &self.data_dirs {
            push_unique(&mut dirs, data_dir.join("applications"));
        }
        push_unique(&mut dirs, PathBuf::from("/usr/share/applications"));
        push_unique(&mut dirs, PathBuf::from("/usr/local/share/applications"));
        push_unique(&mut dirs, self.home.join(".local/share/applications"));
        // flatpak 与 snap 的导出目录通常已在 XDG_DATA_DIRS 中，这里显式补齐。
        push_unique(
            &mut dirs,
            PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        );
        push_unique(
            &mut dirs,
            self.data_home.join("flatpak/exports/share/applications"),
        );
        push_unique(
            &mut dirs,
            PathBuf::from("/var/lib/snapd/desktop/applications"),
        );
        dirs
    }

    /// 图标主题根目录（`<根>/<主题>/<尺寸>/<上下文>/<图标>`）。
    pub fn icon_theme_roots(&self) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        push_unique(&mut roots, self.data_home.join("icons"));
        for data_dir in &self.data_dirs {
            push_unique(&mut roots, data_dir.join("icons"));
        }
        push_unique(&mut roots, self.home.join(".local/share/icons"));
        push_unique(&mut roots, PathBuf::from("/usr/share/icons"));
        push_unique(&mut roots, PathBuf::from("/usr/local/share/icons"));
        roots
    }

    /// 平铺图标目录（图标文件直接放在目录下）。
    pub fn icon_flat_dirs(&self) -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        push_unique(&mut dirs, self.data_home.join("pixmaps"));
        for data_dir in &self.data_dirs {
            push_unique(&mut dirs, data_dir.join("pixmaps"));
        }
        push_unique(&mut dirs, PathBuf::from("/usr/share/pixmaps"));
        dirs
    }

    /// 图标主题优先级。`hicolor` 始终作为最后兜底。
    pub fn icon_themes(&self, explicit: Option<&str>, gtk_theme: Option<&str>) -> Vec<String> {
        let mut themes = Vec::new();
        if let Some(explicit) = explicit.filter(|v| !v.is_empty()) {
            themes.push(explicit.to_string());
        }
        if let Some(gtk) = gtk_theme.filter(|v| !v.is_empty()) {
            // `GTK_THEME=Adwaita:dark` 这类取值只取主题名。
            let name = gtk.split(':').next().unwrap_or(gtk);
            if !name.is_empty() {
                push_unique(&mut themes, name.to_string());
            }
        }
        push_unique(&mut themes, "Adwaita".to_string());
        push_unique(&mut themes, "hicolor".to_string());
        themes
    }

    /// 可用于诊断的、真实存在的应用目录。
    pub fn existing_application_dirs(&self) -> Vec<PathBuf> {
        self.application_dirs()
            .into_iter()
            .filter(|dir| dir.is_dir())
            .collect()
    }
}

fn push_unique<T: PartialEq>(list: &mut Vec<T>, value: T) {
    if !list.contains(&value) {
        list.push(value);
    }
}

/// 某个路径是否位于给定的任一目录之下。
pub fn is_under_any(path: &Path, dirs: &[PathBuf]) -> bool {
    dirs.iter().any(|dir| path.starts_with(dir))
}
