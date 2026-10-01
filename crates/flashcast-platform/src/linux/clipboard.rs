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
//!
//! ## 富文本格式
//!
//! 读：`wl-paste --type text/html`、`xclip -selection clipboard -t text/html -o` 按 MIME
//! 类型取；没有提供该类型时工具以非 0 退出，被当成「这次事件里没有这个格式」。`xsel`
//! 不支持指定类型，因此只提供文本。
//!
//! 写：只能提供纯文本。`wl-copy` 的 `--type` 决定唯一一种提供类型（`wl-clipboard 2.2.1`
//! 手册），再调用一次会接管选区并让上一个格式消失；`xclip` 同样一次只服务一个 target。
//! 因此恢复时写文本并如实报告 HTML/RTF 未同时提供——富文本载荷仍在本机历史里。

use std::path::PathBuf;
use std::sync::Mutex;

use crate::capability::SessionType;
use crate::clipboard::{
    check_files, check_text, file_entries, find_program, fingerprint, fingerprint_files,
    format_uri_list, parse_uri_list, read_with_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardContent, ClipboardError, ClipboardFormatKind,
    ClipboardPoll, ClipboardSourceApp, ClipboardWatcher, ClipboardWriteReport,
};

use super::{force_x11_backend, x11};

/// 打开一次剪贴板后端所需的全部信息：名字、可执行文件与参数。
type Backend = (&'static str, PathBuf, Vec<&'static str>);

