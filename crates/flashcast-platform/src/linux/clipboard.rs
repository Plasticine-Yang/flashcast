//! Linux 剪贴板读写与变化监听。
//!
//! 与能力探测（[`super::cap`]）保持同一套判断：Wayland 会话用 `wl-copy` / `wl-paste`，
//! X11 会话用 `xclip` 或 `xsel`。工具缺失时如实返回「未找到」，不伪造成功。
//!
//! ## 变化监听为什么是轮询
//!
//! `arboard` 等库不提供变化事件（research `app-discovery-and-focus.md`）。这里按固定
//! 间隔读一次剪贴板，用**内容指纹**判断是否变化：外部工具没有可读的变更序号。
//! 代价与取舍写在 [`crate::clipboard`] 的模块文档里。
//!
//! Wayland 下选区由持有者进程提供，数据在进程退出后消失；`wl-paste` 在拿不到选区时
//! 会一直等待，因此读取同样走有界等待（[`crate::clipboard::READ_TIMEOUT`]）。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::capability::SessionType;
use crate::clipboard::{
    check_text, find_program, fingerprint, read_with_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardError, ClipboardPoll, ClipboardSourceApp, ClipboardWatcher,
};

use super::{force_x11_backend, x11};

/// 打开一次剪贴板后端所需的全部信息：名字、可执行文件与参数。
type Backend = (&'static str, PathBuf, Vec<&'static str>);

/// 按会话选择首选工具，首选缺失时回退到另一族（XWayland 场景下两者都可能可用）。
fn pick_backend(
    session: SessionType,
    force_x11: bool,
    candidates: [&dyn Fn() -> Option<Backend>; 2],
) -> Result<Backend, ClipboardError> {
    let (first, second) = if session == SessionType::Wayland && !force_x11 {
        (candidates[0], candidates[1])
    } else {
        (candidates[1], candidates[0])
    };
    first()
        .or_else(second)
        .ok_or_else(|| ClipboardError::ToolMissing {
            reason: format!(
                "{} 会话需要 wl-copy / wl-paste（Wayland）或 xclip / xsel（X11），当前都没有找到",
                session.label_zh()
            ),
        })
}

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

    /// 写入时使用的后端。
    ///
    /// `FLASHCAST_FORCE_X11_BACKEND=1` 时在 Wayland 会话下也优先试 X11 工具
    /// （XWayland 场景的诊断开关，与焦点适配层一致）。
    fn write_backend(&self) -> Result<Backend, ClipboardError> {
        let x11 = || {
            find_program("xclip")
                .map(|path| ("xclip", path, vec!["-selection", "clipboard", "-in"]))
                .or_else(|| find_program("xsel").map(|path| ("xsel", path, vec!["-i", "-b"])))
        };
        let wayland = || find_program("wl-copy").map(|path| ("wl-copy", path, Vec::new()));
        pick_backend(self.session, self.force_x11, [&wayland, &x11])
    }

    /// 读取时使用的后端。
    fn read_backend(&self) -> Result<Backend, ClipboardError> {
        let x11 = || {
            find_program("xclip")
                .map(|path| ("xclip", path, vec!["-selection", "clipboard", "-o"]))
                .or_else(|| find_program("xsel").map(|path| ("xsel", path, vec!["-b", "-o"])))
        };
        let wayland =
            || find_program("wl-paste").map(|path| ("wl-paste", path, vec!["--no-newline"]));
        pick_backend(self.session, self.force_x11, [&wayland, &x11])
    }

    /// 当前会使用的写入后端名字；不可用时给出原因。诊断与真实平台检查用。
    pub fn backend_name(&self) -> Result<&'static str, ClipboardError> {
        self.write_backend().map(|(name, _, _)| name)
    }

    /// 当前会使用的读取后端名字；不可用时给出原因。
    pub fn read_backend_name(&self) -> Result<&'static str, ClipboardError> {
        self.read_backend().map(|(name, _, _)| name)
    }
}

impl ClipboardAccess for LinuxClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        check_text(text)?;
        let (_, program, args) = self.write_backend()?;
        write_with_tool(&program, &args, text)
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let (_, program, args) = self.read_backend()?;
        read_with_tool(&program, &args)
    }
}

/// Linux 的剪贴板变化监听。
///
/// 用内容指纹判断变化。自身写入抑制在适配层记一份指纹，宿主另有一层兜底
/// （见 [`crate::clipboard`] 的模块文档）。
pub struct LinuxClipboardWatcher {
    session: SessionType,
    force_x11: bool,
    /// 上一次报告过的内容指纹。
    last: Mutex<Option<u64>>,
    /// 由 Flashcast 自己写入、尚未被 `poll` 消费掉的内容指纹。
    own: Mutex<Vec<u64>>,
}

impl Default for LinuxClipboardWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxClipboardWatcher {
    pub fn new() -> Self {
        Self {
            session: super::detect_session_type(),
            force_x11: force_x11_backend(),
            last: Mutex::new(None),
            own: Mutex::new(Vec::new()),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self {
            session,
            force_x11,
            last: Mutex::new(None),
            own: Mutex::new(Vec::new()),
        }
    }

    /// 读取当前剪贴板文本（与 [`LinuxClipboard::read_text`] 同一套后端）。
    fn read(&self) -> Result<Option<String>, ClipboardError> {
        LinuxClipboard::with_session(self.session, self.force_x11).read_text()
    }

    /// 这次读取是否走 X11 一族（来源应用只能在 X11 上查到）。
    fn uses_x11(&self) -> bool {
        self.session != SessionType::Wayland || self.force_x11
    }

    /// 来源应用：X11 上查选区持有者，Wayland 上拿不到（如实留空）。
    fn source(&self) -> Option<ClipboardSourceApp> {
        if !self.uses_x11() {
            return None;
        }
        let owner = x11::clipboard_owner()?;
        let app_id = owner.identifier()?;
        Some(ClipboardSourceApp {
            title: owner.title.clone().or_else(|| Some(app_id.clone())),
            app_id,
        })
    }
}

impl ClipboardWatcher for LinuxClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        let Some(text) = self.read()? else {
            // 剪贴板为空或只含非文本格式：v0.1.0 没有可捕获的内容。
            return Ok(ClipboardPoll::Unchanged);
        };
        let print = fingerprint(&text);
        {
            let mut own = self.own.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(index) = own.iter().position(|item| *item == print) {
                own.remove(index);
                // 自身写入：记账后不再报告成新的复制事件（也避免下次重复判定）。
                *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(print);
                return Ok(ClipboardPoll::Unchanged);
            }
        }
        {
            let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
            if *last == Some(print) {
                return Ok(ClipboardPoll::Unchanged);
            }
            *last = Some(print);
        }
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats: vec![crate::clipboard::ClipboardFormatKind::Text],
            text: Some(text),
            source: self.source(),
        }))
    }

    fn note_own_write(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.own
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fingerprint(text));
    }
}
