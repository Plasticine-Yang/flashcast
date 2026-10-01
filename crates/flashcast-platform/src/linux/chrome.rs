//! Linux 上的 Chrome 发现与启动。
//!
//! 用户数据目录的布局来自 `notes/research/chrome-bookmarks.md` §2 的实测表：
//! Debian/RPM 装在 `~/.config/<name>`，snap 在 `~/snap/<name>/common/<name>`，
//! flatpak 在 `~/.var/app/<app-id>/config/<name>`。`$XDG_CONFIG_HOME` 优先于 `~/.config`
//! （snap / flatpak 路径不受它影响，这一点在研究笔记里标注为未验证）。

use std::path::{Path, PathBuf};

use crate::chrome::{
    binaries_in_path, BinaryCandidate, ChromeBrand, ChromeEnvironment, ChromeError, ChromeLaunch,
    ChromeLaunchRequest, ChromeProvider, PathChromeProvider, UserDataCandidate, UserDataOrigin,
};

/// Linux 上的用户数据目录候选，按优先级排列。
pub fn user_data_candidates(home: &Path, xdg_config_home: Option<&Path>) -> Vec<UserDataCandidate> {
    let config = match xdg_config_home {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        _ => home.join(".config"),
    };
    vec![
        UserDataCandidate::new(
            ChromeBrand::Chrome,
            config.join("google-chrome"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chrome,
            home.join(".var/app/com.google.Chrome/config/google-chrome"),
            UserDataOrigin::Flatpak,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chromium,
            config.join("chromium"),
            UserDataOrigin::Default,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chromium,
            home.join("snap/chromium/common/chromium"),
            UserDataOrigin::Snap,
        ),
        UserDataCandidate::new(
            ChromeBrand::Chromium,
            home.join(".var/app/org.chromium.Chromium/config/chromium"),
            UserDataOrigin::Flatpak,
        ),
        UserDataCandidate::new(
            ChromeBrand::Edge,
            config.join("microsoft-edge"),
            UserDataOrigin::Default,
        ),
    ]
}

/// Linux 上的可执行文件候选：先按 `PATH` 查名字，再补充固定安装位置
/// （`/opt/google/chrome/chrome` 是本机实测的真实 ELF，`/snap/bin/chromium` 是 snap 包装器）。
pub fn binary_candidates(path_var: Option<&str>) -> Vec<BinaryCandidate> {
    let mut candidates = Vec::new();
    let mut push = |brand: ChromeBrand, path: PathBuf| {
        if !candidates
            .iter()
            .any(|item: &BinaryCandidate| item.path == path)
        {
            candidates.push(BinaryCandidate::new(brand, path));
        }
    };

    for path in binaries_in_path(&["google-chrome", "google-chrome-stable"], path_var) {
        push(ChromeBrand::Chrome, path);
    }
    for path in [
        "/opt/google/chrome/chrome",
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
    ] {
        push(ChromeBrand::Chrome, PathBuf::from(path));
    }
    for path in binaries_in_path(&["chromium", "chromium-browser"], path_var) {
        push(ChromeBrand::Chromium, path);
    }
    for path in [
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
        "/snap/bin/chromium",
    ] {
        push(ChromeBrand::Chromium, PathBuf::from(path));
    }
    for path in binaries_in_path(&["microsoft-edge", "microsoft-edge-stable"], path_var) {
        push(ChromeBrand::Edge, path);
    }
    for path in ["/usr/bin/microsoft-edge"] {
        push(ChromeBrand::Edge, PathBuf::from(path));
    }
    candidates
}

/// 从环境变量构造候选：`HOME`、`XDG_CONFIG_HOME`、`PATH`。
pub fn candidates_from_env() -> (Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute());
    let path_var = std::env::var("PATH").ok();
    let binaries = binary_candidates(path_var.as_deref());
    let user_data = match home {
        Some(home) => user_data_candidates(&home, xdg.as_deref()),
        None => Vec::new(),
    };
    (binaries, user_data)
}

/// Linux 的 Chrome 适配器。
pub struct LinuxChromeProvider {
    inner: PathChromeProvider,
}

impl Default for LinuxChromeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxChromeProvider {
    pub fn new() -> Self {
        let (binaries, user_data) = candidates_from_env();
        Self {
            inner: PathChromeProvider::new(binaries, user_data),
        }
    }
}

impl ChromeProvider for LinuxChromeProvider {
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
    fn user_data_candidates_cover_deb_snap_and_flatpak_paths() {
        let home = Path::new("/home/u");
        let candidates = user_data_candidates(home, None);
        let paths: Vec<&Path> = candidates.iter().map(|item| item.path.as_path()).collect();
        assert!(paths.contains(&Path::new("/home/u/.config/google-chrome")));
        assert!(paths.contains(&Path::new("/home/u/snap/chromium/common/chromium")));
        assert!(paths.contains(&Path::new(
            "/home/u/.var/app/org.chromium.Chromium/config/chromium"
        )));
        assert!(paths.contains(&Path::new(
            "/home/u/.var/app/com.google.Chrome/config/google-chrome"
        )));
        let snap = candidates
            .iter()
            .find(|item| item.origin == UserDataOrigin::Snap)
            .expect("snap 路径必须在候选里");
        assert!(
            !snap.requires_user_data_dir_switch(),
            "snap 的目录由启动包装器映射，不应显式传 --user-data-dir"
        );
    }

    #[test]
    fn xdg_config_home_overrides_dot_config() {
        let candidates = user_data_candidates(Path::new("/home/u"), Some(Path::new("/xdg")));
        assert_eq!(
            candidates[0].path,
            PathBuf::from("/xdg/google-chrome"),
            "$XDG_CONFIG_HOME 必须优先"
        );
        // 相对路径的 XDG_CONFIG_HOME 不合法，退回 ~/.config。
        let fallback = user_data_candidates(Path::new("/home/u"), Some(Path::new("relative")));
        assert_eq!(
            fallback[0].path,
            PathBuf::from("/home/u/.config/google-chrome")
        );
    }
}
