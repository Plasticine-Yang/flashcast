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

    /// 写入一张图片（ticket 10）。失败原因为面向用户的中文描述。
    ///
    /// 默认实现如实报告「不支持」：新增这一项能力时，其它目标的实现不会因此静默
    /// 变成「写入成功」。
    fn write_image(&self, _image: &ClipboardImage) -> Result<(), ClipboardError> {
        Err(ClipboardError::Unsupported {
            reason: "当前平台适配层还没有实现图片写入".to_string(),
        })
    }

    /// 读取当前剪贴板里的图片。剪贴板里没有图片格式时返回 `Ok(None)`。
    ///
    /// 与 [`Self::read_text`] 同理：读取失败返回 `Err`，不折叠成「没有图片」。
    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Err(ClipboardError::Unsupported {
            reason: "当前平台适配层还没有实现图片读取".to_string(),
        })
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

/// 本版本保存图片时统一使用的 MIME。
///
/// 三个平台读到的图片都会被规范化为 PNG（Linux 直接读 `image/png` 目标，Windows 把
/// `CF_DIB` 或注册格式 `PNG` 编码成 PNG，macOS 从 `NSPasteboard` 取 `PNGf`），
/// 因此存储、缩略图与恢复只需要一条编码路径。
pub const IMAGE_MIME_PNG: &str = "image/png";

/// 单张图片的字节上限。
///
/// 超过上限的图片**不保存**，并由捕获结果里的 [`ClipboardCapture::image_problem`]
/// 如实说明原因：生成一条「看起来保存成功、实际恢复不了」的历史是明确禁止的
/// （spec 用户故事 53「超出容量、格式不支持或文件失效时看到明确状态」）。
pub const MAX_IMAGE_BYTES: usize = 16 * 1024 * 1024;

/// 从剪贴板读到（或准备写回剪贴板）的一张图片。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardImage {
    /// 规范化之后的 MIME（本版本总是 [`IMAGE_MIME_PNG`]）。
    pub mime: String,
    /// 图片字节。
    pub bytes: Vec<u8>,
    /// 像素宽度；无法从文件头判定时为 0。
    pub width: u32,
    /// 像素高度；无法从文件头判定时为 0。
    pub height: u32,
}

impl ClipboardImage {
    /// 用字节构造一张图片，并从文件头解析尺寸（解析不出时尺寸为 0，不猜）。
    pub fn new(mime: impl Into<String>, bytes: Vec<u8>) -> Self {
        let (width, height) = image_dimensions(&bytes).unwrap_or((0, 0));
        Self {
            mime: mime.into(),
            bytes,
            width,
            height,
        }
    }

    /// 人类可读的尺寸描述，用于结果副标题。
    pub fn size_label(&self) -> String {
        if self.width == 0 || self.height == 0 {
            "尺寸未知".to_string()
        } else {
            format!("{}×{}", self.width, self.height)
        }
    }
}

/// 从图片文件头解析像素尺寸。
///
/// 识别 PNG / JPEG / GIF / BMP；识别不出时返回 `None`，调用方据此显示「尺寸未知」，
/// 而不是猜一个数字。所有分支都只做**有界**的头部读取，坏数据不会让它越界。
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    png_dimensions(bytes)
        .or_else(|| jpeg_dimensions(bytes))
        .or_else(|| gif_dimensions(bytes))
        .or_else(|| bmp_dimensions(bytes))
}

fn be_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_be_bytes(slice.try_into().ok()?))
}

fn le_u16(bytes: &[u8], at: usize) -> Option<u16> {
    let slice = bytes.get(at..at + 2)?;
    Some(u16::from_le_bytes(slice.try_into().ok()?))
}

