//! Linux 合成粘贴。
//!
//! - X11 会话：用 `enigo` 的 `x11rb` 后端（XTEST）注入 `Ctrl+V`；
//! - Wayland 会话：**没有**可用手段。GNOME 不允许应用把焦点交给别的应用后再注入
//!   按键，`zwp_virtual_keyboard_manager_v1` 是 wlroots 专有协议（Mutter 未实现），
//!   只有 `org.freedesktop.portal.RemoteDesktop` + EIS 经用户授权后才行（研究 §3.4）。
//!   本版本不申请该权限，因此如实返回 [`PasteError::Unsupported`]。
//!
//! 与焦点适配层一致：`FLASHCAST_FORCE_X11_BACKEND=1` 时在 Wayland 会话下也尝试
//! X11 后端，**仅用于诊断**（XWayland 里的 X11 客户端能收到，原生 Wayland 客户端
//! 收不到，因此这不能算作 Wayland 支持）。

use enigo::{Enigo, Key, Settings};

use crate::capability::SessionType;
use crate::paste::{PasteError, Paster};

use super::{detect_session_type, force_x11_backend, WAYLAND_PASTE_REASON};

/// Linux 的合成粘贴后端。
pub struct LinuxPaster {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxPaster {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxPaster {
    pub fn new() -> Self {
        Self {
            session: detect_session_type(),
            force_x11: force_x11_backend(),
        }
    }

    /// 使用显式会话类型构造，供诊断与测试使用。
    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self { session, force_x11 }
    }

    pub fn session(&self) -> SessionType {
        self.session
    }

    /// 当前会话是否允许尝试 X11 注入。
    pub fn uses_x11(&self) -> bool {
        match self.session {
            SessionType::X11 => true,
            SessionType::Wayland => self.force_x11,
            SessionType::Unknown => self.force_x11,
            SessionType::Headless | SessionType::NotApplicable => false,
        }
    }
}

impl Paster for LinuxPaster {
    fn paste(&self) -> Result<(), PasteError> {
        if !self.uses_x11() {
            let reason = match self.session {
                SessionType::Headless => "当前没有桌面会话，无法注入粘贴".to_string(),
                _ => WAYLAND_PASTE_REASON.to_string(),
            };
            return Err(PasteError::Unsupported { reason });
        }
        let mut enigo = Enigo::new(&Settings::default()).map_err(|error| PasteError::Failed {
            reason: format!("无法连接 X11 合成输入后端（XTEST）：{error}"),
        })?;
        crate::paste::send_paste_chord(&mut enigo, Key::Control)
    }
}
