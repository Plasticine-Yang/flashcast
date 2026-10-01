//! 剪贴板适配（ADR §5 的 `ClipboardAccess` 与 `ClipboardWatcher`）。
//!
//! ticket 07 只需要「把文本写进系统剪贴板」这一项：备忘录的默认操作是粘贴，而
//! 自动粘贴（ticket 08）尚未实现，因此先复制并如实提示手动粘贴。图片、HTML/RTF 与
//! 文件列表属于剪贴板历史（ticket 09/10），会在同一 trait 上继续增加方法。
//!
//! ticket 09 增加「监听」这一半：剪贴板历史需要在用户复制后把内容捕获下来（ADR §5 的
//! `ClipboardWatcher`）。可用的 Rust 剪贴板库（`arboard`）**不提供变化事件**，因此这里
//! 采用轮询（见 research `app-discovery-and-focus.md` 剪贴板一节）；各平台按自己的方式
//! 判断「变化」：
//!
//! - Windows：`GetClipboardSequenceNumber`（系统维护的单调序号，最可靠）；
//! - Linux / macOS：外部工具（`wl-paste` / `xclip` / `pbpaste`）没有序号可读，
//!   因此用**内容指纹**判断变化。代价是同一段文字被复制两次只算一次变化；这与
//!   「重复内容去重」的结果一致，不影响用户体验，但**不能**用真实适配器验证「去重」
//!   ——那一条由宿主的 `content_hash` 保证，并用替身在集成测试里覆盖。
//!
//! ## 自身写入抑制
//!
//! Flashcast 自己也会写剪贴板（粘贴备忘录或历史条目）。这些写入**不能**再被自己捕获：
//! 那会形成「粘贴 → 捕获 → 又出现一条 → 再粘贴」的循环，历史会被自己的内容填满
//! （spec 明确要求）。适配层因此提供 [`ClipboardWatcher::note_own_write`]：宿主在写
//! 剪贴板时登记这次写入，`poll` 不会再把它报告成一次新的复制事件。宿主另有一层基于
//! 内容指纹的抑制作为兜底（见 `flashcast-core` 的捕获管线），两层各自成立。
//!
//! ## 为什么走系统自带工具
//!
//! Linux 的能力探测（[`crate::linux::cap`]）本来就用 `wl-copy` / `xclip` / `xsel`
//! 判断剪贴板是否可用，这一层沿用同一组工具，就不需要为三种平台各引入一套原生
//! 绑定；工具缺失时如实返回「未找到」，而不是假装复制成功。Wayland 下 `wl-copy`、
//! X11 下 `xclip`/`xsel`、macOS 下 `pbcopy` 都是对应桌面环境的标准组件。
//!
//! Windows 没有等价的可靠命令行工具（`clip.exe` 按控制台代码页解析输入，中文会乱码），
//! 因此 Windows 使用 `Win32` 剪贴板 API。
//!
//! ## 富文本格式（HTML / RTF）
//!
//! 一次复制事件可能同时带纯文本与 HTML/RTF。适配层的责任是：**捕获时把同一次事件的
//! 全部格式一起读出来**（[`ClipboardCapture`]），**恢复时把同一次事件的公开格式一起
//! 写回去**（[`ClipboardAccess::write_content`]），让目标应用自己挑。哪些格式真的可用
//! 取决于平台：
//!
//! - Windows：`CF_UNICODETEXT` + `HTML Format` + `Rich Text Format` 可以在一次剪贴板
//!   打开里同时提供，是唯一三种格式齐全的实现；
//! - Linux：`wl-paste` / `xclip` 能按 MIME 类型**读**（`--type text/html`、
//!   `-t text/rtf`），但写回时 `wl-copy` 一次只能指定一个 `--type`，再调用一次会接管
//!   选区并让上一个格式消失，因此只能提供纯文本，其余格式如实报告为未提供；
//! - macOS：`pbcopy` / `pbpaste` 只处理纯文本（`-Prefer rtf` 在没有 RTF 时会退回文本
//!   风味，无法区分，因此不声称支持），只提供纯文本。
//!
//! **从不声称**保留了任意应用私有格式：只有上面这些公开格式会被保存与恢复。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 把文本写入系统剪贴板。属性读取与写入都需要原生能力，因此只在平台层实现。
pub trait ClipboardAccess: Send + Sync {
    /// 写入文本。失败原因为面向用户的中文描述。
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;

    /// 读取当前剪贴板里的文本。剪贴板为空（或格式不是文本）时返回 `Ok(None)`。
    ///
    /// 读取失败与「没有文本」是两回事：前者返回 `Err`，宿主据此给出准确的失败状态，
    /// 而不是把它当成「这次没有内容」而静默跳过。
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;

