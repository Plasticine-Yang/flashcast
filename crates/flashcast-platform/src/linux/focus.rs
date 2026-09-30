//! Linux 焦点追踪。
//!
//! - X11 会话：通过 EWMH 读取当前前台窗口并恢复其焦点；
//! - Wayland 会话：普通应用拿不到全局焦点，如实返回
//!   [`FocusError::Unsupported`]，不伪造结果。

use crate::capability::SessionType;
use crate::focus::{FocusError, FocusTracker, FocusedApp};

use super::x11::{self, X11ActiveWindow};
use super::{detect_session_type, force_x11_backend, WAYLAND_FOCUS_REASON};

pub struct LinuxFocusTracker {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxFocusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxFocusTracker {
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

    /// 当前是否应当使用 X11 后端。Wayland 会话默认返回 false，
    /// 只有显式设置 FLASHCAST_FORCE_X11_BACKEND 时才尝试（用于诊断）。
    pub fn uses_x11(&self) -> bool {
        match self.session {
            SessionType::X11 => true,
            SessionType::Wayland => self.force_x11,
            SessionType::Unknown => self.force_x11,
            SessionType::Headless | SessionType::NotApplicable => false,
        }
    }

    fn unsupported_reason(&self) -> String {
        match self.session {
            SessionType::Wayland => WAYLAND_FOCUS_REASON.to_string(),
            SessionType::Headless => "当前没有桌面会话，无法读取焦点窗口".to_string(),
            _ => WAYLAND_FOCUS_REASON.to_string(),
        }
    }
}

impl FocusTracker for LinuxFocusTracker {
    fn capture(&self) -> Result<FocusedApp, FocusError> {
        if self.session == SessionType::Wayland && !self.force_x11 {
            return Err(FocusError::Unsupported {
                reason: self.unsupported_reason(),
            });
        }
        if self.session == SessionType::Headless {
            return Err(FocusError::Unsupported {
                reason: self.unsupported_reason(),
            });
        }
        let active = x11::active_window().ok_or(FocusError::NoActiveWindow)?;
        Ok(to_focused_app(active))
    }

    fn restore(&self, app: &FocusedApp) -> Result<(), FocusError> {
        if self.session == SessionType::Wayland && !self.force_x11 {
            return Err(FocusError::Unsupported {
                reason: self.unsupported_reason(),
            });
        }
        if self.session == SessionType::Headless {
            return Err(FocusError::Unsupported {
                reason: self.unsupported_reason(),
            });
        }
        let window = app.window.ok_or_else(|| FocusError::Unavailable {
            reason: "没有可用的 X11 窗口句柄，无法恢复焦点".to_string(),
        })?;
        x11::activate_window(window).map_err(|reason| FocusError::Unavailable { reason })
    }
}

fn to_focused_app(active: X11ActiveWindow) -> FocusedApp {
    let process_name = active
        .pid
        .and_then(process_name_from_pid)
        .filter(|name| !name.is_empty());
    let id = active
        .identifier()
        .or_else(|| process_name.clone())
        .unwrap_or_else(|| "unknown".to_string());
    // 优先展示可读标题，其次进程名，最后回退到标识。
    let name = active
        .title
        .clone()
        .or_else(|| process_name.clone())
        .unwrap_or_else(|| id.clone());
    FocusedApp {
        id,
        name,
        wm_class: active.class.clone().or(active.instance.clone()),
        pid: active.pid,
        window: Some(active.window),
    }
}

/// 从 `/proc/<pid>/comm` 读取进程名。仅在 Linux 上可用。
fn process_name_from_pid(pid: u32) -> Option<String> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    let name = raw.trim().to_string();
    (!name.is_empty()).then_some(name)
}
