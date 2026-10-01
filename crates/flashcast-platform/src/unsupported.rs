//! 尚未实现平台的桩实现。
//!
//! Linux（ticket 01）、Windows（ticket 02）与 macOS（ticket 03）都已有真实实现，
//! 因此本模块只在**其余**目标上编译。这些桩必须让宿主在所有目标上都能编译，
//! 并**如实**报告「不支持」，而不是伪造成功。

#![cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]

use crate::capability::{Capabilities, CapabilityProbe, Support};
use crate::catalog::{AppCatalog, AppEntry, CatalogError};
use crate::clipboard::{ClipboardAccess, ClipboardError};
use crate::focus::{FocusError, FocusTracker, FocusedApp};
use crate::hotkey::HotkeySpec;
use crate::launch::{AppLauncher, LaunchError, LaunchReceipt};
use crate::launch_request::LaunchRequest;
use crate::paste::{PasteError, Paster};
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

const REASON: &str = "当前平台的实现尚未提供（由后续平台 ticket 完成）";

pub struct UnsupportedPaster;

impl Paster for UnsupportedPaster {
    fn paste(&self) -> Result<(), PasteError> {
        Err(PasteError::Unsupported {
            reason: REASON.to_string(),
        })
    }
}

pub struct UnsupportedClipboard;

impl ClipboardAccess for UnsupportedClipboard {
    fn write_text(&self, _text: &str) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported {
            reason: REASON.to_string(),
        })
    }
}

pub struct UnsupportedCatalog;

impl AppCatalog for UnsupportedCatalog {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError> {
        Err(CatalogError::Unsupported)
    }
}

pub struct UnsupportedLauncher;

impl AppLauncher for UnsupportedLauncher {
    fn launch(&self, _request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError> {
        Err(LaunchError::Unsupported)
    }
}

pub struct UnsupportedFocusTracker;

impl FocusTracker for UnsupportedFocusTracker {
    fn capture(&self) -> Result<FocusedApp, FocusError> {
        Err(FocusError::Unsupported {
            reason: REASON.to_string(),
        })
    }

    fn restore(&self, _app: &FocusedApp) -> Result<(), FocusError> {
        Err(FocusError::Unsupported {
            reason: REASON.to_string(),
        })
    }
}

pub struct UnsupportedHotkeyManager;

impl HotkeyManager for UnsupportedHotkeyManager {
    fn register(
        &self,
        _spec: &HotkeySpec,
        _on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        Err(HotkeyError::BackendUnavailable {
            reason: REASON.to_string(),
        })
    }

    fn update(
        &self,
        _handle: &HotkeyHandle,
        _spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        Err(HotkeyError::BackendUnavailable {
            reason: REASON.to_string(),
        })
    }

    fn unregister(&self, _handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        Err(HotkeyError::BackendUnavailable {
            reason: REASON.to_string(),
        })
    }
}

pub struct UnsupportedCapabilityProbe;

impl CapabilityProbe for UnsupportedCapabilityProbe {
    fn probe(&self) -> Capabilities {
        Capabilities {
            os: crate::capability::target_os(),
            os_version: None,
            arch: crate::capability::target_arch(),
            session: crate::capability::SessionType::NotApplicable,
            desktop_available: false,
            hotkey: Support::Unknown {
                reason: REASON.to_string(),
            },
            clipboard: Support::Unknown {
                reason: REASON.to_string(),
            },
            auto_paste: Support::Unknown {
                reason: REASON.to_string(),
            },
            notes: vec![REASON.to_string()],
        }
    }
}
