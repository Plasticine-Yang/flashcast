//! 按 XDG 目录优先级扫描 `.desktop` 文件，产出可启动的软件条目。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog::{AppEntry, AppSource, IconRef};

use super::desktop_entry::{desktop_file_id, parse_desktop_entry, parse_exec};
use super::icons::IconIndex;

/// 扫描输入。所有目录都按优先级从高到低排列。
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// `$XDG_DATA_HOME/applications`、`$XDG_DATA_DIRS` 展开结果、flatpak/snap 导出目录。
    pub applications_dirs: Vec<PathBuf>,
    pub icon_theme_roots: Vec<PathBuf>,
    pub icon_flat_dirs: Vec<PathBuf>,
    /// 图标主题优先级。
    pub icon_themes: Vec<String>,
    /// 本地化候选语言，例如 `["zh_CN", "zh"]`。
    pub locale_candidates: Vec<String>,
    /// `TryExec` 查找用的 PATH。
    pub path_env: Option<String>,
}

/// 扫描结果与诊断信息。
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub entries: Vec<AppEntry>,
    /// 实际读取的 `.desktop` 文件数。
    pub files_seen: usize,
    /// 被跳过的条目及原因，供诊断报告使用。
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
    /// 图标索引是否为空（说明图标主题目录缺失，UI 应回退到占位图标）。
    pub icon_index_empty: bool,
    /// 图标索引中出现过的主题数。
    pub icon_themes_seen: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub file: PathBuf,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    NoDisplay,
    Hidden,
    NotApplication,
    MissingName,
    EmptyExec,
    TryExecMissing,
    DuplicateId,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::NoDisplay => "NoDisplay",
            SkipReason::Hidden => "Hidden",
            SkipReason::NotApplication => "非 Application 类型",
            SkipReason::MissingName => "缺少 Name",
            SkipReason::EmptyExec => "缺少可用的 Exec",
            SkipReason::TryExecMissing => "TryExec 不存在",
            SkipReason::DuplicateId => "重复的 desktop file ID",
        }
    }
}

/// 扫描桌面条目目录。同一个 desktop file ID 只保留优先级最高的那一份。
pub fn scan_desktop_dirs(options: &ScanOptions) -> ScanOutcome {
    let icon_index = IconIndex::build(
        &options.icon_theme_roots,
        &options.icon_flat_dirs,
        &options.icon_themes,
    );
    let mut outcome = ScanOutcome {
        icon_index_empty: icon_index.is_empty(),
        icon_themes_seen: icon_index.theme_roots_seen(),
        ..ScanOutcome::default()
    };
    let path_env = options.path_env.clone().unwrap_or_default();
    let mut by_id: BTreeMap<String, AppEntry> = BTreeMap::new();
    let mut seen_ids: BTreeMap<String, PathBuf> = BTreeMap::new();

    for dir in &options.applications_dirs {
        for file in desktop_files_in(dir, 0) {
            outcome.files_seen += 1;
            let source = classify_source(dir);
            let id = match desktop_file_id(&file, dir) {
                Some(id) => id,
                None => continue,
            };
            if let Some(previous) = seen_ids.get(&id) {
                outcome.skipped.push(Skipped {
                    file: file.clone(),
                    reason: SkipReason::DuplicateId,
                });
                outcome.warnings.push(format!(
                    "跳过重复条目 {id}：{} 优先于 {}",
                    previous.display(),
                    file.display()
                ));
                continue;
            }
            let content = match std::fs::read_to_string(&file) {
                Ok(content) => content,
                Err(error) => {
                    outcome
                        .warnings
                        .push(format!("无法读取 {}：{error}", file.display()));
                    continue;
                }
            };
            let parsed = parse_desktop_entry(&content, &options.locale_candidates);
            outcome.warnings.extend(
                parsed
                    .warnings
                    .iter()
                    .map(|w| format!("{}：{w}", file.display())),
            );
            let fields = parsed.fields;
            if !fields.is_launchable_application() {
                let reason = if fields.entry_type.as_deref() != Some("Application") {
                    SkipReason::NotApplication
                } else if fields.no_display {
                    SkipReason::NoDisplay
                } else if fields.hidden {
                    SkipReason::Hidden
                } else {
                    SkipReason::MissingName
                };
                outcome.skipped.push(Skipped {
                    file: file.clone(),
                    reason,
                });
                continue;
            }
            if let Some(try_exec) = fields.try_exec.as_deref() {
                if !try_exec.is_empty() && !is_executable(try_exec, &path_env) {
                    outcome.skipped.push(Skipped {
                        file: file.clone(),
                        reason: SkipReason::TryExecMissing,
                    });
                    continue;
                }
            }
            let argv = fields
                .exec
                .as_deref()
                .map(|exec| {
                    parse_exec(
                        exec,
                        Some(&file),
                        fields.icon.as_deref(),
                        fields.name.as_deref(),
                    )
                })
                .unwrap_or_default();
            if argv.is_empty() {
                outcome.skipped.push(Skipped {
                    file: file.clone(),
                    reason: SkipReason::EmptyExec,
                });
                continue;
            }
            let name = fields.name.clone().unwrap_or_else(|| id.clone());
            let icon = fields.icon.as_ref().map(|raw| IconRef {
                name: raw.clone(),
                path: icon_index.resolve(raw),
            });
            seen_ids.insert(id.clone(), file.clone());
            by_id.insert(
                id.clone(),
                AppEntry {
                    id,
                    name,
                    comment: fields.comment.clone(),
                    icon,
                    exec: argv,
                    desktop_file: Some(file),
                    working_dir: fields
                        .path
                        .as_deref()
                        .filter(|p| !p.is_empty())
                        .map(PathBuf::from),
                    wm_class: fields.startup_wm_class.clone(),
                    terminal: fields.terminal,
                    keywords: fields.keywords.clone(),
                    source,
                },
            );
        }
    }

    let mut entries: Vec<AppEntry> = by_id.into_values().collect();
    // 稳定输出：先按来源，再按 id。避免依赖文件系统返回顺序。
    entries.sort_by(|a, b| a.source.cmp(&b.source).then_with(|| a.id.cmp(&b.id)));
    outcome.entries = entries;
    outcome
}

fn classify_source(dir: &Path) -> AppSource {
    let text = dir.to_string_lossy();
    if text.contains("/flatpak/") {
        AppSource::Flatpak
    } else if text.contains("/snapd/") {
        AppSource::Snap
    } else {
        AppSource::Desktop
    }
}

/// 递归查找目录下的 `.desktop` 文件。子目录参与 ID 计算（如 `kde4/foo.desktop`）。
fn desktop_files_in(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut files = Vec::new();
    if depth > 2 {
        return files;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            files.extend(desktop_files_in(&path, depth + 1));
        } else if file_type.is_file()
            && path.extension().and_then(|e| e.to_str()) == Some("desktop")
        {
            files.push(path);
        }
    }
    files.sort();
    files
}

/// 判断 `TryExec` 指向的可执行文件是否存在。
pub fn is_executable(program: &str, path_env: &str) -> bool {
    if program.contains('/') {
        return is_executable_file(Path::new(program));
    }
    path_env
        .split(':')
        .filter(|segment| !segment.is_empty())
        .any(|segment| is_executable_file(&Path::new(segment).join(program)))
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}
