//! Windows 上的 Chrome 发现。
//!
//! 候选路径的推导是**纯函数**，因此在 Linux 开发机上也能被测试（与
//! `windows/launch_plan.rs` 等同一条分界）；真正调用 `Command` 的启动与需要
//! `%LOCALAPPDATA%` 取值的入口按 `cfg(target_os = "windows")` 条件编译。
//!
//! 未实现：注册表 `App Paths\chrome.exe` 的查询（研究笔记称为「canonical」，但它只能在
//! Windows 上验证）。这里覆盖研究笔记列出的三处安装位置；未覆盖项记录在 ticket 13。

use std::path::{Path, PathBuf};

use crate::chrome::{BinaryCandidate, ChromeBrand, UserDataCandidate, UserDataOrigin};

#[cfg(target_os = "windows")]
use crate::chrome::{
    ChromeEnvironment, ChromeError, ChromeLaunch, ChromeLaunchRequest, ChromeProvider,
    PathChromeProvider,
};

/// Windows 上的用户数据目录候选。
pub fn user_data_candidates(local_app_data: &Path) -> Vec<UserDataCandidate> {
    vec![
        UserDataCandidate::new(
            ChromeBrand::Chrome,
            local_app_data
                .join("Google")
                .join("Chrome")
                .join("User Data"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chromium,
            local_app_data.join("Chromium").join("User Data"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Edge,
            local_app_data
                .join("Microsoft")
                .join("Edge")
                .join("User Data"),
            UserDataOrigin::Default,
        ),
    ]
}

/// Windows 上的可执行文件候选：`%PROGRAMFILES%`、`%PROGRAMFILES(X86)%` 与
/// 每用户安装的 `%LOCALAPPDATA%`。
pub fn binary_candidates(
    program_files: Option<&Path>,
    program_files_x86: Option<&Path>,
    local_app_data: Option<&Path>,
) -> Vec<BinaryCandidate> {
    let mut candidates: Vec<BinaryCandidate> = Vec::new();
    let mut push = |brand: ChromeBrand, path: PathBuf| {
        if !candidates.iter().any(|item| item.path == path) {
            candidates.push(BinaryCandidate::new(brand, path));
        }
    };

    for root in [program_files, program_files_x86] {
        if let Some(root) = root {
            push(
                ChromeBrand::Chrome,
                root.join("Google")
                    .join("Chrome")
                    .join("Application")
                    .join("chrome.exe"),
            );
            push(
                ChromeBrand::Chromium,
                root.join("Chromium").join("Application").join("chrome.exe"),
            );
            push(
                ChromeBrand::Edge,
                root.join("Microsoft")
                    .join("Edge")
                    .join("Application")
                    .join("msedge.exe"),
            );
        }
    }
    if let Some(root) = local_app_data {
        push(
            ChromeBrand::Chrome,
            root.join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe"),
        );
        push(
            ChromeBrand::Chromium,
            root.join("Chromium").join("Application").join("chrome.exe"),
        );
        push(
            ChromeBrand::Edge,
            root.join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe"),
        );
    }
    candidates
}

/// 从环境变量构造候选。
#[cfg(target_os = "windows")]
pub fn candidates_from_env() -> (Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
    let var = |name: &str| std::env::var_os(name).map(PathBuf::from);
    let program_files = var("PROGRAMFILES");
    let program_files_x86 = var("PROGRAMFILES(X86)");
    let local_app_data = var("LOCALAPPDATA");
    let binaries = binary_candidates(
        program_files.as_deref(),
        program_files_x86.as_deref(),
        local_app_data.as_deref(),
    );
    let user_data = match local_app_data {
        Some(root) => user_data_candidates(&root),
        None => Vec::new(),
    };
    (binaries, user_data)
}

/// Windows 的 Chrome 适配器。
#[cfg(target_os = "windows")]
pub struct WindowsChromeProvider {
    inner: PathChromeProvider,
}

#[cfg(target_os = "windows")]
impl Default for WindowsChromeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "windows")]
impl WindowsChromeProvider {
    pub fn new() -> Self {
        let (binaries, user_data) = candidates_from_env();
        Self {
            inner: PathChromeProvider::new(binaries, user_data),
        }
    }
}

#[cfg(target_os = "windows")]
impl ChromeProvider for WindowsChromeProvider {
    fn discover(&self) -> Result<ChromeEnvironment, ChromeError> {
        self.inner.discover()
    }

    fn launch(&self, request: &ChromeLaunchRequest) -> Result<ChromeLaunch, ChromeError> {
        self.inner.launch(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_data_dir_is_under_local_app_data() {
        let local = Path::new(r"C:\Users\me\AppData\Local");
        let candidates = user_data_candidates(local);
        assert_eq!(
            candidates[0].path,
            local.join("Google").join("Chrome").join("User Data")
        );
        assert_eq!(candidates[0].origin, UserDataOrigin::Default);
        assert!(
            !candidates[0].requires_user_data_dir_switch(),
            "默认用户数据目录不需要显式传 --user-data-dir"
        );
    }

    #[test]
    fn binary_candidates_cover_program_files_and_per_user_installs() {
        let program_files = Path::new(r"C:\Program Files");
        let program_files_x86 = Path::new(r"C:\Program Files (x86)");
        let local = Path::new(r"C:\Users\me\AppData\Local");
        let candidates =
            binary_candidates(Some(program_files), Some(program_files_x86), Some(local));
        let chrome = |root: &Path| {
            root.join("Google")
                .join("Chrome")
                .join("Application")
                .join("chrome.exe")
        };
        let paths: Vec<&Path> = candidates.iter().map(|item| item.path.as_path()).collect();
        assert!(paths.contains(&chrome(program_files).as_path()));
        assert!(paths.contains(&chrome(program_files_x86).as_path()));
        assert!(paths.contains(&chrome(local).as_path()));
        assert!(paths.contains(
            &program_files
                .join("Microsoft")
                .join("Edge")
                .join("Application")
                .join("msedge.exe")
                .as_path()
        ));
        assert!(binary_candidates(None, None, None).is_empty());
    }
}