fn le_u32(bytes: &[u8], at: usize) -> Option<u32> {
    let slice = bytes.get(at..at + 4)?;
    Some(u32::from_le_bytes(slice.try_into().ok()?))
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const MAGIC: &[u8] = &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    if !bytes.starts_with(MAGIC) || bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    Some((be_u32(bytes, 16)?, be_u32(bytes, 20)?))
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut at = 2usize;
    // 段长度是 16 位，因此循环最多走完文件；同时设一个硬上限避免坏数据空转。
    let mut guard = 0usize;
    while at + 4 <= bytes.len() && guard < 4096 {
        guard += 1;
        if bytes[at] != 0xff {
            at += 1;
            continue;
        }
        let marker = bytes[at + 1];
        if marker == 0xff {
            at += 1;
            continue;
        }
        // 无长度的标记：RSTn、SOI、EOI。
        if (0xd0..=0xd9).contains(&marker) {
            at += 2;
            continue;
        }
        let length = u16::from_be_bytes(bytes.get(at + 2..at + 4)?.try_into().ok()?) as usize;
        if length < 2 {
            return None;
        }
        // SOF0–SOF15（不含 DHT=C4、JPG=C8、DAC=CC）后面就是尺寸。
        let is_sof =
            (0xc0..=0xcf).contains(&marker) && marker != 0xc4 && marker != 0xc8 && marker != 0xcc;
        if is_sof {
            let height = u16::from_be_bytes(bytes.get(at + 5..at + 7)?.try_into().ok()?) as u32;
            let width = u16::from_be_bytes(bytes.get(at + 7..at + 9)?.try_into().ok()?) as u32;
            return Some((width, height));
        }
        at += 2 + length;
    }
    None
}

fn gif_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"GIF87a") && !bytes.starts_with(b"GIF89a") {
        return None;
    }
    Some((u32::from(le_u16(bytes, 6)?), u32::from(le_u16(bytes, 8)?)))
}

fn bmp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if !bytes.starts_with(b"BM") {
        return None;
    }
    let header = le_u32(bytes, 14)?;
    if header == 12 {
        // BITMAPCOREHEADER：16 位宽高。
        return Some((u32::from(le_u16(bytes, 18)?), u32::from(le_u16(bytes, 20)?)));
    }
    let width = le_u32(bytes, 18)? as i32;
    let height = le_u32(bytes, 22)? as i32;
    if width <= 0 || height == 0 {
        return None;
    }
    Some((width as u32, height.unsigned_abs()))
}

/// 字节是否看起来是一种已知的图片（用于区分「容器格式无法解析尺寸」与「根本不是图片」）。
pub fn looks_like_image(bytes: &[u8]) -> bool {
    const MAGICS: [&[u8]; 6] = [
        &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a],
        &[0xff, 0xd8, 0xff],
        b"GIF87a",
        b"GIF89a",
        b"BM",
        b"RIFF",
    ];
    MAGICS.iter().any(|magic| bytes.starts_with(magic))
}

/// 校验并规范化从剪贴板读到的图片。
///
/// 超过 [`MAX_IMAGE_BYTES`]、内容为空、根本无法识别或**头部已损坏**的字节都返回中文
/// 原因；调用方把原因放进 [`ClipboardCapture::image_problem`]，宿主据此给出「这次复制
/// 没有被保存」的准确状态，而不会生成一条恢复不了的条目。无法解析出尺寸的图片一律
/// 拒绝：尺寸是列表与预览都要用的信息，宁可如实报错也不保存一份读不出来的数据。
pub fn check_image(mime: &str, bytes: Vec<u8>) -> Result<ClipboardImage, String> {
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(format!(
            "图片 {:.1} MB 超过上限 {} MB，这次复制没有被保存",
            bytes.len() as f64 / (1024.0 * 1024.0),
            MAX_IMAGE_BYTES / (1024 * 1024)
        ));
    }
    if bytes.is_empty() {
        return Err("剪贴板报告的图片内容为空，这次复制没有被保存".to_string());
    }
    if image_dimensions(&bytes).is_none() {
        return Err(if looks_like_image(&bytes) {
            "剪贴板里的图片头部不完整或已损坏，这次复制没有被保存".to_string()
        } else {
            "剪贴板里的图片格式无法识别（本版本支持 PNG / JPEG / GIF / BMP），这次复制没有被保存"
                .to_string()
        });
    }
    Ok(ClipboardImage::new(mime, bytes))
}