    /// 把**文件列表**写进系统剪贴板，使用各平台的公开文件列表格式（ticket 12）：
    /// Linux 是 `text/uri-list`，Windows 是 `CF_HDROP`，macOS 是文件 URL 列表。
    ///
    /// 默认实现如实报告「当前平台没有实现」而不是静默成功：宿主据此给用户准确状态，
    /// 不会把「没写进去」说成「已复制」。
    fn write_files(&self, _paths: &[PathBuf]) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported {
            reason: "当前平台没有实现文件列表写入".to_string(),
        })
    }

    /// 读取当前剪贴板里的文件列表；剪贴板里没有文件列表时返回 `Ok(None)`。
    ///
    /// 读取失败与「没有文件列表」同样必须分开（与 [`Self::read_text`] 一致）。
    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        Ok(None)
    }

    /// 按**平台公开格式**一次写入同一次复制的内容：纯文本必选，HTML/RTF 可选。
    ///
    /// 目标应用自己挑它要的格式：富文本目标取 HTML/RTF（保留样式），纯文本目标取文本。
    /// 返回的 [`ClipboardWriteReport`] 必须**如实**说明哪种格式真的进了系统剪贴板；
    /// 平台后端做不到「一次提供多种格式」时（见 [`ClipboardWriteReport`]），
    /// 默认实现退化为只写纯文本并报告其余格式被跳过，**不**假装全部保留。
    fn write_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, ClipboardError> {
        self.write_text(&content.text)?;
        Ok(ClipboardWriteReport::text_only(
            "当前平台的剪贴板后端一次只能提供纯文本",
            content
                .requested_formats()
                .into_iter()
                .filter(|kind| *kind != ClipboardFormatKind::Text),
        ))
    }
}

/// 要写进剪贴板的一次内容：纯文本必选，HTML/RTF 可选。
///
/// 这是恢复剪贴板历史时交给系统的东西——**同一次复制**的全部公开格式放在一起，
/// 让目标应用自己选，而不是由 Flashcast 猜。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClipboardContent {
    pub text: String,
    pub html: Option<String>,
    pub rtf: Option<String>,
}

impl ClipboardContent {
    /// 只有文本的内容。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            html: None,
            rtf: None,
        }
    }

    /// 附上 HTML 载荷（空串按「没有这个格式」处理）。
    pub fn with_html(mut self, html: Option<String>) -> Self {
        self.html = html.filter(|value| !value.is_empty());
        self
    }

    /// 附上 RTF 载荷（空串按「没有这个格式」处理）。
    pub fn with_rtf(mut self, rtf: Option<String>) -> Self {
        self.rtf = rtf.filter(|value| !value.is_empty());
        self
    }

    /// 请求写入的格式集合，顺序固定：文本、HTML、RTF。
    pub fn requested_formats(&self) -> Vec<ClipboardFormatKind> {
        let mut formats = vec![ClipboardFormatKind::Text];
        if self.html.is_some() {
            formats.push(ClipboardFormatKind::Html);
        }
        if self.rtf.is_some() {
            formats.push(ClipboardFormatKind::Rtf);
        }
        formats
    }

    /// 有没有富文本载荷。
    pub fn has_rich(&self) -> bool {
        self.html.is_some() || self.rtf.is_some()
    }
}

/// 一种没能写进系统剪贴板的格式与中文原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardSkippedFormat {
    pub kind: ClipboardFormatKind,
    pub reason: String,
}

/// 一次写入之后，系统剪贴板里**真的**有哪几种格式。
///
/// 这是「如实降级」的载体：平台的剪贴板后端可能无法一次提供多种格式——例如 Wayland
/// 的 `wl-copy` 每次调用只能指定**一个** `--type`，再调用一次会接管选区并让上一次的
/// 格式消失（`wl-clipboard 2.2.1` 的 man page：`-t` 决定「wl-copy 提供内容的类型」，
/// 单数）。这种情况下报告必须写明实际提供的格式与未提供的格式及原因，宿主据此给用户
/// 准确的中文反馈，而不是声称富文本样式已经保留。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ClipboardWriteReport {
    /// 真的进了系统剪贴板的格式。
    pub formats: Vec<ClipboardFormatKind>,
    /// 没能写进去的格式与原因。
    pub skipped: Vec<ClipboardSkippedFormat>,
}

impl ClipboardWriteReport {
    /// 只写入了文本，并如实记下被跳过的富文本格式（都被同一个原因跳过）。
    pub fn text_only(reason: &str, skipped: impl IntoIterator<Item = ClipboardFormatKind>) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Text],
            skipped: skipped
                .into_iter()
                .map(|kind| ClipboardSkippedFormat {
                    kind,
                    reason: reason.to_string(),
                })
                .collect(),
        }
    }

    /// 有没有格式被跳过（即这次恢复没能把全部格式交回系统）。
    pub fn degraded(&self) -> bool {
        !self.skipped.is_empty()
    }

    /// 面向用户的中文说明：实际提供了哪些格式，哪些没有以及为什么。
    pub fn describe_zh(&self) -> String {
        let provided: Vec<&str> = self.formats.iter().map(|kind| kind.label_zh()).collect();
        let mut text = format!("剪贴板已提供：{}", provided.join("、"));
        if self.skipped.is_empty() {
            return text;
        }
        let skipped: Vec<String> = self
            .skipped
            .iter()
            .map(|item| format!("{}（{}）", item.kind.label_zh(), item.reason))
            .collect();
        text.push_str(&format!("；未提供：{}", skipped.join("、")));
        text
    }
}

