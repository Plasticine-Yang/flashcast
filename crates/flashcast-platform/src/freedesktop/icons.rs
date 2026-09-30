//! freedesktop 图标解析。
//!
//! 按 Icon Theme Specification 的目录布局
//! （`<主题根>/<主题>/<尺寸>/<上下文>/<名称>.<扩展名>`）建立一次索引，
//! 之后按「用户主题 → hicolor → 任意主题 → `/usr/share/pixmaps`」的顺序解析。
//!
//! 索引在首次使用时建立一次，避免每个条目都去遍历磁盘。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

const EXTENSIONS: [&str; 3] = ["png", "svg", "xpm"];

/// 尺寸偏好：在启动器 20px 显示与 2x 缩放之间取平衡。
const SIZE_PREFERENCE: [&str; 14] = [
    "128x128",
    "96x96",
    "64x64",
    "scalable",
    "256x256",
    "48x48",
    "512x512",
    "32x32",
    "24x24",
    "22x22",
    "16x16",
    "symbolic",
    "fixed",
    "8x8",
];

/// 图标候选的偏好分数，越小越好。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Score {
    theme: usize,
    size: usize,
    extension: usize,
}

#[derive(Debug, Default)]
pub struct IconIndex {
    /// (主题, 名称) -> (最佳路径, 分数)。
    themed: HashMap<(String, String), (PathBuf, Score)>,
    /// 名称 -> 任意主题下的最佳路径，作为兜底。
    by_stem: HashMap<String, (PathBuf, Score)>,
    /// `/usr/share/pixmaps` 这类平铺目录。
    flat: HashMap<String, PathBuf>,
    /// 构建时给定的主题优先级。
    themes: Vec<String>,
}

impl IconIndex {
    /// 建立索引。`theme_roots` 为图标主题根目录（如 `/usr/share/icons`），
    /// `flat_dirs` 为平铺目录（如 `/usr/share/pixmaps`）。
    pub fn build(theme_roots: &[PathBuf], flat_dirs: &[PathBuf], themes: &[String]) -> Self {
        let mut index = IconIndex {
            themes: themes.to_vec(),
            ..IconIndex::default()
        };
        for root in theme_roots {
            index.index_theme_root(root);
        }
        for dir in flat_dirs {
            index.index_flat_dir(dir);
        }
        index
    }

    /// 按构建时给定的主题优先级解析 `Icon=` 取值。
    ///
    /// 含 `/` 的取值按路径处理（绝对路径或相对当前目录），不查主题。
    pub fn resolve(&self, icon: &str) -> Option<PathBuf> {
        self.resolve_with_themes(icon, &self.themes)
    }

    /// 按显式主题优先级解析。`themes` 越靠前优先级越高。
    pub fn resolve_with_themes(&self, icon: &str, themes: &[String]) -> Option<PathBuf> {
        if icon.is_empty() {
            return None;
        }
        if icon.contains('/') {
            let path = PathBuf::from(icon);
            return path.is_file().then_some(path);
        }
        let stem = strip_extension(icon);
        for theme in themes {
            if let Some((path, _)) = self.themed.get(&(theme.clone(), stem.to_string())) {
                return Some(path.clone());
            }
        }
        if let Some((path, _)) = self.by_stem.get(stem) {
            return Some(path.clone());
        }
        self.flat.get(stem).cloned()
    }

    /// 索引中是否一个图标都没有（用于诊断：说明图标主题缺失）。
    pub fn is_empty(&self) -> bool {
        self.themed.is_empty() && self.flat.is_empty()
    }

    pub fn theme_roots_seen(&self) -> usize {
        self.themed
            .keys()
            .map(|(theme, _)| theme.as_str())
            .collect::<std::collections::BTreeSet<_>>()
            .len()
    }

    fn index_theme_root(&mut self, root: &Path) {
        let Ok(themes) = std::fs::read_dir(root) else {
            return;
        };
        for theme_entry in themes.flatten() {
            if !theme_entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let theme = theme_entry.file_name().to_string_lossy().into_owned();
            let theme_rank = self
                .themes
                .iter()
                .position(|t| *t == theme)
                .unwrap_or(self.themes.len());
            let theme_dir = theme_entry.path();
            // 深度限制：主题/尺寸/上下文/文件 共 3 层目录。
            walk(&theme_dir, 3, &mut |path, relative_depth| {
                let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                    return;
                };
                let ext = ext.to_ascii_lowercase();
                if !EXTENSIONS.contains(&ext.as_str()) {
                    return;
                }
                let Some(file_name) = path.file_name().and_then(|n| n.to_str()) else {
                    return;
                };
                let stem = strip_extension(file_name).to_string();
                if stem.is_empty() {
                    return;
                }
                // relative_depth == 1 表示文件直接位于主题目录下（无尺寸目录）。
                let size_dir = if relative_depth >= 2 {
                    path.strip_prefix(&theme_dir)
                        .ok()
                        .and_then(|rel| rel.components().next())
                        .map(|c| c.as_os_str().to_string_lossy().into_owned())
                } else {
                    None
                };
                let score = Score {
                    theme: theme_rank,
                    size: size_rank(size_dir.as_deref()),
                    extension: ext_rank(&ext),
                };
                let theme_key = (theme.clone(), stem.clone());
                insert_if_better(&mut self.themed, theme_key, &path, score);
                insert_if_better(&mut self.by_stem, stem, &path, score);
            });
        }
    }

    fn index_flat_dir(&mut self, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
                continue;
            };
            if !EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
                continue;
            }
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let stem = strip_extension(name).to_string();
            self.flat.entry(stem).or_insert(path);
        }
    }
}

fn insert_if_better<K: std::hash::Hash + Eq>(
    map: &mut HashMap<K, (PathBuf, Score)>,
    key: K,
    path: &Path,
    score: Score,
) {
    match map.get(&key) {
        Some((_, existing)) if *existing <= score => {}
        _ => {
            map.insert(key, (path.to_path_buf(), score));
        }
    }
}

fn size_rank(size: Option<&str>) -> usize {
    size.and_then(|s| SIZE_PREFERENCE.iter().position(|p| *p == s))
        .unwrap_or(SIZE_PREFERENCE.len())
}

fn ext_rank(ext: &str) -> usize {
    EXTENSIONS
        .iter()
        .position(|e| *e == ext)
        .unwrap_or(EXTENSIONS.len())
}

fn strip_extension(name: &str) -> &str {
    for ext in EXTENSIONS {
        if let Some(stem) = name.strip_suffix(&format!(".{ext}")) {
            return stem;
        }
    }
    name
}

fn walk(dir: &Path, max_depth: usize, visit: &mut impl FnMut(&Path, usize)) {
    walk_inner(dir, 0, max_depth, visit);
}

fn walk_inner(dir: &Path, depth: usize, max_depth: usize, visit: &mut impl FnMut(&Path, usize)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            if depth < max_depth {
                walk_inner(&path, depth + 1, max_depth, visit);
            }
        } else if file_type.is_file() {
            visit(&path, depth + 1);
        }
    }
}