/// 把 RGBA8 像素编码为 PNG 字节。
///
/// 复用 Windows 图标编码同一条路径（`windows::icons::encode_png`）：它是纯逻辑，
/// 在所有目标上编译并已被真实像素的往返测试覆盖。
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    crate::windows::icons::encode_png(width, height, rgba)
}

/// 生成缩略图 PNG（长边不超过 `max_edge`，不放大）。
///
/// 结果列表里的每一条图片历史都要有缩略图；把 16 MB 的原图直接编码成 data URL 会让
/// 一次查询返回几十 MB 的字符串。缩放与编码是纯逻辑，与操作系统无关，因此放在这里
/// 由本地测试覆盖，而不是留给 UI 或某一个平台。
pub fn png_thumbnail(png: &[u8], max_edge: u32) -> Result<Vec<u8>, String> {
    if max_edge == 0 {
        return Err("缩略图边长必须大于 0".to_string());
    }
    let decoded = image::load_from_memory(png).map_err(|error| format!("无法解码图片：{error}"))?;
    let thumbnail = decoded.thumbnail(max_edge, max_edge).to_rgba8();
    let (width, height) = thumbnail.dimensions();
    encode_png(width, height, thumbnail.as_raw())
}

/// 把 `CF_DIB` 字节（Windows 剪贴板的位图格式，没有 `BITMAPFILEHEADER`）转换为 PNG。
///
/// 支持 `BITMAPINFOHEADER` / `BITMAPV4HEADER` / `BITMAPV5HEADER` 的 24 位与 32 位
/// BI_RGB / BI_BITFIELDS；其余（调色板位图、RLE 压缩等）返回中文原因，由调用方如实
/// 上报「无法恢复的格式」，而不是保存一个空的图片条目。
///
/// 这是**纯逻辑**：`tests/windows_fixture.rs` 在 Linux 上用真实合成的 DIB 验证它，
/// 因此这条 Windows 路径不是「只在 Windows 上才第一次运行」的代码。
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
    let header_size = le_u32(dib, 0).ok_or_else(|| "位图头不完整".to_string())? as usize;
    if header_size < 40 {
        return Err(format!(
            "不支持的位图头（{header_size} 字节，缺少 BITMAPINFOHEADER）"
        ));
    }
    let width = le_u32(dib, 4).ok_or_else(|| "位图头不完整".to_string())? as i32;
    let height = le_u32(dib, 8).ok_or_else(|| "位图头不完整".to_string())? as i32;
    let bit_count = le_u16(dib, 14).ok_or_else(|| "位图头不完整".to_string())?;
    let compression = le_u32(dib, 16).ok_or_else(|| "位图头不完整".to_string())?;
    if width <= 0 || height == 0 {
        return Err(format!("位图尺寸无效：{width}×{height}"));
    }
    if bit_count != 24 && bit_count != 32 {
        return Err(format!("不支持的位深：{bit_count} 位（只支持 24 / 32 位）"));
    }
    if compression != 0 && compression != 3 {
        return Err(format!("不支持的位图压缩方式：{compression}"));
    }
    let width = width as usize;
    let height = height.unsigned_abs() as usize;
    let top_down = (le_u32(dib, 8).unwrap_or(0) as i32) < 0;
    let stride = ((width * usize::from(bit_count) + 31) / 32) * 4;
    // 像素起点：位图头之后是颜色表或（BI_BITFIELDS 的）掩码。
    // `BITMAPINFOHEADER` + BI_BITFIELDS 时掩码在头**之后**（本仓库写 4 个，共 16 字节）；
    // V4/V5 头的掩码包含在头内。若 `biSizeImage` 与实际数据长度自洽，则直接从尾部反推，
    // 这样对只写 3 个掩码的其它实现也成立。
    let mut pixels_at = header_size;
    if compression == 3 && header_size == 40 {
        pixels_at += 16;
    }
    let size_image = le_u32(dib, 20).unwrap_or(0) as usize;
    let expected_pixels = stride.saturating_mul(height);
    if size_image == expected_pixels && dib.len() >= header_size + size_image {
        pixels_at = dib.len() - size_image;
    }
    let needed = pixels_at
        .checked_add(expected_pixels)
        .ok_or_else(|| "位图尺寸溢出".to_string())?;
    if dib.len() < needed {
        return Err(format!(
            "位图数据不完整：需要 {needed} 字节，实际 {} 字节",
            dib.len()
        ));
    }
    let mut rgba = vec![0u8; width * height * 4];
    for row in 0..height {
        // 正的高度表示自下而上存储。
        let source_row = if top_down { row } else { height - 1 - row };
        let start = pixels_at + source_row * stride;
        for column in 0..width {
            let at = start + column * usize::from(bit_count) / 8;
            let (blue, green, red) = (dib[at], dib[at + 1], dib[at + 2]);
            let alpha = if bit_count == 32 { dib[at + 3] } else { 255 };
            let target = (row * width + column) * 4;
            rgba[target] = red;
            rgba[target + 1] = green;
            rgba[target + 2] = blue;
            // BI_RGB 的 32 位位图第 4 字节常常是 0（未定义），此时按不透明处理，
            // 否则整张图会变成全透明。
            rgba[target + 3] = if bit_count == 32 && compression == 0 {
                255
            } else {
                alpha
            };
        }
    }
    encode_png(width as u32, height as u32, &rgba)
}

