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
    check_image_write, check_text, find_program, fingerprint, fingerprint_bytes, image_from_bytes,
    read_bytes_bounded, read_with_tool, write_bytes_with_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardError, ClipboardImage, ClipboardPoll, ClipboardSourceApp,
    ClipboardWatcher, IMAGE_MIME_PNG, MAX_IMAGE_BYTES,
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

    /// 写入图片时使用的后端。
    ///
    /// 图片必须按 MIME 目标写入（`wl-copy -t image/png` / `xclip -t image/png`），
    /// `xsel` 没有按目标选择格式的能力，因此图片路径上**不**回退到 `xsel`：
    /// 拿不到就如实报「工具缺失」，而不是写出一段被当成文本的二进制。
    fn image_write_backend(&self) -> Result<Backend, ClipboardError> {
        let x11 = || {
            find_program("xclip").map(|path| {
                (
                    "xclip",
                    path,
                    vec!["-selection", "clipboard", "-t", IMAGE_MIME_PNG, "-in"],
                )
            })
        };
        let wayland =
            || find_program("wl-copy").map(|path| ("wl-copy", path, vec!["-t", IMAGE_MIME_PNG]));
        pick_backend(self.session, self.force_x11, [&wayland, &x11]).map_err(|error| match error {
            ClipboardError::ToolMissing { .. } => ClipboardError::ToolMissing {
                reason: format!(
                    "{} 会话需要 wl-copy（Wayland）或 xclip（X11）才能写入图片；xsel 不支持按图片格式写入",
                    self.session.label_zh()
                ),
            },
            other => other,
        })
    }

    /// 读取图片时使用的后端（同样只走支持 MIME 目标的工具）。
    fn image_read_backend(&self) -> Result<Backend, ClipboardError> {
        let x11 = || {
            find_program("xclip").map(|path| {
                (
                    "xclip",
                    path,
                    vec!["-selection", "clipboard", "-t", IMAGE_MIME_PNG, "-o"],
                )
            })
        };
        let wayland = || {
            find_program("wl-paste")
                .map(|path| ("wl-paste", path, vec!["--no-newline", "-t", IMAGE_MIME_PNG]))
        };
        pick_backend(self.session, self.force_x11, [&wayland, &x11])
    }

    /// 读取当前剪贴板里的图片，并把「有图片但无法保存」的原因一并带回。
    ///
    /// `None` 表示剪贴板里没有 PNG 图片（很常见：复制的是文字）；原因是给监听层用的，
    /// 它会如实上报「这次复制没有被保存」，而不是静默跳过。
    pub fn read_image_detailed(
        &self,
    ) -> Result<(Option<ClipboardImage>, Option<String>), ClipboardError> {
        let backend = match self.image_read_backend() {
            Ok(backend) => backend,
            // 没有支持图片目标的工具：如实表示「读不到图片」，不是错误。
            Err(ClipboardError::ToolMissing { .. }) => return Ok((None, None)),
            Err(error) => return Err(error),
        };
        let (_, program, args) = backend;
        let bytes = read_bytes_bounded(&program, &args, MAX_IMAGE_BYTES)?;
        Ok(image_from_bytes(IMAGE_MIME_PNG, bytes))
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

    /// 写入图片（ticket 10）。
    ///
    /// Wayland 的选区由持有者提供：`wl-copy` 会 fork 出守护进程持有选区，因此与文本
    /// 一样走有界等待；拿不到选区时如实报错，宿主据此降级为手动粘贴。
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        check_image_write(image)?;
        let (_, program, args) = self.image_write_backend()?;
        write_bytes_with_tool(&program, &args, &image.bytes)
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Ok(self.read_image_detailed()?.0)
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

    /// 读取当前剪贴板图片（ticket 10）。
    fn read_image(&self) -> Result<(Option<ClipboardImage>, Option<String>), ClipboardError> {
        LinuxClipboard::with_session(self.session, self.force_x11).read_image_detailed()
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
        // 文字与图片都读一次：一次复制事件可能同时带来两者（例如浏览器同时给出
        // 图片与图片地址），只读其中一种会漏掉另一半。
        let text = self.read()?.filter(|text| !text.is_empty());
        let (image, problem) = self.read_image()?;
        if text.is_none() && image.is_none() && problem.is_none() {
            // 剪贴板为空或只含本版本不捕获的格式。
            return Ok(ClipboardPoll::Unchanged);
        }
        let print = match (&text, &image) {
            (Some(text), _) => fingerprint(text),
            (None, Some(image)) => fingerprint_bytes(&image.bytes),
            // 只有「有图片但保存不了」的原因时，按原因本身判定变化，
            // 同一个失败不会被反复上报成新的复制事件。
            (None, None) => fingerprint(problem.as_deref().unwrap_or_default()),
        };
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
        let mut formats = Vec::new();
        if text.is_some() {
            formats.push(crate::clipboard::ClipboardFormatKind::Text);
        }
        if image.is_some() {
            formats.push(crate::clipboard::ClipboardFormatKind::Image);
        }
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats,
            text,
            image,
            image_problem: problem,
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