/// 一次复制事件里存在的格式。v0.1.0 只捕获文本，其余取值是 ticket 10–12 的扩展点：
/// 捕获结构体会带上对应内容，而宿主的存储 schema 已经能容纳它们。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardFormatKind {
    Text,
    Html,
    Rtf,
    Image,
    Files,
}

impl ClipboardFormatKind {
    /// 存储里的稳定标签（`clipboard_formats.kind`）。
    pub fn tag(self) -> &'static str {
        match self {
            ClipboardFormatKind::Text => "text",
            ClipboardFormatKind::Html => "html",
            ClipboardFormatKind::Rtf => "rtf",
            ClipboardFormatKind::Image => "image",
            ClipboardFormatKind::Files => "files",
        }
    }

    /// 面向用户的中文格式名（结果副标题与恢复反馈都用它）。
    pub fn label_zh(self) -> &'static str {
        match self {
            ClipboardFormatKind::Text => "文字",
            ClipboardFormatKind::Html => "HTML",
            ClipboardFormatKind::Rtf => "RTF",
            ClipboardFormatKind::Image => "图片",
            ClipboardFormatKind::Files => "文件",
        }
    }
}

/// 剪贴板内容的来源应用（平台能提供时）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardSourceApp {
    /// 稳定标识：X11 的 `WM_CLASS` 实例名 / Windows 的进程名 / macOS 的 bundle id。
    pub app_id: String,
    /// 面向用户的名称；无法获得时为 `app_id`。
    pub title: Option<String>,
}

/// 剪贴板里的一个文件条目（ticket 12）。
///
/// v0.1.0 **把视频也按文件处理**：视频文件与其它文件走完全相同的捕获、引用与副本路径，
/// 没有录屏，也没有从任意应用提取视频片段。
///
/// `bytes` 是**观察值**：原文件可能已经被移动或删除，因此读不到时是 `None`，由宿主的
/// 存储层记为 0，并在界面里如实显示「不可恢复」而不是猜一个大小。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFileEntry {
    /// 原文件路径（引用类附件指向它）。
    pub path: PathBuf,
    /// 展示用名称：文件名；路径没有文件名时用末段。
    pub name: String,
    /// 观察到的字节数；读不到时为 `None`。
    pub bytes: Option<u64>,
    /// 按扩展名推断的 MIME（尽力而为）；未知时 `None`。
    pub mime: Option<String>,
}

/// 一次复制事件里最多保存多少个文件。超过上限的列表如实拒绝，而不是截断。
pub const MAX_FILES: usize = 1024;

/// 由文件路径构造一个文件条目：名称、大小与 MIME 都是**尽力而为**的观察值。
pub fn file_entry(path: impl Into<PathBuf>) -> ClipboardFileEntry {
    let path = path.into();
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());
    let bytes = std::fs::metadata(&path).ok().map(|meta| meta.len());
    let mime = mime_for_path(&path);
    ClipboardFileEntry {
        path,
        name,
        bytes,
        mime,
    }
}

/// 一组路径的文件条目。
pub fn file_entries(paths: &[PathBuf]) -> Vec<ClipboardFileEntry> {
    paths.iter().cloned().map(file_entry).collect()
}

/// 按扩展名推断 MIME。只为展示与检索服务，拿不准时如实返回 `None`。
pub fn mime_for_path(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase())?;
    let mime = match extension.as_str() {
        "txt" | "log" | "md" => "text/plain",
        "html" | "htm" => "text/html",
        "rtf" => "application/rtf",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "csv" => "text/csv",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "mkv" => "video/x-matroska",
        "webm" => "video/webm",
        "avi" => "video/x-msvideo",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        _ => return None,
    };
    Some(mime.to_string())
}

/// 轮询到的一次新复制事件。
///
/// 一次复制事件的**全部**格式在同一个结构体里：纯文本与它受支持的富文本格式必须一起
/// 进入同一条历史，不允许按格式拆成多条记录（那样历史里会出现互相重复的条目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCapture {
    /// 本次事件里存在的格式集合。
    pub formats: Vec<ClipboardFormatKind>,
    /// 文本内容（可索引的检索文字；富文本载荷**不**参与检索）。文件列表事件通常没有它。
    pub text: Option<String>,
    /// HTML 载荷（平台公开的 `text/html`）。没有时如实为 `None`。
    pub html: Option<String>,
    /// RTF 载荷（平台公开的 `text/rtf` 或 Windows 的 `Rich Text Format`）。
    pub rtf: Option<String>,
    /// 文件列表（ticket 12）。空表示这次复制事件里没有文件。
    pub files: Vec<ClipboardFileEntry>,
    /// 来源应用（平台可提供时）。
    pub source: Option<ClipboardSourceApp>,
}

