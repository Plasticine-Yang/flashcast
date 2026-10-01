//! # flashcast-platform
//!
//! Flashcast 的平台适配层。按能力拆分的 trait 定义在这里，各平台实现按
//! `#[cfg(target_os)]` 分模块，测试替身只在 [`fake`]（feature `fake`）中。
//!
//! v0.1.0 的 Linux（ticket 01）、Windows（ticket 02）与 macOS（ticket 03）实现分别
//! 在 [`linux`]、[`windows`] 与 [`macos`]；其余目标由 [`unsupported`] 中的桩实现
//! 报告「不支持」，保证宿主在各平台上都能编译并如实报告能力状态。

pub mod capability;
pub mod catalog;
pub mod chrome;
pub mod clipboard;
pub mod focus;
pub mod freedesktop;
pub mod hotkey;
pub mod launch;
pub mod launch_request;
pub mod macos;
pub mod paste;
pub mod shortcut;
pub mod unsupported;
pub mod windows;

#[cfg(feature = "fake")]
pub mod fake;

/// `global-hotkey` 后端的共享实现，由 Linux、Windows 与 macOS 的快捷键适配器使用。
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub mod hotkey_backend;

#[cfg(target_os = "linux")]
pub mod linux;

use std::sync::Arc;

pub use capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};
pub use catalog::{AppCatalog, AppEntry, AppSource, CatalogError, IconRef};
pub use chrome::{
    build_open_args, validate_open_url, BinaryCandidate, ChromeBrand, ChromeEnvironment,
    ChromeError, ChromeLaunch, ChromeLaunchRequest, ChromeProfile, ChromeProvider,
    PathChromeProvider, UserDataCandidate, UserDataOrigin,
};
pub use clipboard::{
    cf_html_bytes, cf_html_fragment, fingerprint, ClipboardAccess, ClipboardCapture,
    ClipboardContent, ClipboardError, ClipboardFormatKind, ClipboardPoll, ClipboardSkippedFormat,
    ClipboardSourceApp, ClipboardWatcher, ClipboardWriteReport,
};
pub use focus::{same_app, FocusError, FocusTracker, FocusedApp};
pub use hotkey::{HotkeySpec, HotkeySpecError, Key, Modifier, DEFAULT_HOTKEY};
pub use launch::{AppLauncher, LaunchError, LaunchReceipt};
pub use launch_request::LaunchRequest;
pub use paste::{manual_paste_hint, manual_paste_message, PasteError, Paster};
pub use shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

/// 宿主注入使用的适配器集合。
#[derive(Clone)]
pub struct PlatformAdapters {
    pub catalog: Arc<dyn AppCatalog>,
    pub launcher: Arc<dyn AppLauncher>,
    pub focus: Arc<dyn FocusTracker>,
    pub hotkeys: Arc<dyn HotkeyManager>,
    pub capabilities: Arc<dyn CapabilityProbe>,
    pub clipboard: Arc<dyn ClipboardAccess>,
    /// 剪贴板变化监听（ADR §5，ticket 09）。剪贴板历史据此在后台捕获复制事件；
    /// 停用插件后宿主不再轮询它。
    pub clipboard_watcher: Arc<dyn ClipboardWatcher>,
    /// Chrome 发现与启动（ADR §5 的 `ChromeProvider`）。
    pub chrome: Arc<dyn ChromeProvider>,
    /// 合成粘贴（ADR §5）。只有宿主在核对过「前台确实是唤起前的应用」之后才调用它。
    pub paster: Arc<dyn Paster>,
}

/// 当前平台的适配器集合。
///
/// Linux、Windows 与 macOS 使用各自的真实实现；其余平台使用 [`unsupported`]
/// 的桩实现，如实报告「不支持」而不是伪造成功。
pub fn current() -> PlatformAdapters {
    #[cfg(target_os = "linux")]
    {
        PlatformAdapters {
            catalog: Arc::new(linux::LinuxAppCatalog::new()),
            launcher: Arc::new(linux::LinuxLauncher::new()),
            focus: Arc::new(linux::LinuxFocusTracker::new()),
            hotkeys: Arc::new(linux::LinuxHotkeyManager::new()),
            capabilities: Arc::new(linux::LinuxCapabilityProbe::new()),
            clipboard: Arc::new(linux::LinuxClipboard::new()),
            clipboard_watcher: Arc::new(linux::LinuxClipboardWatcher::new()),
            chrome: Arc::new(linux::LinuxChromeProvider::new()),
            paster: Arc::new(linux::LinuxPaster::new()),
        }
    }
    #[cfg(target_os = "windows")]
    {
        PlatformAdapters {
            catalog: Arc::new(windows::WindowsAppCatalog::new()),
            launcher: Arc::new(windows::WindowsLauncher::new()),
            focus: Arc::new(windows::WindowsFocusTracker::new()),
            hotkeys: Arc::new(windows::WindowsHotkeyManager::new()),
            capabilities: Arc::new(windows::WindowsCapabilityProbe::new()),
            clipboard: Arc::new(windows::WindowsClipboard::new()),
            clipboard_watcher: Arc::new(windows::WindowsClipboardWatcher::new()),
            chrome: Arc::new(windows::WindowsChromeProvider::new()),
            paster: Arc::new(windows::WindowsPaster::new()),
        }
    }
    #[cfg(target_os = "macos")]
    {
        PlatformAdapters {
            catalog: Arc::new(macos::MacosAppCatalog::new()),
            launcher: Arc::new(macos::MacosLauncher::new()),
            focus: Arc::new(macos::MacosFocusTracker::new()),
            hotkeys: Arc::new(macos::MacosHotkeyManager::new()),
            capabilities: Arc::new(macos::MacosCapabilityProbe::new()),
            clipboard: Arc::new(macos::MacosClipboard::new()),
            clipboard_watcher: Arc::new(macos::MacosClipboardWatcher::new()),
            chrome: Arc::new(macos::MacosChromeProvider::new()),
            paster: Arc::new(macos::MacosPaster::new()),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        PlatformAdapters {
            catalog: Arc::new(unsupported::UnsupportedCatalog),
            launcher: Arc::new(unsupported::UnsupportedLauncher),
            focus: Arc::new(unsupported::UnsupportedFocusTracker),
            hotkeys: Arc::new(unsupported::UnsupportedHotkeyManager),
            capabilities: Arc::new(unsupported::UnsupportedCapabilityProbe),
            clipboard: Arc::new(unsupported::UnsupportedClipboard),
            clipboard_watcher: Arc::new(unsupported::UnsupportedClipboardWatcher),
            chrome: Arc::new(unsupported::UnsupportedChrome),
            paster: Arc::new(unsupported::UnsupportedPaster),
        }
    }
}
