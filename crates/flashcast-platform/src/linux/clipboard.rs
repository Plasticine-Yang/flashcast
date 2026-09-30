//! Linux 剪贴板文本写入。
//!
//! 与能力探测（[`super::cap`]）保持同一套判断：Wayland 会话用 `wl-copy`，X11 会话用
//! `xclip` 或 `xsel`。工具缺失时如实返回「未找到」，不伪造成功。

use std::path::PathBuf;

use crate::capability::SessionType;
use crate::clipboard::{
    check_text, find_program, write_with_tool, ClipboardAccess, ClipboardError,
};

use super::force_x11_backend;

/// Linux 的文本剪贴板后端。
pub struct LinuxClipboard {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxClipboard {
    pub fn new() -> Self {
        Self {
            session: super::detect_session_type(),
            force_x11: force_x11_backend(),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self { session, force_x11 }
    }

    /// 当前会使用的后端：名字、可执行文件与参数。
    ///
    /// `FLASHCAST_FORCE_X11_BACKEND=1` 时在 Wayland 会话下也优先试 X11 工具
    /// （XWayland 场景的诊断开关，与焦点适配层一致）。
    fn backend(&self) -> Result<(&'static str, PathBuf, Vec<&'static str>), ClipboardError> {
        let x11 = || {
            find_program("xclip")
                .map(|path| ("xclip", path, vec!["-selection", "clipboard", "-in"]))
                .or_else(|| find_program("xsel").map(|path| ("xsel", path, vec!["-i", "-b"])))
        };
        let wayland = || find_program("wl-copy").map(|path| ("wl-copy", path, Vec::new()));

        let preferred = if self.session == SessionType::Wayland && !self.force_x11 {
            wayland()
        } else {
            x11()
        };
        // 首选缺失时回退到另一族工具：XWayland 会话下 xclip 通常仍然可用，
        // 反过来 X11 会话里也可能只有 wl-clipboard（例如通过 X 转发运行）。
        let backend = preferred.or_else(|| {
            if self.session == SessionType::Wayland {
                x11()
            } else {
                wayland()
            }
        });
        backend.ok_or_else(|| ClipboardError::ToolMissing {
            reason: format!(
                "{} 会话需要 wl-copy（Wayland）或 xclip / xsel（X11），当前都没有找到",
                self.session.label_zh()
            ),
        })
    }

    /// 当前会使用的后端名字；不可用时给出原因。诊断与真实平台检查用。
    pub fn backend_name(&self) -> Result<&'static str, ClipboardError> {
        self.backend().map(|(name, _, _)| name)
    }
}

impl ClipboardAccess for LinuxClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        check_text(text)?;
        let (_, program, args) = self.backend()?;
        write_with_tool(&program, &args, text)
    }
}