impl ClipboardCapture {
    /// 只有文本的一次复制事件。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Text],
            text: Some(text.into()),
            files: Vec::new(),
            html: None,
            rtf: None,
            source: None,
        }
    }

    /// 只有文件列表的一次复制事件（视频文件也是文件）。
    pub fn files(paths: &[PathBuf]) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Files],
            text: None,
            files: file_entries(paths),
            html: None,
            rtf: None,
            source: None,
        }
    }

    /// 一次携带 HTML / RTF 的复制事件。
    ///
    /// 空串按「没有这个格式」处理：宁可不声称，也不留下一个空载荷。格式集合由**真的
    /// 带来了内容**的字段推导，因此宿主记录的格式集合与载荷始终一致。
    pub fn rich(text: impl Into<String>, html: Option<String>, rtf: Option<String>) -> Self {
        let html = html.filter(|value| !value.is_empty());
        let rtf = rtf.filter(|value| !value.is_empty());
        let mut formats = vec![ClipboardFormatKind::Text];
        if html.is_some() {
            formats.push(ClipboardFormatKind::Html);
        }
        if rtf.is_some() {
            formats.push(ClipboardFormatKind::Rtf);
        }
        Self {
            formats,
            text: Some(text.into()),
            html,
            rtf,
            files: Vec::new(),
            source: None,
        }
    }

    /// 这次事件是否带有文件列表。
    pub fn has_files(&self) -> bool {
        !self.files.is_empty()
    }

    /// 本次捕获里真的带来了内容的富文本格式。
    pub fn rich_formats(&self) -> Vec<ClipboardFormatKind> {
        let mut formats = Vec::new();
        if self.html.as_deref().is_some_and(|html| !html.is_empty()) {
            formats.push(ClipboardFormatKind::Html);
        }
        if self.rtf.as_deref().is_some_and(|rtf| !rtf.is_empty()) {
            formats.push(ClipboardFormatKind::Rtf);
        }
        formats
    }
}

/// 一次轮询的结果。区分「没有变化」与「变化了但内容不可捕获」，宿主才能给出准确状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPoll {
    /// 自上次轮询以来没有新的复制事件。
    Unchanged,
    /// 有新的复制事件。
    Changed(ClipboardCapture),
}

/// 剪贴板变化监听（ADR §5）。
///
/// 实现必须满足两条：
///
/// 1. `poll` **有界返回**：拿不到选区（自动化会话、Wayland 缺 `wl-clipboard`）时要如实
///    报错，不能挂住调用方；
/// 2. `note_own_write` 之后再 `poll` **不能**把那次写入报告成新的复制事件。
pub trait ClipboardWatcher: Send + Sync {
    /// 轮询一次剪贴板。
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError>;

    /// 登记一次由 Flashcast 自己发起的剪贴板写入（自身写入抑制）。
    fn note_own_write(&self, text: &str);

    /// 登记一次由 Flashcast 自己发起的**文件列表**写入（自身写入抑制，ticket 12）。
    ///
    /// 默认是空操作，具体平台按自己的文件列表指纹抑制；宿主另有一层基于内容指纹的兜底
    /// （见 `flashcast-core` 的捕获管线），两层各自成立。
    fn note_own_write_files(&self, _paths: &[PathBuf]) {}
}

/// 内容指纹：没有平台序号可读时用它判断剪贴板是否变化。
///
/// 只用标准库的 `DefaultHasher`，不引入额外依赖，也不需要密码学强度——它只用来
/// 判断「和上次读到的是不是同一段文字」。
pub fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// 文件列表的内容指纹：用于适配层判断「还是不是同一份文件列表」。
///
/// 只看路径本身（保序）：文件被复制两次时大小可能不同（原文件在改），而用户看到的是
/// 「同一份列表」；这与宿主的 `content_hash_files` 保持同一判据。
pub fn fingerprint_files(paths: &[PathBuf]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    paths.hash(&mut hasher);
    hasher.finish()
}

/// 校验待写入的文件列表。
pub fn check_files(paths: &[PathBuf]) -> Result<(), ClipboardError> {
    if paths.is_empty() {
        return Err(ClipboardError::Failed(
            "文件列表为空，没有可复制的东西".to_string(),
        ));
    }
    if paths.len() > MAX_FILES {
        return Err(ClipboardError::Failed(format!(
            "文件列表有 {} 个文件，超过上限 {MAX_FILES}",
            paths.len()
        )));
    }
    if paths.iter().any(|path| path.as_os_str().is_empty()) {
        return Err(ClipboardError::Failed(
            "文件列表里有空路径，已拒绝写入剪贴板".to_string(),
        ));
    }
    Ok(())
}

/// 解析 Linux 的 `text/uri-list`（RFC 2483）与 GNOME 的 `x-special/gnome-copied-files`。
///
/// 处理了实际会遇到的三件事：
///
/// - 行分隔是 `\r\n`，`#` 开头是注释，空行忽略；
/// - GNOME 的私有种格式第一行是 `copy` / `cut`，要跳过；
/// - 路径里含**空格**或**非 ASCII** 时是百分号编码的 UTF-8（`%20`、`%E6%8A%A5`），
///   也见得到未编码的原始 UTF-8——两种都要还原成同一份路径。
///
/// 非 `file://` 的行（例如 `http://`）会被忽略：剪贴板历史只保存本机文件引用。
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r').trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // GNOME 私有种格式的首行标记。
        if index == 0 && (line == "copy" || line == "cut") {
            continue;
        }
        if let Some(path) = parse_file_uri(line) {
            paths.push(path);
        }
    }
    paths
}

