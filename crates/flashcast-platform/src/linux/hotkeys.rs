//! Linux 全局快捷键：基于 `global-hotkey`（X11 `XGrabKey`）。
//!
//! 该后端**只支持 X11**。Wayland 会话下抓取即便返回成功也不会收到按键事件，
//! 因此默认拒绝注册并返回 [`HotkeyError::BackendUnavailable`]，让 UI 明确提示
//! 用户改用托盘入口，而不是给出一个「看起来注册成功」的假象。
//!
//! 进程级回调表、按键映射与错误分类在 [`crate::hotkey_backend`] 中与 macOS 共享；
//! 本模块只负责「当前会话是否允许注册」这一 Linux 专有判断。

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
            SessionType::Wayland => Err(HotkeyError::BackendUnavailable {
                reason: "Wayland 会话不支持全局快捷键抓取；请使用托盘入口，或在 X11 会话下运行"
                    .to_string(),
            }),
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