/// 把 PNG 字节转换为 `CF_DIB`（供 Windows 写入剪贴板，让只认位图的旧应用也能粘贴）。
///
/// 写的是 32 位 BI_BITFIELDS + 四个颜色掩码：这样透明度也能原样带回（BI_RGB 的 32 位
/// 位图第 4 字节按规范是未定义的，写进去会被多数应用忽略）。
pub fn png_to_dib(png: &[u8]) -> Result<Vec<u8>, String> {
    let decoded = image::load_from_memory(png)
        .map_err(|error| format!("无法解码 PNG：{error}"))?
        .to_rgba8();
    let (width, height) = (decoded.width(), decoded.height());
    if width == 0 || height == 0 {
        return Err("图片尺寸为 0".to_string());
    }
    let stride = width as usize * 4;
    let mut out = Vec::with_capacity(56 + stride * height as usize);
    out.extend_from_slice(&40u32.to_le_bytes());
    out.extend_from_slice(&(width as i32).to_le_bytes());
    out.extend_from_slice(&(height as i32).to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&32u16.to_le_bytes());
    // BI_BITFIELDS：随后的 16 字节是 ARGB 掩码。
    out.extend_from_slice(&3u32.to_le_bytes());
    out.extend_from_slice(&(stride as u32 * height).to_le_bytes());
    out.extend_from_slice(&[0u8; 16]);
    out.extend_from_slice(&0x00ff_0000u32.to_le_bytes());
    out.extend_from_slice(&0x0000_ff00u32.to_le_bytes());
    out.extend_from_slice(&0x0000_00ffu32.to_le_bytes());
    out.extend_from_slice(&0xff00_0000u32.to_le_bytes());
    // 自下而上、BGRA（负高度表示自上而下，这里用正高度 + 反序行）。
    for row in (0..height).rev() {
        for column in 0..width {
            let pixel = decoded.get_pixel(column, row).0;
            out.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
    Ok(out)
}

/// 轮询到的一次新复制事件。
///
/// 一次复制事件的**全部**格式在同一个结构体里：纯文本与它受支持的富文本格式必须一起
/// 进入同一条历史，不允许按格式拆成多条记录（那样历史里会出现互相重复的条目）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCapture {
    /// 本次事件里存在的格式集合。
    pub formats: Vec<ClipboardFormatKind>,
    /// 文本内容（可索引的检索文字；富文本载荷**不**参与检索）。ticket 10–12 在同一
    /// 结构体上增加了图片与文件字段。
    pub text: Option<String>,
    /// 图片内容（ticket 10）。可能与文字同时存在（例如浏览器同时给出图片与网址）。
    pub image: Option<ClipboardImage>,
    /// 剪贴板里**有**图片，但无法保存的中文原因（超大、格式无法识别、解码失败）。
    ///
    /// 与 `image: None` + 没有原因（剪贴板里本来就没有图片）是两回事：前者必须让用户
    /// 看到「这次复制没有被保存」，不能静默跳过。
    pub image_problem: Option<String>,
    /// HTML 载荷（平台公开的 `text/html`）。没有时如实为 `None`。
    pub html: Option<String>,
    /// RTF 载荷（平台公开的 `text/rtf` 或 Windows 的 `Rich Text Format`）。
    pub rtf: Option<String>,
    /// 来源应用（平台可提供时）。
    pub source: Option<ClipboardSourceApp>,
}

impl ClipboardCapture {
    /// 只有文本的一次复制事件。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Text],
            text: Some(text.into()),
            image: None,
            image_problem: None,
            html: None,
            rtf: None,
            source: None,
        }
    }

    /// 只有图片的一次复制事件。
    pub fn image(image: ClipboardImage) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Image],
            text: None,
            image: Some(image),
            image_problem: None,
            html: None,
            rtf: None,
            source: None,
        }
    }

    /// 剪贴板里有图片但无法保存：只有原因，没有内容。
    pub fn image_failed(reason: impl Into<String>) -> Self {
        Self {
            formats: Vec::new(),
            text: None,
            image: None,
            image_problem: Some(reason.into()),
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
            image: None,
            image_problem: None,
            html,
            rtf,
            source: None,
        }
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

/// 图片字节的变化指纹（ticket 10）。
///
/// 与 [`fingerprint`] 同源（`DefaultHasher`，只用来判断「和上次读到的是不是同一份
/// 内容」）：图片没有可读的文本，因此按字节指纹判断变化。
pub fn fingerprint_bytes(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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

/// 校验待写入剪贴板的图片。
///
/// 与 [`check_text`] 对齐：空内容与超限都在**写入之前**拒绝，并把中文原因带回宿主，
/// 而不是让工具去失败。
pub fn check_image_write(image: &ClipboardImage) -> Result<(), ClipboardError> {
    if image.bytes.is_empty() {
        return Err(ClipboardError::Failed(
            "图片内容为空，没有可复制的东西".to_string(),
        ));
    }
    if image.bytes.len() > MAX_IMAGE_BYTES {
        return Err(ClipboardError::Failed(format!(
            "图片超过 {} MB，已拒绝写入剪贴板",
            MAX_IMAGE_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// 用一个外部剪贴板工具写入**二进制**内容：内容走标准输入。
///
/// 与 [`write_with_tool`] 同一套有界等待与标准错误处理（工具 fork 出守护进程后会一直
/// 持有管道，因此不能同步读到 EOF）。
pub(crate) fn write_bytes_with_tool(
    program: &Path,
    args: &[&str],
    bytes: &[u8],
) -> Result<(), ClipboardError> {
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
            .write_all(bytes)
            .map_err(|error| ClipboardError::Failed(format!("写入剪贴板失败：{error}")))?;
    }
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

/// 从一个外部剪贴板工具读取**二进制**内容（ticket 10 的图片）。
///
/// 与 [`read_with_tool`] 同样有界，并且额外限制**读入内存的字节数**：图片可能是
/// 几十 MB，`read_to_end` 会先把它们全部读进来才轮到容量判断。这里最多读
/// `max_bytes + 1` 字节——多读的那 1 字节用来区分「刚好等于上限」与「超过上限」，
/// 后者由 [`check_image`] 给出面向用户的原因。
pub(crate) fn read_bytes_bounded(
    program: &Path,
    args: &[&str],
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, ClipboardError> {
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
    let limit = max_bytes as u64 + 1;
    let reader = std::thread::spawn(move || {
        use std::io::Read as _;
        let mut buffer = Vec::new();
        if let Some(stdout) = stdout {
            let _ = stdout.take(limit).read_to_end(&mut buffer);
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
    if !status.success() || bytes.is_empty() {
        // 非 0 退出通常表示「当前剪贴板里没有这种格式」：这是正常状态，不是错误。
        return Ok(None);
    }
    Ok(Some(bytes))
}

/// 有界地读取一个文件（macOS 用 `osascript` 把图片写到临时文件后再读它）。
pub(crate) fn read_file_bounded(
    path: &Path,
    max_bytes: u64,
) -> Result<Option<Vec<u8>>, ClipboardError> {
    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            if metadata.len() > max_bytes {
                return Err(ClipboardError::ReadFailed(format!(
                    "剪贴板导出的图片 {:.1} MB 超过上限 {} MB",
                    metadata.len() as f64 / (1024.0 * 1024.0),
                    max_bytes / (1024 * 1024)
                )));
            }
        }
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ClipboardError::ReadFailed(format!(
                "无法读取 {}：{error}",
                path.display()
            )))
        }
    }
    match std::fs::read(path) {
        Ok(bytes) if bytes.is_empty() => Ok(None),
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) => Err(ClipboardError::ReadFailed(format!(
            "无法读取 {}：{error}",
            path.display()
        ))),
    }
}

/// 运行一个外部工具并取回它的标准输出与退出状态（有界）。
///
/// macOS 的图片剪贴板只能经 `osascript` 完成，而它的脚本参数不是「往标准输入写」的
/// 形态，因此需要这一条与 [`read_with_tool`] 分开的路径。等待同样是有界的：拿不到
/// 剪贴板环境时不能让后台轮询线程卡死。
pub(crate) fn run_tool(program: &Path, args: &[&str]) -> Result<(bool, String), ClipboardError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ClipboardError::ReadFailed(format!("无法启动 {}：{error}", program.display()))
        })?;
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        if let Some(mut stdout) = stdout {
            let _ = std::io::Read::read_to_string(&mut stdout, &mut out);
        }
        let mut err = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut err);
        }
        (out, err)
    });
    let deadline = Instant::now() + READ_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(ClipboardError::ReadFailed(format!(
                    "{} 超过 {} 秒没有返回，已中止",
                    program.display(),
                    READ_TIMEOUT.as_secs()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(ClipboardError::ReadFailed(format!(
                    "等待 {} 失败：{error}",
                    program.display()
                )));
            }
        }
    };
    let (out, err) = reader.join().unwrap_or_default();
    if status.success() {
        Ok((true, out))
    } else {
        let message = if err.trim().is_empty() { out } else { err };
        Ok((false, message))
    }
}

