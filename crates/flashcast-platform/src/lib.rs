//! # flashcast-platform
//!
//! Flashcast 的平台适配层。按能力拆分的 trait 定义在这里，各平台实现按
//! `#[cfg(target_os)]` 分模块，测试替身只在 [`fake`]（feature `fake`）中。
//!
//! v0.1.0 ticket 01 实现 Linux，ticket 03 实现 macOS；Windows 由后续 ticket 提供，
//! 在此之前由 [`unsupported`] 中的桩实现报告「不支持」，保证宿主能在三个
//! 平台上编译并如实报告能力状态。

pub mod capability;
pub mod catalog;
pub mod focus;
pub mod freedesktop;
pub mod hotkey;
pub mod launch;
pub mod launch_request;
pub mod macos;
pub mod shortcut;
pub mod unsupported;

#[cfg(feature = "fake")]
pub mod fake;

#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod hotkey_backend;

#[cfg(target_os = "linux")]
pub mod linux;

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
/// Linux 与 macOS 使用真实实现；Windows 等其余平台在对应 ticket 落地前使用
/// 「不支持」桩实现，如实报告能力状态而不是伪造成功。
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
    #[cfg(target_os = "macos")]
    {
        PlatformAdapters {
            catalog: Arc::new(macos::MacosAppCatalog::new()),
            launcher: Arc::new(macos::MacosLauncher::new()),
            focus: Arc::new(macos::MacosFocusTracker::new()),
            hotkeys: Arc::new(macos::MacosHotkeyManager::new()),
            capabilities: Arc::new(macos::MacosCapabilityProbe::new()),
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
