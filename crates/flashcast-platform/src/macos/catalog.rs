//! macOS 软件发现：扫描应用目录、解析 `Info.plist`、把图标渲染成 UI 可用的 PNG。
//!
//! 纯逻辑（遍历、字段映射、身份规则）在 [`super::bundle`] 中，可在任意平台测试；
//! 本模块只补齐 macOS 专有的图标渲染，并把渲染结果写进 `IconRef.path`。
//!
//! 为什么不用 Spotlight（`mdfind`）：用户可能关闭索引，`mdfind` 还是个子进程，
//! 而且它只返回路径——每个包仍要逐个读 `Info.plist`，省不下真正的工作量。
//! 只有需要覆盖标准目录之外的应用时才值得额外跑一次 `mdfind`。

use std::path::PathBuf;

use crate::catalog::{AppCatalog, AppEntry, CatalogError};

use super::bundle::{self, ScanOptions, ScanOutcome};
use super::icons;

pub struct MacosAppCatalog {
    options: ScanOptions,
}

impl Default for MacosAppCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosAppCatalog {
    pub fn new() -> Self {
        Self {
            options: ScanOptions::default(),
        }
    }

    /// 使用显式根目录构造，供真实平台检查与测试使用。
    pub fn with_options(options: ScanOptions) -> Self {
        Self { options }
    }

    pub fn options(&self) -> &ScanOptions {
        &self.options
    }

    /// 实际存在的扫描根目录。
    pub fn existing_roots(&self) -> Vec<PathBuf> {
        self.options
            .roots
            .iter()
            .filter(|root| root.is_dir())
            .cloned()
            .collect()
    }

    /// 不存在的扫描根目录（诊断用）。
    pub fn missing_roots(&self) -> Vec<PathBuf> {
        self.options
            .roots
            .iter()
            .filter(|root| !root.is_dir())
            .cloned()
            .collect()
    }

    /// 扫描并返回完整诊断信息（跳过项、图标渲染结果）。
    pub fn scan_detailed(&self) -> CatalogOutcome {
        let scan = bundle::scan(&self.options);
        let mut entries = scan.entries.clone();
        let mut icons_rendered = 0usize;
        let mut icons_failed = Vec::new();

        for entry in entries.iter_mut() {
            let Some(bundle_path) = entry.exec.first().map(PathBuf::from) else {
                continue;
            };
            match icons::icon_file_for_bundle(&bundle_path) {
                Ok(png) => {
                    if let Some(icon) = entry.icon.as_mut() {
                        // `.icns` 不能被 webview 渲染；这里换成真实的 PNG 路径。
                        icon.path = Some(png);
                    }
                    icons_rendered += 1;
                }
                Err(error) => {
                    // 渲染失败时清掉 `.icns` 提示路径，让 UI 回退到占位图标，
                    // 而不是拿到一个无法显示的文件。
                    if let Some(icon) = entry.icon.as_mut() {
                        icon.path = None;
                    }
                    icons_failed.push((bundle_path, error.to_string()));
                }
            }
        }

        CatalogOutcome {
            entries,
            scan,
            icons_rendered,
            icons_failed,
        }
    }
}

impl AppCatalog for MacosAppCatalog {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError> {
        let existing = self.existing_roots();
        if existing.is_empty() {
            return Err(CatalogError::Unreadable(
                "未找到任何应用目录（/Applications、/System/Applications、~/Applications）"
                    .to_string(),
            ));
        }
        Ok(self.scan_detailed().entries)
    }
}

/// 扫描结果与诊断信息。
pub struct CatalogOutcome {
    pub entries: Vec<AppEntry>,
    pub scan: ScanOutcome,
    /// 成功渲染为 PNG 的图标数量。
    pub icons_rendered: usize,
    /// 渲染失败的应用包与原因。
    pub icons_failed: Vec<(PathBuf, String)>,
}
