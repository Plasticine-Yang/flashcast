//! Linux 全局快捷键：X11 使用 `global-hotkey`，Wayland 使用 XDG 门户。
//!
//! XWayland 抓取即便返回成功也不会收到原生 Wayland 按键事件，
//! 因此 Wayland 默认走门户，只有诊断开关才强制使用 X11 后端。

use crate::capability::SessionType;
use crate::hotkey::HotkeySpec;
use crate::hotkey_backend;
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

use super::{detect_session_type, force_x11_backend};

pub struct LinuxHotkeyManager {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxHotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxHotkeyManager {
    pub fn new() -> Self {
        Self {
            session: detect_session_type(),
            force_x11: force_x11_backend(),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self { session, force_x11 }
    }

    /// 当前会话下是否允许尝试注册。
    pub fn backend_allowed(&self) -> Result<(), HotkeyError> {
        match self.session {
            SessionType::X11 => Ok(()),
            SessionType::Wayland if self.force_x11 => Ok(()),
            SessionType::Wayland => super::portal_hotkeys::available(),
            SessionType::Headless => Err(HotkeyError::BackendUnavailable {
                reason: "当前没有桌面会话，无法注册全局快捷键".to_string(),
            }),
            SessionType::Unknown | SessionType::NotApplicable => {
                Err(HotkeyError::BackendUnavailable {
                    reason: "无法确定会话类型，未尝试注册全局快捷键".to_string(),
                })
            }
        }
    }
}

impl HotkeyManager for LinuxHotkeyManager {
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        if self.session == SessionType::Wayland && !self.force_x11 {
            return super::portal_hotkeys::register(spec, on_press);
        }
        self.backend_allowed()?;
        hotkey_backend::register(spec, on_press)
    }

    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        if self.session == SessionType::Wayland && !self.force_x11 {
            return super::portal_hotkeys::update(handle, spec);
        }
        hotkey_backend::update(handle, spec)
    }

    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        if self.session == SessionType::Wayland && !self.force_x11 {
            return super::portal_hotkeys::unregister(handle);
        }
        hotkey_backend::unregister(handle)
    }
}