/// 原始字节 → 「图片」或「无法保存的中文原因」。
///
/// 三个平台的读取实现共用这一对语义：`None` 表示剪贴板里没有图片，
/// `Some(Err(reason))` 表示有图片但不能保存（超大、格式无法识别）。
pub fn image_from_bytes(
    mime: &str,
    bytes: Option<Vec<u8>>,
) -> (Option<ClipboardImage>, Option<String>) {
    match bytes {
        None => (None, None),
        Some(bytes) => match check_image(mime, bytes) {
            Ok(image) => (Some(image), None),
            Err(reason) => (None, Some(reason)),
        },
    }
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
}

/// 图片相关的**纯逻辑**测试（ticket 10）。
///
/// 不加 `unix` 限制：尺寸解析、上限判断与 DIB 转换都不依赖操作系统，因此 Windows
/// runner 上的 `cargo test` 也会跑它们；`dib_to_png` 是 Windows CI 之外**唯一**在
/// 提交前验证 Windows 位图路径的机会。
#[cfg(test)]
mod image_tests {
    use super::*;
    // `DynamicImage` 的 `dimensions()` 来自这个 trait，不在 prelude 里。
    use image::GenericImageView;

    fn png_of(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
        encode_png(width, height, rgba).expect("编码 PNG")
    }

