//! # flashcast-platform
//!
//! Flashcast 的平台适配层。按能力拆分的 trait 定义在这里，各平台实现按
//! `#[cfg(target_os)]` 分模块，测试替身只在 [`fake`]（feature `fake`）中。
//!
//! v0.1.0 的 Linux（ticket 01）与 Windows（ticket 02）实现分别在 [`linux`] 与
//! [`windows`]；macOS 由后续 ticket 提供，在此之前由 [`unsupported`] 中的桩实现
//! 报告「不支持」，保证宿主能在三个平台上编译并如实报告能力状态。

pub mod capability;
pub mod catalog;
pub mod focus;
pub mod freedesktop;
pub mod hotkey;
pub mod launch;
pub mod launch_request;
pub mod shortcut;
pub mod unsupported;
pub mod windows;

#[cfg(feature = "fake")]
pub mod fake;

#[cfg(target_os = "linux")]
pub mod linux;

/// `global-hotkey` 后端的共享实现，由 Linux 与 Windows 的快捷键适配器使用。
#[cfg(any(target_os = "linux", target_os = "windows"))]
mod hotkey_backend;

use std::sync::Arc;

pub use capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};
pub use catalog::{AppCatalog, AppEntry, AppSource, CatalogError, IconRef};
pub use focus::{FocusError, FocusTracker, FocusedApp};
pub use hotkey::{HotkeySpec, HotkeySpecError, Key, Modifier, DEFAULT_HOTKEY};
pub use launch::{AppLauncher, LaunchError, LaunchReceipt};
pub use launch_request::LaunchRequest;
pub use shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

/// 宿主注入使用的适配器集合。
#[derive(Clone)]
pub struct PlatformAdapters {
    pub catalog: Arc<dyn AppCatalog>,
    pub launcher: Arc<dyn AppLauncher>,
    pub focus: Arc<dyn FocusTracker>,
    pub hotkeys: Arc<dyn HotkeyManager>,
    pub capabilities: Arc<dyn CapabilityProbe>,
}

/// 当前平台的适配器集合。
///
/// Linux 与 Windows 使用各自的真实实现；尚未实现的平台（macOS）使用
/// [`unsupported`] 的桩实现，如实报告「不支持」而不是伪造成功。
pub fn current() -> PlatformAdapters {
    #[cfg(target_os = "linux")]
    {
        PlatformAdapters {
            catalog: Arc::new(linux::LinuxAppCatalog::new()),
            launcher: Arc::new(linux::LinuxLauncher::new()),
            focus: Arc::new(linux::LinuxFocusTracker::new()),
            hotkeys: Arc::new(linux::LinuxHotkeyManager::new()),
            capabilities: Arc::new(linux::LinuxCapabilityProbe::new()),
        }
    }
    #[cfg(target_os = "windows")]
    {
        PlatformAdapters {
            catalog: Arc::new(unsupported::UnsupportedCatalog),
            launcher: Arc::new(unsupported::UnsupportedLauncher),
            focus: Arc::new(unsupported::UnsupportedFocusTracker),
            hotkeys: Arc::new(windows::hotkeys::WindowsHotkeyManager::new()),
            capabilities: Arc::new(unsupported::UnsupportedCapabilityProbe),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        PlatformAdapters {
            catalog: Arc::new(unsupported::UnsupportedCatalog),
            launcher: Arc::new(unsupported::UnsupportedLauncher),
            focus: Arc::new(unsupported::UnsupportedFocusTracker),
            hotkeys: Arc::new(unsupported::UnsupportedHotkeyManager),
            capabilities: Arc::new(unsupported::UnsupportedCapabilityProbe),
        }
    }
}