/// 把一行 `file://` URI 还原成本机路径；不是本机文件 URI 时返回 `None`。
fn parse_file_uri(line: &str) -> Option<PathBuf> {
    let rest = line.strip_prefix("file://")?;
    // `file://host/path`：只有空 authority 或 localhost 才算本机文件。
    // 路径**包含** authority 之后的那个 `/`（`file:///home/a` 的路径是 `/home/a`）。
    let path = match rest.find('/') {
        Some(0) => rest,
        Some(slash) => {
            let authority = &rest[..slash];
            if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
                return None;
            }
            &rest[slash..]
        }
        None => return None,
    };
    if path.is_empty() {
        return None;
    }
    Some(PathBuf::from(percent_decode(path)))
}

/// 百分号解码（按字节解码再按 UTF-8 解释，因此非 ASCII 路径能正确还原）。
fn percent_decode(input: &str) -> String {
    fn hex(byte: u8) -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            b'A'..=b'F' => Some(byte - b'A' + 10),
            _ => None,
        }
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[index + 1]), hex(bytes[index + 2])) {
                out.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// 把本机路径编码成一行 `file://` URI（供 `text/uri-list` 使用）。
///
/// 只做必要的百分号编码：非 ASCII、空格、`%`、`#`、`?` 与控制字符。`/` 与 ASCII 可见
/// 字符原样保留，因此 `parse_uri_list(format_uri_list(p))` 一定回到同一个路径。
pub fn encode_file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    let text = path.to_string_lossy();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        let keep = byte >= 0x21
            && byte < 0x7f
            && byte != b'%'
            && byte != b'#'
            && byte != b'?'
            && byte != b'"';
        if keep {
            uri.push(byte as char);
        } else {
            uri.push('%');
            uri.push_str(&format!("{byte:02X}"));
        }
        index += 1;
    }
    uri
}

/// 把文件列表编码成 `text/uri-list` 内容（每行一个 URI，CRLF 结尾）。
pub fn format_uri_list(paths: &[PathBuf]) -> String {
    let mut out = String::new();
    for path in paths {
        out.push_str(&encode_file_uri(path));
        out.push_str("\r\n");
    }
    out
}

// ---------------------------------------------------------------------------
// Windows 的 `HTML Format`（CF_HTML）
// ---------------------------------------------------------------------------

/// `HTML Format` 头部里的片段起始键。
const CF_HTML_START: &str = "StartFragment:";
/// `HTML Format` 头部里的片段结束键。
const CF_HTML_END: &str = "EndFragment:";

/// CF_HTML 头部的唯一拼装点。四个偏移都用 `{:010}` 定宽输出，因此格式化后的头部长度与
/// 偏移值无关——这正是 CF_HTML 偏移可以自洽回填的前提。
fn cf_html_head(
    start_html: usize,
    end_html: usize,
    start_fragment: usize,
    end_fragment: usize,
) -> String {
    use std::fmt::Write;
    let mut head = String::new();
    // `write!` 到 String 不会失败；偏移超过 10 位时长度会变，因此下面用断言兜住。
    let _ = write!(
        head,
        "Version:1.0\r\nStartHTML:{start_html:010}\r\nEndHTML:{end_html:010}\r\nStartFragment:{start_fragment:010}\r\nEndFragment:{end_fragment:010}\r\n"
    );
    head
}

/// 把一段 HTML 片段编码成 Windows `HTML Format`（CF_HTML）载荷。
///
/// CF_HTML 是「头部 + 完整文档」的字节串，头部里的偏移是**从字节串开头算起的字节数**，
/// 而且是定宽 10 位十进制。先用全 0 偏移量一次头部长度（与真实头部等长），再回填真实
/// 偏移并断言长度未变。
///
/// 这个函数是**纯逻辑**，放在跨平台模块里，好让 Linux 开发机上也能用真实字节验证编码
/// （Windows 适配层本身只能在 Windows 上编译与运行）。
pub fn cf_html_bytes(fragment: &str) -> Vec<u8> {
    let head_len = cf_html_head(0, 0, 0, 0).len();
    const OPEN: &str = "<html><body><!--StartFragment-->";
    const CLOSE: &str = "<!--EndFragment--></body></html>";
    let start_fragment = head_len + OPEN.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + CLOSE.len();
    let head = cf_html_head(head_len, end_html, start_fragment, end_fragment);
    debug_assert_eq!(head.len(), head_len, "CF_HTML 头部必须是定宽偏移");
    let mut bytes = head.into_bytes();
    bytes.extend_from_slice(OPEN.as_bytes());
    bytes.extend_from_slice(fragment.as_bytes());
    bytes.extend_from_slice(CLOSE.as_bytes());
    bytes
}