    /// 尺寸从文件头解析：PNG 走真实编码，JPEG / GIF / BMP 用手写的头部。
    #[test]
    fn dimensions_come_from_the_file_header() {
        let png = png_of(3, 2, &vec![0u8; 3 * 2 * 4]);
        assert_eq!(image_dimensions(&png), Some((3, 2)));

        // SOI + APP0(len 4) + SOF0(len 17, 精度 8, 高 7, 宽 5) + EOI
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00,
            0x07, 0x00, 0x05, 0x03, 0x01, 0x11, 0x00, 0xff, 0xd9,
        ];
        assert_eq!(image_dimensions(&jpeg), Some((5, 7)));

        let mut gif = b"GIF89a".to_vec();
        gif.extend_from_slice(&9u16.to_le_bytes());
        gif.extend_from_slice(&4u16.to_le_bytes());
        assert_eq!(image_dimensions(&gif), Some((9, 4)));

        // BITMAPINFOHEADER：宽 6、高 -3（自上而下）。
        let mut bmp = b"BM".to_vec();
        bmp.extend_from_slice(&[0u8; 12]);
        bmp.extend_from_slice(&40u32.to_le_bytes());
        bmp.extend_from_slice(&6i32.to_le_bytes());
        bmp.extend_from_slice(&(-3i32).to_le_bytes());
        assert_eq!(image_dimensions(&bmp), Some((6, 3)));