/// Linux 文件列表的公开格式。GNOME 的私有种格式只是回退（第一行 `copy`/`cut`）。
const URI_LIST: &str = "text/uri-list";
const GNOME_COPIED_FILES: &str = "x-special/gnome-copied-files";

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

    /// 写入**文件列表**的后端：只有 `wl-copy` 与 `xclip` 能指定 MIME 类型，
    /// `xsel` 没有等价参数，因此不能用来写文件列表（不能把 URI 当成普通文本写进去，
    /// 那会让目标应用收到一段文字而不是文件）。
    fn write_files_backend(&self) -> Result<Backend, ClipboardError> {
        let x11 = || {
            find_program("xclip").map(|path| {
                (
                    "xclip",
                    path,
                    vec!["-selection", "clipboard", "-t", URI_LIST, "-in"],
                )
            })
        };
        let wayland =
            || find_program("wl-copy").map(|path| ("wl-copy", path, vec!["--type", URI_LIST]));
        pick_backend(self.session, self.force_x11, [&wayland, &x11])
    }

    /// 读取文件列表的后端与类型，按「公开格式优先、GNOME 私有种格式兜底」排序。
    fn read_files_candidates(&self) -> Vec<(&'static str, PathBuf, Vec<&'static str>)> {
        let x11 = |mime: &'static str| {
            find_program("xclip").map(|path| {
                (
                    "xclip",
                    path,
                    vec!["-selection", "clipboard", "-t", mime, "-o"],
                )
            })
        };
        let wayland = |mime: &'static str| {
            find_program("wl-paste")
                .map(|path| ("wl-paste", path, vec!["--no-newline", "--type", mime]))
        };
        let (first, second): (
            fn(&'static str) -> Option<Backend>,
            fn(&'static str) -> Option<Backend>,
        ) = if self.session == SessionType::Wayland && !self.force_x11 {
            (wayland, x11)
        } else {
            (x11, wayland)
        };
        let mut candidates = Vec::new();
        for mime in [URI_LIST, GNOME_COPIED_FILES] {
            if let Some(found) = first(mime) {
                candidates.push(found);
            }
            if let Some(found) = second(mime) {
                candidates.push(found);
            }
        }
        candidates
    }

    /// 读取当前剪贴板里的文件列表；没有文件列表时 `Ok(None)`。
    ///
    /// 每种类型都**有界**读取（与文本一致）；工具缺失时返回 `Ok(None)` 而不是报错——
    /// 「当前会话没有可用的剪贴板工具」与「本次没有文件列表」在 watcher 里都会退化为
    /// 「没有变化」，而写入侧的失败会如实报告。
    fn read_files_optional(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        for (_, program, args) in self.read_files_candidates() {
            match read_with_tool(&program, &args) {
                Ok(Some(text)) => {
                    let paths = parse_uri_list(&text);
                    if !paths.is_empty() {
                        return Ok(Some(paths));
                    }
                }
                // 工具以非 0 退出＝当前剪贴板没有这种类型；继续试下一个类型。
                Ok(None) => {}
                // 读取失败（例如工具卡住被中止）：如实上报，不能当成「没有文件」。
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    /// 按 MIME 类型读剪贴板里的一种格式；没有这种格式时返回 `Ok(None)`。
    ///
    /// `wl-paste` 用 `--type`、`xclip` 用 `-t`；两者在「没有提供该类型」时都以非 0 退出，
    /// 因此 [`read_with_tool`] 会把它当成「没有这种格式」，而不是失败。`xsel` 不支持指定
    /// 类型，此时如实返回 `None`（不猜一个格式出来）。
    ///
    /// 读取同样**有界**（[`crate::clipboard::READ_TIMEOUT`]），不会让后台轮询线程卡住。
    ///
    /// 公开（`pub`）是给 `flashcast-platform-check` 用的：真实平台检查就靠它报告
    /// 「按 MIME 类型读取」这条路径在当前选区上是否真的可用。
    pub fn read_typed(&self, mime: &str) -> Result<Option<String>, ClipboardError> {
        let (name, program, args) = self.read_backend()?;
        match name {
            "wl-paste" => {
                let mut args = args;
                args.push("--type");
                args.push(mime);
                read_with_tool(&program, &args)
            }
            "xclip" => {
                // `xclip -selection clipboard -o` → `… -t <mime> -o`（顺序与手册一致）。
                let mut typed: Vec<&str> = Vec::with_capacity(args.len() + 2);
                for arg in args {
                    if arg == "-o" {
                        typed.push("-t");
                        typed.push(mime);
                    }
                    typed.push(arg);
                }
                read_with_tool(&program, &typed)
            }
            // xsel 只能读写文本，没有按类型取数据的接口。
            _ => Ok(None),
        }
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

    fn write_files(&self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        check_files(paths)?;
        let (_, program, args) = self.write_files_backend()?;
        let content = format_uri_list(paths);
        write_with_tool(&program, &args, &content)
    }

    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        self.read_files_optional()
    }

    /// 恢复剪贴板历史：Linux 上只能提供**纯文本**。
    ///
    /// `wl-copy` 的 `--type` 决定「提供内容的类型」（单数），再调用一次会接管选区并让
    /// 上一个格式消失；`xclip` 同样一次只服务一个 target。因此这里只写文本，并如实报告
    /// HTML/RTF 没有同时提供——而不是写一遍富文本、让纯文本目标什么都拿不到。
    /// 富文本载荷仍然完整地保存在本机历史里（见 `flashcast-core` 的 `clipboard_payloads`）。
    fn write_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, ClipboardError> {
        check_text(&content.text)?;
        let rich: Vec<ClipboardFormatKind> = content
            .requested_formats()
            .into_iter()
            .filter(|kind| *kind != ClipboardFormatKind::Text)
            .collect();
        if rich.is_empty() {
            let (_, program, args) = self.write_backend()?;
            write_with_tool(&program, &args, &content.text)?;
            return Ok(ClipboardWriteReport {
                formats: vec![ClipboardFormatKind::Text],
                skipped: Vec::new(),
            });
        }
        let (name, program, args) = self.write_backend()?;
        write_with_tool(&program, &args, &content.text)?;
        Ok(ClipboardWriteReport::text_only(
            format!(
                "{name} 一次只能提供一种 MIME 类型，同时提供会让纯文本目标拿不到内容；富文本载荷仍保存在本机历史里"
            )
            .as_str(),
            rich,
        ))
    }
}

/// Linux 的剪贴板变化监听。
///
/// 用内容指纹判断变化。自身写入抑制在适配层记一份指纹，宿主另有一层兜底
/// （见 [`crate::clipboard`] 的模块文档）。
pub struct LinuxClipboardWatcher {
    session: SessionType,
    force_x11: bool,
    /// 上一次报告过的**文本**内容指纹。
    last: Mutex<Option<u64>>,
    /// 上一次报告过的**文件列表**内容指纹。
    last_files: Mutex<Option<u64>>,
    /// 由 Flashcast 自己写入、尚未被 `poll` 消费掉的文本指纹。
    own: Mutex<Vec<u64>>,
    /// 由 Flashcast 自己写入、尚未被 `poll` 消费掉的文件列表指纹。
    own_files: Mutex<Vec<u64>>,
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
            last_files: Mutex::new(None),
            own: Mutex::new(Vec::new()),
            own_files: Mutex::new(Vec::new()),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self {
            session,
            force_x11,
            last: Mutex::new(None),
            last_files: Mutex::new(None),
            own: Mutex::new(Vec::new()),
            own_files: Mutex::new(Vec::new()),
        }
    }

    /// 读取当前剪贴板文本（与 [`LinuxClipboard::read_text`] 同一套后端）。
    fn read(&self) -> Result<Option<String>, ClipboardError> {
        LinuxClipboard::with_session(self.session, self.force_x11).read_text()
    }

    /// 读取当前剪贴板的文件列表（ticket 12）。
    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        LinuxClipboard::with_session(self.session, self.force_x11).read_files()
    }

    /// 读取一种富文本格式。**尽力而为**：拿不到这种格式（没提供、工具不支持、读取失败）
    /// 一律按「这次事件里没有这个格式」处理，绝不影响纯文本的捕获，也不凭空造一个载荷。
    fn read_rich(&self, mimes: &[&str]) -> Option<String> {
        let clipboard = LinuxClipboard::with_session(self.session, self.force_x11);
        for mime in mimes {
            match clipboard.read_typed(mime) {
                Ok(Some(value)) if !value.is_empty() => return Some(value),
                Ok(_) => continue,
                // 读取失败（例如选区持有者没有响应）：不再试其它类型，如实当作没有。
                Err(_) => return None,
            }
        }
        None
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

    /// 文件列表的一次变化：先看是不是自身写入，再看是不是和上次相同。
    fn poll_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        let Some(paths) = self.read_files()? else {
            return Ok(None);
        };
        let print = fingerprint_files(&paths);
        {
            let mut own = self.own_files.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(index) = own.iter().position(|item| *item == print) {
                own.remove(index);
                *self.last_files.lock().unwrap_or_else(|p| p.into_inner()) = Some(print);
                return Ok(None);
            }
        }
        {
            let mut last = self.last_files.lock().unwrap_or_else(|p| p.into_inner());
            if *last == Some(print) {
                return Ok(None);
            }
            *last = Some(print);
        }
        Ok(Some(paths))
    }
}

impl ClipboardWatcher for LinuxClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        // 文件列表优先：文件管理器复制文件时剪贴板里同时有 `text/uri-list` 与一段
        // 可读文字（URI 本身）。先按文件捕获，才能把「复制文件」与「复制这段文字」
        // 区分开，而不是把文件列表存成一条文字历史。
        if let Some(paths) = self.poll_files()? {
            return Ok(ClipboardPoll::Changed(ClipboardCapture {
                formats: vec![ClipboardFormatKind::Files],
                text: None,
                files: file_entries(&paths),
                source: self.source(),
            }));
        }
        let Some(text) = self.read()? else {
            // 剪贴板为空或只含本版本不支持的格式。
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
        // 同一次复制事件的富文本格式**必须与文本一起**进同一条捕获，不能拆成第二条。
        // 读取是尽力而为的：拿不到就按「这次事件里没有这个格式」处理。
        let html = self.read_rich(&["text/html"]);
        let rtf = self.read_rich(&["text/rtf", "application/rtf"]);
        let mut capture = ClipboardCapture::rich(text, html, rtf);
        capture.source = self.source();
        Ok(ClipboardPoll::Changed(capture))
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

    fn note_own_write_files(&self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        self.own_files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fingerprint_files(paths));
    }
}
