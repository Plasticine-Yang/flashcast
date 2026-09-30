//! Windows 全局快捷键：基于 `global-hotkey`（`RegisterHotKey`）。
//!
//! 与 Linux 不同，Windows 上不存在「注册成功但收不到按键」的假象：`RegisterHotKey`
//! 要么成功，要么因为组合已被占用（或没有可用桌面）而失败。因此这里只在**没有可
//! 交互桌面**时提前拒绝，其余情况一律真实尝试注册，并把失败分类成用户能理解的原因
//! （「已被其他应用占用，请在设置中改用其他组合」）。
//!
//! 注册、注销、更新与事件分发在 [`crate::hotkey_backend`] 中与 Linux 共用。

use crate::hotkey::HotkeySpec;
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

pub struct WindowsHotkeyManager;

impl Default for WindowsHotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsHotkeyManager {
    pub fn new() -> Self {
        Self
    }

    /// 当前会话下是否允许尝试注册。
    pub fn backend_allowed(&self) -> Result<(), HotkeyError> {
        if super::session::interactive_desktop_available() {
            Ok(())
        } else {
            Err(HotkeyError::BackendUnavailable {
                reason: "当前没有可交互的桌面会话（没有前台窗口），无法注册全局快捷键；\
                         请使用托盘入口打开 Flashcast"
                    .to_string(),
            })
        }
    }
}

impl HotkeyManager for WindowsHotkeyManager {
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        self.backend_allowed()?;
        crate::hotkey_backend::register(spec, on_press)
    }

    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        // 更新不重复做准入判断：能注册成功就说明后端可用。
        crate::hotkey_backend::update(handle, spec)
    }

    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        crate::hotkey_backend::unregister(handle)
    }
}