/// 从 Windows `HTML Format` 载荷里取出 HTML 片段。
///
/// 返回 `None` 表示载荷不是 CF_HTML（没有可解析的 `StartFragment` / `EndFragment`），
/// 调用方据此决定是拒绝还是原样保留——本适配层选择不猜。
pub fn cf_html_fragment(payload: &str) -> Option<String> {
    // 头部里的键在文档开始之前；只在开头一段里找，避免正文里的同名文本被误当成头部。
    let head_end = payload
        .find("<html")
        .or_else(|| payload.find("<!DOCTYPE"))
        .unwrap_or(payload.len())
        .min(payload.len());
    let head = &payload[..head_end];
    let start = cf_html_offset(head, CF_HTML_START)?;
    let end = cf_html_offset(head, CF_HTML_END)?;
    // 偏移颠倒或越界说明头部不可信；相等是合法的（空片段）。
    if start > end || end > payload.len() {
        return None;
    }
    // 偏移落在字符边界之外时 `get` 返回 `None`，同样视为不可解析。
    payload.get(start..end).map(|fragment| fragment.to_string())
}

/// 从 CF_HTML 头部里读一个字节偏移。
fn cf_html_offset(head: &str, key: &str) -> Option<usize> {
    head.lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix(key))
        .and_then(|value| value.trim().parse::<usize>().ok())
}

/// 剪贴板操作的失败原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    #[error("当前会话不支持写入剪贴板：{reason}")]
    Unsupported { reason: String },
    #[error("未找到可用的剪贴板工具：{reason}")]
    ToolMissing { reason: String },
    #[error("写入剪贴板失败：{0}")]
    Failed(String),
    /// 读取失败与写入失败是两件事：面向用户的说明必须说清是哪一半失败了，
    /// 否则「拿不到剪贴板选区」会被误读成「写入坏了」。
    #[error("读取剪贴板失败：{0}")]
    ReadFailed(String),
}

/// 文本写入的最大长度。剪贴板工具对超长文本没有明确上限，这里只做防呆。
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// 校验待写入的文本。
pub fn check_text(text: &str) -> Result<(), ClipboardError> {
    if text.trim().is_empty() {
        return Err(ClipboardError::Failed(
            "内容为空，没有可复制的东西".to_string(),
        ));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(ClipboardError::Failed(format!(
            "内容超过 {} MB，已拒绝写入剪贴板",
            MAX_TEXT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// 在 `PATH` 中查找一个可执行文件。
pub(crate) fn find_program(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // macOS 的最小 PATH（例如从 Finder 启动）常常看不到 /usr/bin，因此再查一遍
    // 系统目录，避免把「工具存在但不在 PATH 里」误报成缺失。
    for dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
        let candidate = Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 等待外部剪贴板工具返回的上限。
///
/// 这些工具在正常桌面会话里立刻返回（它们 fork 出后台进程持有选区）。但在拿不到选区的
/// 会话里（例如自动化会话没有可用的输入序列）`wl-copy` 会**永久阻塞**——实测观察到的
/// 就是 5 分钟不返回。宿主执行「复制」时不能因此挂住，所以超过这个上限就杀掉进程并如实
/// 报告「剪贴板工具没有返回」，由宿主的降级路径给用户中文反馈。
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// 从外部剪贴板工具读取文本时的等待上限。
///
/// 与写入同理：Wayland 的选区由持有者进程提供，自动化会话里 `wl-paste` 可能永远等不到
/// 内容。宿主在后台按固定间隔轮询，一旦某次读取卡住，整个捕获线程就再也不会前进，
/// 因此读取同样必须**有界**。
pub const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// 用一个外部剪贴板工具写入文本：文本走标准输入。
///
/// 这些工具（`wl-copy`、`xclip`、`pbcopy`）都会 fork 出后台进程持有选区，父进程随后
/// 退出，因此必须显式关闭标准输入再 `wait`，避免写入端被 SIGPIPE 打断。
///
/// 等待是**有界**的（[`WRITE_TIMEOUT`]）。标准错误由独立线程读取：工具 fork 出的守护
/// 进程会继承这个管道，父进程退出后管道仍未关闭，若在主线程里读到 EOF 就会再次挂住。
pub(crate) fn write_with_tool(
    program: &Path,
    args: &[&str],
    text: &str,
) -> Result<(), ClipboardError> {
    check_text(text)?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ClipboardError::Failed(format!("无法启动 {}：{error}", program.display()))
        })?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| ClipboardError::Failed("无法写入剪贴板工具的标准输入".to_string()))?;
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| ClipboardError::Failed(format!("写入剪贴板失败：{error}")))?;
    }
    // 关闭标准输入（drop 掉句柄）后再等待，工具才知道内容已经结束。
    drop(child.stdin.take());
    let stderr = child.stderr.take();
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + WRITE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // 不 join 读取线程：它的管道可能永远不关闭；进程退出时线程自然消失。
                return Err(ClipboardError::Failed(format!(
                    "剪贴板工具 {} 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
                    program.display(),
                    WRITE_TIMEOUT.as_secs()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(ClipboardError::Failed(format!(
                    "等待剪贴板工具失败：{error}"
                )));
            }
        }
    };
    let stderr = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let stderr = stderr.trim();
        return Err(ClipboardError::Failed(if stderr.is_empty() {
            format!("剪贴板工具 {} 以状态 {status} 退出", program.display())
        } else {
            format!("剪贴板工具 {} 失败：{stderr}", program.display())
        }));
    }
    Ok(())
}

