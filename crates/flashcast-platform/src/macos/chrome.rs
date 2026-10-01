//! macOS 上的 Chrome 发现。
//!
//! 与 `macos/bundle.rs` 一样，候选路径推导是**纯函数**，在 Linux 开发机上也能被测试；
//! 真正启动进程的适配器按 `cfg(target_os = "macos")` 条件编译。
//!
//! 未覆盖：本机是 Linux，无法在 macOS 上实测这些路径；`/Applications` 之外的自定义
//! 安装位置（例如 Homebrew Cask 的非标准前缀）由后续版本按需补充。

use std::path::{Path, PathBuf};

use crate::chrome::{BinaryCandidate, ChromeBrand, UserDataCandidate, UserDataOrigin};

#[cfg(target_os = "macos")]
use crate::chrome::{
    ChromeEnvironment, ChromeError, ChromeLaunch, ChromeLaunchRequest, ChromeProvider,
    PathChromeProvider,
};

/// macOS 上的用户数据目录候选。
pub fn user_data_candidates(home: &Path) -> Vec<UserDataCandidate> {
    let support = home.join("Library").join("Application Support");
    vec![
        UserDataCandidate::new(
            ChromeBrand::Chrome,
            support.join("Google").join("Chrome"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chromium,
            support.join("Chromium"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Edge,
            support.join("Microsoft Edge"),
            UserDataOrigin::Default,
        ),
    ]
}

/// macOS 上的可执行文件候选：系统 `/Applications` 与用户级 `~/Applications`。
pub fn binary_candidates(home: Option<&Path>) -> Vec<BinaryCandidate> {
    let mut candidates: Vec<BinaryCandidate> = Vec::new();
    let mut push = |brand: ChromeBrand, path: PathBuf| {
        if !candidates.iter().any(|item| item.path == path) {
            candidates.push(BinaryCandidate::new(brand, path));
        }
    };
    let bundles: [(ChromeBrand, &str, &str); 3] = [
        (ChromeBrand::Chrome, "Google Chrome.app", "Google Chrome"),
        (ChromeBrand::Chromium, "Chromium.app", "Chromium"),
        (ChromeBrand::Edge, "Microsoft Edge.app", "Microsoft Edge"),
    ];
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = home {
        roots.push(home.join("Applications"));
    }
    for root in roots {
        for (brand, bundle, executable) in &bundles {
            push(
                *brand,
                root.join(bundle)
                    .join("Contents")
                    .join("MacOS")
                    .join(executable),
            );
        }
    }
    candidates
}

/// 从环境变量构造候选。
#[cfg(target_os = "macos")]
pub fn candidates_from_env() -> (Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let binaries = binary_candidates(home.as_deref());
    let user_data = match home {
        Some(home) => user_data_candidates(&home),
        None => Vec::new(),
    };
    (binaries, user_data)
}

/// macOS 的 Chrome 适配器。
#[cfg(target_os = "macos")]
pub struct MacosChromeProvider {
    inner: PathChromeProvider,
}

#[cfg(target_os = "macos")]
impl Default for MacosChromeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosChromeProvider {
    pub fn new() -> Self {
        let (binaries, user_data) = candidates_from_env();
        Self {
            inner: PathChromeProvider::new(binaries, user_data),
        }
    }
}

#[cfg(target_os = "macos")]
impl ChromeProvider for MacosChromeProvider {
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
    fn user_data_dir_lives_in_application_support() {
        let candidates = user_data_candidates(Path::new("/Users/me"));
        assert_eq!(
            candidates[0].path,
            PathBuf::from("/Users/me/Library/Application Support/Google/Chrome")
        );
        assert_eq!(
            candidates[2].path,
            PathBuf::from("/Users/me/Library/Application Support/Microsoft Edge")
        );
    }

    #[test]
    fn binary_candidates_live_inside_the_app_bundle() {
        let candidates = binary_candidates(Some(Path::new("/Users/me")));
        let paths: Vec<&Path> = candidates.iter().map(|item| item.path.as_path()).collect();
        assert!(paths.contains(&Path::new(
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        )));
        assert!(paths.contains(&Path::new(
            "/Users/me/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
        )));
        assert!(paths.contains(&Path::new(
            "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"
        )));
    }
}