        // 认不出来的字节不猜尺寸。
        assert_eq!(image_dimensions(b"not an image"), None);
    }

    /// 超过上限的图片**不保存**，并给出面向用户的中文原因。
    #[test]
    fn oversize_images_are_rejected_with_a_reason() {
        let too_big = vec![0u8; MAX_IMAGE_BYTES + 1];
        let error = check_image(IMAGE_MIME_PNG, too_big).expect_err("超过上限必须被拒绝");
        assert!(error.contains("超过上限"), "原因要说明超限：{error}");

        let (image, problem) = image_from_bytes(
            IMAGE_MIME_PNG,
            Some(vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]),
        );
        assert!(image.is_none());
        // 完整 PNG 魔数但没有 IHDR：容器认得出、内容读不出，必须如实报「已损坏」，
        // 而不是保存一条永远显示不出来的图片。
        assert!(
            problem.unwrap_or_default().contains("损坏"),
            "头部不完整的图片要如实说明"
        );

        let (image, problem) = image_from_bytes(IMAGE_MIME_PNG, Some(b"garbage".to_vec()));
        assert!(image.is_none());
        assert!(
            problem.unwrap_or_default().contains("无法识别"),
            "无法识别的字节要如实说明"
        );

        // 没有图片与「有图片但坏了」必须能区分。
        let (image, problem) = image_from_bytes(IMAGE_MIME_PNG, None);
        assert!(image.is_none() && problem.is_none());
    }

    /// Windows 的 `CF_DIB` 往返：`png → dib → png` 后像素不变。
    #[test]
    fn dib_round_trip_preserves_pixels() {
        let (width, height) = (4u32, 3u32);
        let mut rgba = Vec::new();
        for row in 0..height {
            for column in 0..width {
                rgba.extend_from_slice(&[
                    (column * 60) as u8,
                    (row * 70) as u8,
                    128,
                    if (column + row) % 2 == 0 { 255 } else { 200 },
                ]);
            }
        }
        let png = png_of(width, height, &rgba);
        let dib = png_to_dib(&png).expect("PNG → DIB");
        let back = dib_to_png(&dib).expect("DIB → PNG");
        let decoded = image::load_from_memory(&back).expect("回读").to_rgba8();
        assert_eq!(decoded.dimensions(), (width, height));
        assert_eq!(decoded.into_raw(), rgba, "往返后像素必须逐字节一致");
    }

    /// 自上而下的 DIB（负高度）也要还原成正确的行序。
    #[test]
    fn top_down_dib_rows_are_reversed() {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes());
        dib.extend_from_slice(&2i32.to_le_bytes());
        // 负高度：自上而下。
        dib.extend_from_slice(&(-2i32).to_le_bytes());
        dib.extend_from_slice(&1u16.to_le_bytes());
        dib.extend_from_slice(&24u16.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&0u32.to_le_bytes());
        dib.extend_from_slice(&[0u8; 16]);
        // 第一行全红、第二行全蓝（BGR 顺序、每行补到 4 字节）。
        dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0]);
        dib.extend_from_slice(&[255, 0, 0, 255, 0, 0, 255, 0]);
        let png = dib_to_png(&dib).expect("24 位 DIB");
        let decoded = image::load_from_memory(&png).expect("回读").to_rgba8();
        assert_eq!(decoded.get_pixel(0, 0).0, [255, 0, 0, 255]);
        assert_eq!(decoded.get_pixel(0, 1).0, [0, 0, 255, 255]);
    }

    /// 无法恢复的位图格式必须给出准确原因，不能悄悄产出空图片。
    #[test]
    fn unsupported_dib_formats_report_a_reason() {
        let mut palette = Vec::new();
        palette.extend_from_slice(&40u32.to_le_bytes());
        palette.extend_from_slice(&2i32.to_le_bytes());
        palette.extend_from_slice(&2i32.to_le_bytes());
        palette.extend_from_slice(&1u16.to_le_bytes());
        palette.extend_from_slice(&8u16.to_le_bytes());
        palette.extend_from_slice(&0u32.to_le_bytes());
        palette.extend_from_slice(&0u32.to_le_bytes());
        palette.extend_from_slice(&[0u8; 16]);
        let error = dib_to_png(&palette).expect_err("8 位调色板位图不支持");
        assert!(error.contains("位深"), "原因要说清位深：{error}");

        let truncated = dib_to_png(&[0u8; 20]).expect_err("头部不完整必须失败");
        assert!(
            truncated.contains("位图头"),
            "原因要指出头部问题：{truncated}"
        );
    }

    /// 图片字节的变化指纹只反映内容：同样的字节得到同样的指纹，不同字节不同。
    #[test]
    fn byte_fingerprint_tracks_content() {
        assert_eq!(fingerprint_bytes(b"abc"), fingerprint_bytes(b"abc"));
        assert_ne!(fingerprint_bytes(b"abc"), fingerprint_bytes(b"abd"));
    }

    /// 缩略图：长边收到上限、不放大、仍然是可解码的 PNG。
    #[test]
    fn thumbnails_shrink_but_never_grow() {
        let big = png_of(400, 200, &vec![7u8; 400 * 200 * 4]);
        let thumb = png_thumbnail(&big, 96).expect("生成缩略图");
        let decoded = image::load_from_memory(&thumb).expect("缩略图必须可解码");
        assert_eq!(decoded.dimensions(), (96, 48));
        assert!(thumb.len() < big.len(), "缩略图必须更小");

        // 原图比上限还小：保持原尺寸，不放大。
        let small = png_of(12, 8, &vec![9u8; 12 * 8 * 4]);
        let decoded =
            image::load_from_memory(&png_thumbnail(&small, 96).expect("缩略图")).expect("可解码");
        assert_eq!(decoded.dimensions(), (12, 8));

        // 不是 PNG 的字节如实失败，不产出半张图。
        assert!(png_thumbnail(b"not a png", 96).is_err());
        assert!(png_thumbnail(&small, 0).is_err());
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