/// 用一个外部剪贴板工具读取文本：内容走标准输出。
///
/// 与 [`write_with_tool`] 一样是**有界**的，并且用独立线程读取标准输出：读取端在主线程
/// 里等到 EOF 会再次挂住（选区持有者可能一直不关管道）。工具超时会被杀掉并如实报错。
pub(crate) fn read_with_tool(
    program: &Path,
    args: &[&str],
) -> Result<Option<String>, ClipboardError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            ClipboardError::ReadFailed(format!("无法启动 {}：{error}", program.display()))
        })?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stdout) = stdout {
            let _ = std::io::Read::read_to_end(&mut stdout, &mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + READ_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // 不 join 读取线程：它的管道可能永远不关闭；进程退出时线程自然消失。
                return Err(ClipboardError::ReadFailed(format!(
                    "剪贴板工具 {} 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
                    program.display(),
                    READ_TIMEOUT.as_secs()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(ClipboardError::ReadFailed(format!(
                    "等待剪贴板工具失败：{error}"
                )));
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    if !status.success() {
        // 工具以非 0 退出通常表示「当前剪贴板里没有这种格式」，按「没有文本」处理，
        // 而不是报错——空的剪贴板是正常状态。
        return Ok(None);
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// 工具卡住时必须**有界**返回，而不是让宿主的执行入口挂死。
    ///
    /// 这里用真实的 `sh -c 'sleep 30'` 当替身：它是真的会阻塞的进程，不依赖任何平台替身。
    #[test]
    fn blocking_tool_is_aborted_after_the_timeout() {
        let started = Instant::now();
        let result = write_with_tool(Path::new("/bin/sh"), &["-c", "sleep 30"], "正文");
        let elapsed = started.elapsed();
        let error = result.expect_err("阻塞的工具必须被判定为失败");
        assert!(
            error.to_string().contains("没有返回"),
            "失败原因必须说明工具没有返回：{error}"
        );
        assert!(
            elapsed < WRITE_TIMEOUT * 3,
            "必须在超时附近返回，实际耗时 {elapsed:?}"
        );
    }

    /// 读取同样必须有界：后台轮询线程一旦卡死，剪贴板历史就再也不会前进。
    #[test]
    fn blocking_read_tool_is_aborted_after_the_timeout() {
        let started = Instant::now();
        let result = read_with_tool(Path::new("/bin/sh"), &["-c", "sleep 30"]);
        let elapsed = started.elapsed();
        let error = result.expect_err("阻塞的读取必须被判定为失败");
        assert!(
            error.to_string().contains("没有返回"),
            "失败原因必须说明工具没有返回：{error}"
        );
        assert!(
            elapsed < READ_TIMEOUT * 3,
            "必须在超时附近返回，实际耗时 {elapsed:?}"
        );
    }

    /// 正常读取返回工具的标准输出；空输出按「剪贴板里没有文本」处理，不是错误。
    #[test]
    fn read_tool_returns_output_and_treats_empty_as_none() {
        let text = read_with_tool(Path::new("/bin/sh"), &["-c", "printf '来自剪贴板'"])
            .expect("正常读取不应失败");
        assert_eq!(text.as_deref(), Some("来自剪贴板"));
        let empty = read_with_tool(Path::new("/bin/sh"), &["-c", ":"]).expect("空输出不应失败");
        assert_eq!(empty, None);
        // 工具以非 0 退出表示「当前剪贴板没有这种格式」，同样不是错误。
        let missing =
            read_with_tool(Path::new("/bin/sh"), &["-c", "exit 1"]).expect("非 0 退出不应失败");
        assert_eq!(missing, None);
    }

    /// 文件列表往返：**含空格**与**非 ASCII** 的路径必须回到同一个路径（ticket 12）。
    #[test]
    fn uri_list_round_trips_spaces_and_non_ascii_paths() {
        let paths = vec![
            PathBuf::from("/home/user/报告 草稿.pdf"),
            PathBuf::from("/home/user/带#井号 与%百分号.txt"),
            PathBuf::from("/home/user/普通.txt"),
        ];
        let encoded = format_uri_list(&paths);
        // 空格与非 ASCII 都必须被编码，否则 URI 会在第一个空格处被截断。
        assert!(encoded.contains("%20"), "空格必须编码：{encoded}");
        assert!(!encoded.contains(' '), "URI 里不应出现裸空格：{encoded}");
        assert_eq!(
            encoded.lines().count(),
            paths.len(),
            "每个文件一行：{encoded}"
        );
        assert_eq!(parse_uri_list(&encoded), paths, "往返必须回到同一份路径");
    }

    /// 实际格式里会遇到的东西：CRLF、注释、`copy` 首行、非本机 URI、localhost。
    #[test]
    fn uri_list_parsing_handles_real_world_shapes() {
        let raw = "# 这是注释\r\n\
                   file:///home/user/一.txt\r\n\
                   \r\n\
                   https://example.com/not-a-file\r\n\
                   file://localhost/home/user/two.txt\r\n\
                   file://remote-host/home/user/three.txt\r\n";
        assert_eq!(
            parse_uri_list(raw),
            vec![
                PathBuf::from("/home/user/一.txt"),
                PathBuf::from("/home/user/two.txt"),
            ],
            "只保留本机 file:// 路径"
        );
        // GNOME 私有种格式（x-special/gnome-copied-files）：首行是 copy / cut。
        let gnome = "copy\r\nfile:///home/user/a%20b.txt\r\n";
        assert_eq!(
            parse_uri_list(gnome),
            vec![PathBuf::from("/home/user/a b.txt")]
        );
        // 未编码的原始 UTF-8 与百分号编码的 UTF-8 必须还原成同一个路径。
        assert_eq!(
            parse_uri_list("file:///home/user/报告.txt"),
            parse_uri_list("file:///home/user/%E6%8A%A5%E5%91%8A.txt")
        );
    }

    /// 文件列表的内容指纹只看路径，且空列表与有内容可区分。
    #[test]
    fn file_fingerprint_follows_paths() {
        let a = vec![PathBuf::from("/home/user/a.txt")];
        let b = vec![PathBuf::from("/home/user/b.txt")];
        assert_eq!(fingerprint_files(&a), fingerprint_files(&a));
        assert_ne!(fingerprint_files(&a), fingerprint_files(&b));
        assert_ne!(fingerprint_files(&[]), fingerprint_files(&a));
    }

    /// 写文件列表前的校验：空列表与超上限如实拒绝，而不是写入半份列表。
    #[test]
    fn check_files_rejects_empty_and_oversized_lists() {
        assert!(check_files(&[]).is_err());
        let over: Vec<PathBuf> = (0..=MAX_FILES)
            .map(|index| PathBuf::from(format!("/tmp/f{index}")))
            .collect();
        assert!(check_files(&over).is_err());
        assert!(check_files(&[PathBuf::from("/tmp/ok.txt")]).is_ok());
    }
}

/// CF_HTML 编解码是**纯逻辑**，在 Linux 上也要有真实字节覆盖：Windows 适配层只能在
/// Windows 上编译运行，如果把它写进 `cfg(target_os = "windows")` 就永远没有本地证据。
#[cfg(test)]
mod cf_html_tests {
    use super::{cf_html_bytes, cf_html_fragment};

    /// 头部里的偏移必须真的指向片段：按偏移切出来的字节就是原文。
    #[test]
    fn cf_html_offsets_point_at_the_fragment() {
        let fragment = "<b>加粗</b>与中文";
        let payload = cf_html_bytes(fragment);
        let payload = String::from_utf8(payload).expect("CF_HTML 头部是 ASCII + UTF-8 正文");
        assert!(payload.starts_with("Version:1.0\r\nStartHTML:"));
        // 头部是定宽的：StartHTML 的值就是正文开始的位置。
        let head_len: usize = payload
            .lines()
            .find_map(|line| line.strip_prefix("StartHTML:"))
            .and_then(|value| value.trim().parse().ok())
            .expect("头部必须有 StartHTML");
        assert_eq!(
            &payload[head_len..head_len + "<html>".len()],
            "<html>",
            "StartHTML 必须指向 <html> 的开头"
        );
        assert_eq!(cf_html_fragment(&payload).as_deref(), Some(fragment));
    }

    /// 空片段与含 CRLF 的片段同样往返成功。
    #[test]
    fn cf_html_round_trips_empty_and_newline_fragments() {
        for fragment in ["", "第一行\r\n第二行", "<p>a</p><p>b</p>"] {
            let payload = String::from_utf8(cf_html_bytes(fragment)).expect("UTF-8");
            assert_eq!(
                cf_html_fragment(&payload).as_deref(),
                Some(fragment),
                "片段「{fragment}」必须原样取回"
            );
        }
    }

    /// 不是 CF_HTML 的载荷（例如浏览器直接给的 HTML 片段）返回 `None`，调用方据此不猜。
    #[test]
    fn non_cf_html_payload_is_not_parsed() {
        assert_eq!(cf_html_fragment("<b>纯 HTML，没有头部</b>"), None);
        assert_eq!(cf_html_fragment(""), None);
        // 偏移越界 / 颠倒时同样不猜，也不 panic。
        assert_eq!(
            cf_html_fragment("StartHTML:0000000000\r\nStartFragment:0000000099\r\nEndFragment:0000000001\r\n<html></html>"),
            None
        );
    }
}
