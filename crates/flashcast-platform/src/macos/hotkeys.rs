//! macOS 全局快捷键：基于 `global-hotkey` 的 Carbon `RegisterEventHotKey` 后端。
//!
//! 后端把事件处理器装在应用的 Carbon 事件目标上，因此：
//! - 注册必须发生在 Cocoa 应用事件循环可用之后（宿主的 `setup` 阶段正好满足）；
//! - 普通组合键**不需要**辅助功能权限；只有媒体键（`global-hotkey` 用
//!   `CGEventTap` 监听 `SystemDefined` 事件）才需要，本产品不使用媒体键；
//! - 组合键已被其他应用占用时 `RegisterEventHotKey` 返回 `eventHotKeyExistsErr`，
//!   被分类为 [`HotkeyError::Conflict`] 交给 UI 展示，而不是静默失败。
//!
//! 按键映射、回调分发与错误分类在 [`crate::hotkey_backend`] 中与 Linux 共享；
//! 本模块只判断「当前是否有可交互的桌面会话」。

#[cfg(target_os = "macos")]
use crate::hotkey_backend;

#[cfg(target_os = "macos")]
use super::cap::desktop_available;
#[cfg(target_os = "macos")]
use crate::hotkey::HotkeySpec;
#[cfg(target_os = "macos")]
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

#[cfg(target_os = "macos")]
pub struct MacosHotkeyManager {
    desktop_available: bool,
}

#[cfg(target_os = "macos")]
impl Default for MacosHotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosHotkeyManager {
    pub fn new() -> Self {
        Self {
            desktop_available: desktop_available(),
        }
    }

    /// 使用显式环境事实构造，供诊断与测试使用。
    pub fn with_desktop_available(desktop_available: bool) -> Self {
        Self { desktop_available }
    }

    /// 当前环境是否允许尝试注册。
    pub fn backend_allowed(&self) -> Result<(), HotkeyError> {
        if self.desktop_available {
            Ok(())
        } else {
            Err(HotkeyError::BackendUnavailable {
                reason: "当前没有可交互的桌面会话（无 Aqua 会话），无法注册全局快捷键；\
                         请使用托盘入口"
                    .to_string(),
            })
        }
    }
}

#[cfg(target_os = "macos")]
impl HotkeyManager for MacosHotkeyManager {
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        self.backend_allowed()?;
        hotkey_backend::register(spec, on_press)
    }

    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        hotkey_backend::update(handle, spec)
    }

    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        hotkey_backend::unregister(handle)
    }
}
