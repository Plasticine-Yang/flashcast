//! 剪贴板历史：一次复制事件的数据模型与本机 SQLite 存储（ADR §8，spec「剪贴板」）。
//!
//! ## 数据模型
//!
//! 一次复制事件 = 一行 [`ClipboardEvent`]，外加三张从表：
//!
//! ```text
//! clipboard_events       一次复制事件：稳定 id、捕获时间、去重键、摘要、可索引文字、
//!                        来源应用、置顶标记与重复次数
//! clipboard_formats      本次事件里存在的格式集合（text / html / rtf / image / files）
//! clipboard_payloads     需要原样保留的载荷（html / rtf），role 是字符串标签
//! clipboard_attachments  附件引用：图片、已保存的文件副本、原文件引用
//! ```
//!
//! 这张 schema 从 ticket 09 起就是**完整的**：图片（ticket 10）、HTML/RTF（ticket 11）
//! 与文件列表（ticket 12）只增加行，不增加表或列，因此**不需要迁移**。为此：
//!
//! - 格式、载荷角色与附件种类在存储里都是字符串标签，读写两侧对未知取值有
//!   [`ClipboardFormat::Other`] / [`PayloadRole::Other`] / [`AttachmentKind::Other`]
//!   这样的降级表示，不会丢数据；
//! - 图片尺寸、文件数量这类随种类而异的元数据放在通用列（`width` / `height` /
//!   `item_count` / `names`）里，没有值的种类留空；
//! - `text_content` 是唯一被检索的文字列（spec「剪贴板支持文字搜索……不承诺 OCR」）。
//!
//! ## 为什么在应用数据目录
//!
//! 剪贴板历史是本机数据，**绝不进入配置工作区**（spec 用户故事 30）：数据库与附件都在
//! 设备本地目录（`<应用数据目录>/clipboard/`）下，工作区里只有可迁移的偏好
//! （保留期限、容量、暂停开关，见 [`crate::settings`]）。
//!
//! ## 富文本（ticket 11）
//!
//! - **一次复制事件一条历史**：同一次复制的纯文本与受支持的 HTML/RTF 一起落进一行
//!   `clipboard_events` 加多行 `clipboard_formats` / `clipboard_payloads`，不按格式拆分，
//!   因此历史里不会出现互相重复的条目。HTML/RTF 属于本机 SQLite（`inline_text`），
//!   **不需要** schema 迁移（ticket 09 已备好这两张表）。
//! - **检索只用可索引文本**：`text_content` 是唯一被检索的内容列，HTML/RTF 载荷
//!   **不参与**匹配（[`ClipboardStore::list`] 与插件的元数据都不带载荷原文）。
//! - **去重语义不变**：`content_hash` 始终是纯文本指纹（[`content_hash_text`]），
//!   因此纯文本条目的去重与 ticket 09 完全一致，不会因为多带了富文本就变成另一条。
//!   去重命中时若已有条目**还没有**任何载荷，后一次复制带来的 HTML/RTF 会被补进这条
//!   已有条目（先到的富文本版本不会被覆盖），刚捕获到的格式因此不会白丢。
//! - **恢复走平台公开格式**：宿主把同一次事件的文本 + HTML/RTF 一起写回剪贴板
//!   （[`crate::Host::execute`] 的剪贴板分支），由目标应用自己挑；平台做不到时如实
//!   报告哪些格式没有提供（见 `flashcast_platform::clipboard::ClipboardWriteReport`），
//!   从不声称保留了任意应用私有格式。
//! - **预览是惰性文本**：预览只取 [`ClipboardEvent::text`]（由用户可读的纯文本构成，
//!   见 [`crate::plugins::clipboard::clipboard_item`]），HTML/RTF 原文既不进 webview，
//!   也不渲染成标记。webview 只用 React 文本节点渲染它，因此剪贴板提供的
//!   `<script>` / `onerror` / 远端资源不会被解析或加载——不需要「先消毒再插入 HTML」
//!   那种容易被绕过的路径。

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flashcast_platform::clipboard::{
    ClipboardCapture, ClipboardFileEntry, ClipboardFormatKind, ClipboardImage, ClipboardSourceApp,
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

/// 剪贴板历史插件在清单里的标识。
pub const CLIPBOARD_PLUGIN_ID: &str = "clipboard";

/// 结果条目标识前缀：`clipboard:<事件 id>`。
pub const CLIPBOARD_ITEM_PREFIX: &str = "clipboard:";

/// 设备本地目录下存放剪贴板历史的子目录。
pub const CLIPBOARD_DIR: &str = "clipboard";

/// SQLite 数据库文件名。
pub const CLIPBOARD_DB_FILE: &str = "history.sqlite3";

/// 附件（图片、已保存的文件副本）的存放目录名。
pub const ATTACHMENTS_DIR: &str = "attachments";

/// 当前 schema 版本。tickets 10–12 只增加行，因此这个值不需要变。
pub const SCHEMA_VERSION: i64 = 1;

/// 摘要最多显示多少个字符。
pub const SUMMARY_MAX_CHARS: usize = 60;

/// 一次复制事件里存在的格式。
///
/// 这是**事件级**的格式集合，带有该格式可用的元数据；具体内容存在 `text`、
/// `payloads` 或 `attachments` 里。新增格式时在存储中只是多一种 `kind` 标签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ClipboardFormat {
    Text {
        bytes: usize,
    },
    Html {
        bytes: usize,
    },
    Rtf {
        bytes: usize,
    },
    Image {
        mime: String,
        width: u32,
        height: u32,
        bytes: usize,
    },
    Files {
        count: usize,
        names: Vec<String>,
    },
    /// 存储里出现但本版本不认识的格式标签（前向兼容，不丢数据）。
    ///
    /// 字段名不能与 `#[serde(tag = "kind")]` 的标签同名，否则 serde 的内部标记
    /// 枚举会直接编译失败。
    Other {
        name: String,
        bytes: usize,
    },
}

impl ClipboardFormat {
    /// 存储里的稳定标签。
    pub fn tag(&self) -> &str {
        match self {
            ClipboardFormat::Text { .. } => "text",
            ClipboardFormat::Html { .. } => "html",
            ClipboardFormat::Rtf { .. } => "rtf",
            ClipboardFormat::Image { .. } => "image",
            ClipboardFormat::Files { .. } => "files",
            ClipboardFormat::Other { name, .. } => name,
        }
    }

    /// 该格式的字节数（无法得知时为 0）。
    pub fn bytes(&self) -> usize {
        match self {
            ClipboardFormat::Text { bytes }
            | ClipboardFormat::Html { bytes }
            | ClipboardFormat::Rtf { bytes }
            | ClipboardFormat::Image { bytes, .. }
            | ClipboardFormat::Other { bytes, .. } => *bytes,
            ClipboardFormat::Files { .. } => 0,
        }
    }

    /// 面向用户的格式名，用于结果副标题。
    pub fn label_zh(&self) -> &'static str {
        match self {
            ClipboardFormat::Text { .. } => "文字",
            ClipboardFormat::Html { .. } => "HTML",
            ClipboardFormat::Rtf { .. } => "RTF",
            ClipboardFormat::Image { .. } => "图片",
            ClipboardFormat::Files { .. } => "文件",
            ClipboardFormat::Other { .. } => "其它格式",
        }
    }
}

/// 由平台捕获结果构造一次复制事件。
///
/// 返回 `None` 表示这次变化里没有任何本版本能保存的内容（既没有文字，也没有文件）。
/// **不**返回一个空条目：那会让历史出现「点开什么都没有」的条目。
///
/// 图片（ticket 10）在这里映射成 [`ClipboardFormat::Image`] 加一条
/// [`ClipboardAttachment`]：附件路径落在 `attachments_dir` 下，字节由调用方
/// （[`ClipboardRuntime`]）在入库成功后写进去——先生成条目再发现写不进去，就会留下
/// 一条「看起来保存了、其实恢复不了」的历史。
///
/// HTML/RTF（ticket 11）作为同一事件里的载荷进入 `payloads`：一次复制只有一条历史。
///
/// ## 文件列表（ticket 12）走同一个入口
///
/// - 每个文件生成一个 [`AttachmentKind::FileReference`] 附件，`depends_on_source` 为
///   `true`——它依赖原文件，原文件消失后不可恢复；
/// - 图片（ticket 10）生成一个 [`AttachmentKind::Image`] 附件，字节落在
///   `attachments_dir` 下、`depends_on_source` 为 `false`（本机副本，原来源消失后仍可恢复）；
/// - 有文件时摘要与去重键都按**文件列表**计算（`files:` 前缀），与文字事件的 `text:`
///   键不会互相误判（spec「视频按文件处理」「去重不跨类型误判」）；
/// - 视频文件与其它文件没有任何差别。
///
/// 图片与文字的去重键在两者之间**不共通**：文字用文本指纹、图片用字节指纹，
/// 因此两种内容不可能互相误判成同一条；同时带回文字与图片时按文字处理。
///
/// ## 去重键不因富文本而改变
///
/// 只有文字的事件，[`ClipboardEvent::content_hash`] 始终是**纯文本**的指纹
/// （[`content_hash_text`]）。因此同一次复制先后以「纯文本」和「文本 + HTML」出现时
/// 仍然合并到同一条（纯文本条目的去重语义完全不变），而富文本载荷永远不会变成检索键。
///
/// `capture.image_problem`（有图片但无法保存）不在这里处理：它没有可落库的内容，
/// 由捕获管线转成一次如实的失败。
pub fn event_from_capture(
    capture: &ClipboardCapture,
    now_ms: i64,
    attachments_dir: &Path,
) -> Option<ClipboardEvent> {
    // 空字符串不算文字内容：剪贴板里常常同时有「空文本目标」与图片。
    let text = capture.text.clone().filter(|text| !text.is_empty());
    let image = capture.image.as_ref();
    let html = capture.html.clone().filter(|html| !html.is_empty());
    let rtf = capture.rtf.clone().filter(|rtf| !rtf.is_empty());
    let files: Vec<ClipboardFileEntry> = capture
        .files
        .iter()
        .filter(|file| !file.path.as_os_str().is_empty())
        .cloned()
        .collect();
    if text.is_none() && image.is_none() && files.is_empty() {
        return None;
    }

    // 只记录**真的有内容**的格式：格式集合与载荷必须一致，否则会出现「声称有 RTF、
    // 点开却没有」的条目。
    let mut formats: Vec<ClipboardFormat> = Vec::new();
    let mut push = |format: ClipboardFormat| {
        if !formats
            .iter()
            .any(|existing| existing.tag() == format.tag())
        {
            formats.push(format);
        }
    };
    if let Some(text) = &text {
        push(ClipboardFormat::Text { bytes: text.len() });
    }
    if let Some(image) = image {
        push(ClipboardFormat::Image {
            mime: image.mime.clone(),
            width: image.width,
            height: image.height,
            bytes: image.bytes.len(),
        });
    }
    for kind in &capture.formats {
        // 文字、图片与文件列表的元数据以真实内容为准，已经在上面加过。
        let format = match kind {
            ClipboardFormatKind::Text | ClipboardFormatKind::Image => continue,
            ClipboardFormatKind::Html => {
                let Some(html) = html.as_deref() else {
                    continue;
                };
                ClipboardFormat::Html { bytes: html.len() }
            }
            ClipboardFormatKind::Rtf => {
                let Some(rtf) = rtf.as_deref() else {
                    continue;
                };
                ClipboardFormat::Rtf { bytes: rtf.len() }
            }
            ClipboardFormatKind::Files => {
                if files.is_empty() {
                    continue;
                }
                ClipboardFormat::Files {
                    count: files.len(),
                    names: files.iter().map(|file| file.name.clone()).collect(),
                }
            }
        };
        push(format);
    }
    // 平台没有在 `formats` 里列出、但确实带回了载荷的格式同样要记上（宁可多记，
    // 不可让「保存了却没有格式标记」的内容在界面上隐身）；反过来，声明了格式却没有
    // 条目/载荷的也不记——两种都由实际内容说了算。
    if let Some(html) = html.as_deref() {
        push(ClipboardFormat::Html { bytes: html.len() });
    }
    if let Some(rtf) = rtf.as_deref() {
        push(ClipboardFormat::Rtf { bytes: rtf.len() });
    }
    if !files.is_empty() {
        push(ClipboardFormat::Files {
            count: files.len(),
            names: files.iter().map(|file| file.name.clone()).collect(),
        });
    }
    if formats.is_empty() {
        let text = text.as_ref()?;
        formats.push(ClipboardFormat::Text { bytes: text.len() });
    }

    let mut attachments: Vec<ClipboardAttachment> = Vec::new();
    if let Some(image) = image {
        let id = new_attachment_id();
        // 附件是本机副本（`depends_on_source = false`）：原来源消失后仍可恢复
        // （spec「图片及已保存文件副本在原来源消失后仍可恢复」）。
        attachments.push(ClipboardAttachment {
            path: attachments_dir.join(format!("{id}.png")),
            name: attachment_name(image),
            mime: Some(image.mime.clone()),
            bytes: image.bytes.len() as u64,
            depends_on_source: false,
            created_at_ms: now_ms,
            kind: AttachmentKind::Image,
            id,
        });
    }
    // 文件引用（`depends_on_source = true`）：原文件消失后不可恢复，界面据此提示。
    attachments.extend(files.iter().map(|file| ClipboardAttachment {
        id: new_attachment_id(),
        kind: AttachmentKind::FileReference,
        path: file.path.clone(),
        name: file.name.clone(),
        mime: file.mime.clone(),
        bytes: file.bytes.unwrap_or(0),
        depends_on_source: true,
        created_at_ms: now_ms,
    }));

    let payloads: Vec<ClipboardPayload> = html
        .clone()
        .map(|html| ClipboardPayload {
            role: PayloadRole::Html,
            bytes: html.len(),
            inline: Some(html),
            attachment_id: None,
            mime: Some("text/html".to_string()),
        })
        .into_iter()
        .chain(rtf.clone().map(|rtf| ClipboardPayload {
            role: PayloadRole::Rtf,
            bytes: rtf.len(),
            inline: Some(rtf),
            attachment_id: None,
            mime: Some("text/rtf".to_string()),
        }))
        .collect();

    // 去重键：有文件时按文件列表（`files:` 前缀），否则按可索引的文字或图片指纹——
    // 纯文本条目的去重语义不因富文本载荷而改变，文件列表也不会和同名文字互相误判。
    let (content_hash, summary) = if !files.is_empty() {
        let paths: Vec<PathBuf> = files.iter().map(|file| file.path.clone()).collect();
        let names: Vec<String> = files.iter().map(|file| file.name.clone()).collect();
        (content_hash_files(&paths), summary_for_files(&names))
    } else {
        match (&text, image) {
            (Some(text), _) => (content_hash_text(text), summary_for_text(text)),
            (None, Some(image)) => (content_hash_image(&image.bytes), summary_for_image(image)),
            (None, None) => return None,
        }
    };

    Some(ClipboardEvent {
        id: new_event_id(),
        captured_at_ms: now_ms,
        content_hash,
        summary,
        text,
        formats,
        attachments,
        payloads,
        source: capture.source.clone(),
        pinned: false,
        copies: 1,
    })
}

/// 附件在历史里的显示名。
fn attachment_name(image: &ClipboardImage) -> String {
    if image.width > 0 && image.height > 0 {
        format!("图片 {}×{}.png", image.width, image.height)
    } else {
        "剪贴板图片.png".to_string()
    }
}

/// 需要原样保留的载荷（HTML / RTF 等）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardPayload {
    pub role: PayloadRole,
    /// 内联内容。图片与文件走附件，这里是 `None`。
    pub inline: Option<String>,
    /// 指向附件的标识（图片 / 文件副本）。
    pub attachment_id: Option<String>,
    pub mime: Option<String>,
    pub bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "role")]
pub enum PayloadRole {
    Html,
    Rtf,
    Other { name: String },
}

impl PayloadRole {
    pub fn tag(&self) -> &str {
        match self {
            PayloadRole::Html => "html",
            PayloadRole::Rtf => "rtf",
            PayloadRole::Other { name } => name,
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "html" => PayloadRole::Html,
            "rtf" => PayloadRole::Rtf,
            other => PayloadRole::Other {
                name: other.to_string(),
            },
        }
    }
}

/// 附件引用：图片、已保存的文件副本，或对原文件的引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardAttachment {
    pub id: String,
    pub kind: AttachmentKind,
    pub path: PathBuf,
    pub name: String,
    pub mime: Option<String>,
    pub bytes: u64,
    /// 是否依赖原文件。引用类附件在原文件消失后不可恢复（spec「分清文件引用与已保存副本」）。
    pub depends_on_source: bool,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AttachmentKind {
    Image,
    FileCopy,
    FileReference,
    Other { name: String },
}

impl AttachmentKind {
    pub fn tag(&self) -> &str {
        match self {
            AttachmentKind::Image => "image",
            AttachmentKind::FileCopy => "file-copy",
            AttachmentKind::FileReference => "file-reference",
            AttachmentKind::Other { name } => name,
        }
    }

    pub fn from_tag(tag: &str) -> Self {
        match tag {
            "image" => AttachmentKind::Image,
            "file-copy" => AttachmentKind::FileCopy,
            "file-reference" => AttachmentKind::FileReference,
            other => AttachmentKind::Other {
                name: other.to_string(),
            },
        }
    }
}

/// 一次复制事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardEvent {
    /// 稳定标识，跨查询与重启一致。
    pub id: String,
    /// 捕获时间（Unix 毫秒）。
    pub captured_at_ms: i64,
    /// 去重键：同一内容的事件只有一条，重复复制只累加 [`Self::copies`]。
    pub content_hash: String,
    /// 可读摘要（结果标题）。
    pub summary: String,
    /// 可索引的文字内容。图片与文件列表没有它，只能用名称与元数据检索
    /// （spec「不承诺 OCR」）。
    pub text: Option<String>,
    pub formats: Vec<ClipboardFormat>,
    pub attachments: Vec<ClipboardAttachment>,
    pub payloads: Vec<ClipboardPayload>,
    /// 来源应用（平台能提供时）。
    pub source: Option<ClipboardSourceApp>,
    pub pinned: bool,
    /// 同一内容被复制的次数（首次为 1）。去重与自身写入抑制据此区分。
    pub copies: u32,
}

impl ClipboardEvent {
    /// 结果条目的稳定标识。
    pub fn item_id(&self) -> String {
        format!("{CLIPBOARD_ITEM_PREFIX}{}", self.id)
    }

    /// 事件里有哪些格式（面向用户的短标签）。
    pub fn format_labels(&self) -> Vec<&'static str> {
        self.formats.iter().map(ClipboardFormat::label_zh).collect()
    }

    /// 这条历史里的图片附件（ticket 10）。没有图片时返回 `None`。
    pub fn image_attachment(&self) -> Option<&ClipboardAttachment> {
        self.attachments
            .iter()
            .find(|attachment| attachment.kind == AttachmentKind::Image)
    }

    /// 图片格式的「类型 + 尺寸」描述（用于结果副标题）。
    pub fn image_label(&self) -> Option<String> {
        self.formats.iter().find_map(image_label)
    }
}

/// 从结果标识还原事件标识。
pub fn event_id_from_item_id(item_id: &str) -> Option<&str> {
    item_id
        .strip_prefix(CLIPBOARD_ITEM_PREFIX)
        .filter(|id| !id.is_empty())
}

/// 生成一个新的事件标识：时间戳保证可读与大致有序，进程内计数器保证唯一。
pub fn new_event_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = now_ms();
    format!("clip-{nanos:x}-{counter:x}")
}

/// 当前 Unix 毫秒。
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// 文字内容的去重键。
///
/// 用 FNV-1a 64 而不是 `std` 的 `DefaultHasher`：后者不保证跨 Rust 版本稳定，
/// 升级编译器后旧条目会突然「不再重复」，历史里会出现两条同样的内容。
pub fn content_hash_text(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("text:{hash:016x}")
}

/// 文件列表的去重键。
///
/// 与 [`content_hash_text`] 用同一套 FNV-1a 64，但**前缀是 `files:`**，并且按顺序把每个
/// 路径和长度都混进来：同样的文字与同样的文件列表不会互相误判成同一条历史
/// （spec「去重不跨类型误判」）。路径之间夹一个 0 字节，`["ab","c"]` 与 `["a","bc"]`
/// 不会算出同一个键。
pub fn content_hash_files(paths: &[PathBuf]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |bytes: &[u8]| {
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    for path in paths {
        mix(path.to_string_lossy().as_bytes());
        mix(&[0]);
    }
    format!("files:{hash:016x}")
}

/// 由文件名称生成可读摘要；名称全部保留到截断为止，便于在结果里按名称检索。
pub fn summary_for_files(names: &[String]) -> String {
    if names.is_empty() {
        return "（空文件列表）".to_string();
    }
    let joined = if names.len() == 1 {
        names[0].clone()
    } else {
        format!("{} 个文件：{}", names.len(), names.join("、"))
    };
    if joined.chars().count() <= SUMMARY_MAX_CHARS {
        return joined;
    }
    let mut summary: String = joined.chars().take(SUMMARY_MAX_CHARS).collect();
    summary.push('…');
    summary
}

/// 由文字生成可读摘要：折叠空白、截断到 [`SUMMARY_MAX_CHARS`]。
pub fn summary_for_text(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        return "（空白内容）".to_string();
    }
    let mut summary: String = normalized.chars().take(SUMMARY_MAX_CHARS).collect();
    if normalized.chars().count() > SUMMARY_MAX_CHARS {
        summary.push('…');
    }
    summary
}

/// 图片内容的去重键。
///
/// 用同一套 FNV-1a 64（理由见 [`content_hash_text`]），但**必须**带 `image:` 前缀：
/// 文字与图片共用一张表，没有前缀时一段文字与一份图片字节可能算出同一个键，
/// 跨类型误判成「同一条内容」会让用户复制图片时看到的却是一条旧文字。
pub fn content_hash_image(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("image:{hash:016x}")
}

/// 由图片元数据生成可读摘要（结果标题，也是这类条目唯一可检索的文字，spec「不承诺 OCR」）。
pub fn summary_for_image(image: &ClipboardImage) -> String {
    let kind = mime_short_label(&image.mime);
    let size = human_bytes(image.bytes.len() as u64);
    if image.width > 0 && image.height > 0 {
        format!("图片 {kind} {}×{}（{size}）", image.width, image.height)
    } else {
        format!("图片 {kind}（{size}）")
    }
}

/// MIME 的短标签（`image/png` → `PNG`），用于结果副标题与摘要。
pub fn mime_short_label(mime: &str) -> String {
    let subtype = mime.rsplit('/').next().unwrap_or(mime);
    match subtype.to_ascii_lowercase().as_str() {
        "jpeg" | "jpg" => "JPEG".to_string(),
        other => other.to_ascii_uppercase(),
    }
}

/// 人类可读的字节数（结果副标题用，精确到一位小数）。
pub fn human_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

/// 图片格式的「类型 + 尺寸」描述，例如 `PNG 1920×1080`；不是图片时返回 `None`。
pub fn image_label(format: &ClipboardFormat) -> Option<String> {
    match format {
        ClipboardFormat::Image {
            mime,
            width,
            height,
            ..
        } => Some(if *width > 0 && *height > 0 {
            format!("{} {}×{}", mime_short_label(mime), width, height)
        } else {
            format!("{} 尺寸未知", mime_short_label(mime))
        }),
        _ => None,
    }
}

/// 相对时间的可读描述，用于结果副标题。
///
/// 只精确到天：历史条目的价值在内容而不是秒级时间，跨越一周以上也不必为此引入
/// 日期格式化依赖（这里只做减法）。
pub fn describe_age(captured_at_ms: i64, now: i64) -> String {
    let delta = now.saturating_sub(captured_at_ms);
    if delta < 60_000 {
        "刚刚".to_string()
    } else if delta < 3_600_000 {
        format!("{} 分钟前", delta / 60_000)
    } else if delta < 86_400_000 {
        format!("{} 小时前", delta / 3_600_000)
    } else {
        format!("{} 天前", delta / 86_400_000)
    }
}

/// 生成一个附件标识（图片、文件副本共用一套前缀）。
pub fn new_attachment_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    format!("att-{:x}-{counter:x}", now_ms())
}

/// 单份本机副本的容量上限（一个文件）。
pub const MAX_COPY_BYTES: u64 = 512 * 1024 * 1024;

/// 本机副本占用的**总**容量上限（附件目录里的所有副本文件）。
pub const MAX_COPY_TOTAL_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// 已保存副本记录其来源原文件的载荷角色。
///
/// 用载荷行（role 是字符串标签）而不是新增列：ticket 09 定下的 schema 不再迁移，
/// 未知 role 在读写两侧都会降级成 [`PayloadRole::Other`]，将来也不会丢数据。
pub const PAYLOAD_ROLE_FILE_SOURCE: &str = "file-source";

/// 显式保存本机副本时的准确失败原因。每一种都必须能被用户看懂，不能都折成「失败」。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardCopyError {
    #[error("找不到这条剪贴板历史或它的文件条目：{0}")]
    NotFound(String),
    #[error("「{0}」已经是本机副本，不需要再复制一次")]
    AlreadySaved(String),
    #[error("原文件已不存在或被移动：{0}")]
    SourceMissing(PathBuf),
    #[error("无法读取原文件「{path}」：{reason}")]
    AccessFailed { path: PathBuf, reason: String },
    #[error("不支持的文件类型（目录或特殊文件）：{0}")]
    UnsupportedType(PathBuf),
    #[error("文件 {bytes} 字节，超过单份副本上限 {limit} 字节")]
    TooLarge { bytes: u64, limit: u64 },
    #[error("本机副本已占用 {used} 字节，再加 {bytes} 字节会超过总上限 {limit} 字节")]
    TotalLimitReached { used: u64, limit: u64, bytes: u64 },
    #[error("复制中断：{reason}")]
    CopyFailed { reason: String },
    #[error("剪贴板历史存储失败：{0}")]
    Storage(String),
}

/// 恢复文件列表时的失败原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardRestoreError {
    #[error("「{0}」不是文件列表，没有可恢复的文件")]
    NotAFileList(String),
    #[error("这些文件已不可恢复：{names}（原文件已被移动或删除，且没有保存本机副本）", names = names.join("、"))]
    Unrecoverable { names: Vec<String> },
}

/// 列表与预览里的一个文件条目。
///
/// `kind` 与 `recoverable` 一起回答两个不同的问题：**这是引用还是副本**，以及
/// **现在还能不能恢复**。引用在原文件消失后必须如实显示不可恢复，而副本必须仍然可恢复。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardFileView {
    pub attachment_id: String,
    pub name: String,
    /// 恢复时实际写进剪贴板的路径（引用＝原路径，副本＝原文件仍在时的原路径）。
    pub path: PathBuf,
    /// 副本记录的原文件路径；引用与未知类型为 `None`。
    pub source_path: Option<PathBuf>,
    /// 引用 / 已保存副本 / 未知附件类型。
    pub kind: AttachmentKind,
    pub mime: Option<String>,
    pub bytes: u64,
    /// 现在是否可以恢复。
    pub recoverable: bool,
    /// 不可恢复的中文原因；可恢复时为 `None`。
    pub problem: Option<String>,
}

impl ClipboardFileView {
    /// 面向用户的状态：引用、已保存副本，或未知类型。
    pub fn kind_label_zh(&self) -> &'static str {
        file_kind_label_zh(&self.kind)
    }
}

/// 文件类附件的中文名。
pub fn file_kind_label_zh(kind: &AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::FileReference => "引用",
        AttachmentKind::FileCopy => "已保存副本",
        AttachmentKind::Image => "图片",
        AttachmentKind::Other { .. } => "未知附件",
    }
}

/// 附件是否是**文件类**（引用或副本）。图片不算文件列表。
pub fn is_file_attachment(attachment: &ClipboardAttachment) -> bool {
    matches!(
        attachment.kind,
        AttachmentKind::FileReference | AttachmentKind::FileCopy
    )
}

impl ClipboardEvent {
    /// 这条事件里的文件格式（如果有）。
    pub fn files_format(&self) -> Option<&ClipboardFormat> {
        self.formats
            .iter()
            .find(|format| matches!(format, ClipboardFormat::Files { .. }))
    }

    /// 这条事件是否是文件列表。
    pub fn is_file_list(&self) -> bool {
        self.files_format().is_some()
            || self
                .attachments
                .iter()
                .any(|attachment| is_file_attachment(attachment))
    }

    /// 文件类附件，按存储顺序（引用与副本都算）。
    pub fn file_attachments(&self) -> Vec<&ClipboardAttachment> {
        self.attachments
            .iter()
            .filter(|attachment| is_file_attachment(attachment))
            .collect()
    }

    /// 引用与已保存副本的数量。
    pub fn file_counts(&self) -> (usize, usize) {
        let references = self
            .attachments
            .iter()
            .filter(|attachment| attachment.kind == AttachmentKind::FileReference)
            .count();
        let copies = self
            .attachments
            .iter()
            .filter(|attachment| attachment.kind == AttachmentKind::FileCopy)
            .count();
        (references, copies)
    }

    /// 副本记录的原文件路径（由 `file-source` 载荷承载）。
    pub fn attachment_source_path(&self, attachment: &ClipboardAttachment) -> Option<PathBuf> {
        self.payloads
            .iter()
            .find(|payload| {
                payload.role.tag() == PAYLOAD_ROLE_FILE_SOURCE
                    && payload.attachment_id.as_deref() == Some(attachment.id.as_str())
            })
            .and_then(|payload| payload.inline.clone())
            .map(PathBuf::from)
    }

    /// 现在能不能恢复，以及恢复时会用哪个路径。
    fn file_state(&self, attachment: &ClipboardAttachment) -> (Option<PathBuf>, Option<String>) {
        let source = self.attachment_source_path(attachment);
        match attachment.kind {
            AttachmentKind::FileReference => {
                if attachment.path.is_file() {
                    (Some(attachment.path.clone()), None)
                } else if attachment.path.exists() {
                    (
                        None,
                        Some(format!(
                            "不支持的文件类型（目录或特殊文件）：{}",
                            attachment.path.display()
                        )),
                    )
                } else {
                    (
                        None,
                        Some(format!("原文件已不存在：{}", attachment.path.display())),
                    )
                }
            }
            AttachmentKind::FileCopy => {
                // 原文件还在就用原路径（避免粘贴出一份重复文件）；原文件没了才用副本。
                if let Some(source) = source.as_ref().filter(|source| source.is_file()) {
                    return (Some(source.clone()), None);
                }
                if attachment.path.is_file() {
                    (Some(attachment.path.clone()), None)
                } else {
                    (
                        None,
                        Some(format!("本机副本已丢失：{}", attachment.path.display())),
                    )
                }
            }
            _ => (None, Some("未知的附件类型，无法恢复".to_string())),
        }
    }

    /// 列表与预览用的文件条目（包含不可恢复的条目，状态如实）。
    pub fn file_views(&self) -> Vec<ClipboardFileView> {
        self.attachments
            .iter()
            .filter(|attachment| {
                is_file_attachment(attachment)
                    || matches!(attachment.kind, AttachmentKind::Other { .. })
            })
            .map(|attachment| {
                let (path, problem) = self.file_state(attachment);
                ClipboardFileView {
                    attachment_id: attachment.id.clone(),
                    name: attachment.name.clone(),
                    path: path.unwrap_or_else(|| attachment.path.clone()),
                    source_path: self.attachment_source_path(attachment),
                    kind: attachment.kind.clone(),
                    mime: attachment.mime.clone(),
                    bytes: attachment.bytes,
                    recoverable: problem.is_none(),
                    problem,
                }
            })
            .collect()
    }

    /// 恢复用的文件列表：**整份**列表要么全部可恢复，要么如实失败。
    ///
    /// 只要有一个引用或副本不可用就整体失败：往剪贴板放一份缺了文件的列表，目标应用
    /// 会静默地少粘贴一个文件，那是比明确报错更糟的结果（spec「不伪造成功」）。
    pub fn restore_paths(&self) -> Result<Vec<PathBuf>, ClipboardRestoreError> {
        let files = self.file_attachments();
        if files.is_empty() {
            return Err(ClipboardRestoreError::NotAFileList(self.summary.clone()));
        }
        let mut paths = Vec::with_capacity(files.len());
        let mut missing = Vec::new();
        for attachment in files {
            let (path, problem) = self.file_state(attachment);
            match path {
                Some(path) => paths.push(path),
                None => missing.push(format!(
                    "{}（{}）",
                    attachment.name,
                    problem.unwrap_or_else(|| "不可恢复".to_string())
                )),
            }
        }
        if !missing.is_empty() {
            return Err(ClipboardRestoreError::Unrecoverable { names: missing });
        }
        Ok(paths)
    }
}

/// 剪贴板历史**管理操作**（置顶、删除、清空）的失败原因，面向用户的中文。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardActionError {
    #[error("找不到这条剪贴板历史：{0}")]
    NotFound(String),
    #[error("{0}")]
    Storage(String),
    /// 显式保存本机副本失败（容量、访问、中断等）。
    #[error("{0}")]
    Copy(#[from] ClipboardCopyError),
}
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardStoreError {
    #[error("剪贴板历史存储不可用：{0}")]
    Unavailable(String),
    #[error("剪贴板历史读写失败：{0}")]
    Database(String),
    #[error("剪贴板历史数据不可用：{0}")]
    Corrupt(String),
}

/// 一次插入的结果。三种结果都必须被如实报告，不能都当成「成功」。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InsertOutcome {
    /// 新条目。
    Inserted { id: String },
    /// 同一内容已经存在：不新增条目，把已有条目提到最新并累加重复次数。
    Deduplicated { id: String, copies: u32 },
    /// 容量已满，且剩下的都是置顶条目：如实拒绝。
    CapacityReached { entries: usize, capacity: usize },
}

/// 一次回收（保留期限 + 容量）的结果。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReclaimReport {
    /// 因超过保留期限被删除的条目数。
    pub expired: usize,
    /// 因超过容量被删除的条目数。
    pub over_capacity: usize,
    /// 同步回收的附件文件数。
    pub attachments: usize,
    /// 回收后剩下的条目数。
    pub remaining: usize,
    /// 回收后剩下的置顶条目数。
    pub pinned: usize,
    /// 回收后仍然超出容量（全部是置顶条目）。
    pub capacity_reached: bool,
}

/// 历史规模统计，用于准确报告容量状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClipboardStats {
    pub total: usize,
    pub pinned: usize,
    pub attachments: usize,
}

/// 本机剪贴板历史存储。
///
/// 打开失败**不会**让宿主不可用：原因被记下来，之后每次访问都会重试并如实返回
/// [`ClipboardStoreError::Unavailable`]，宿主据此给出「存储失败」状态而不是静默成功。
pub struct ClipboardStore {
    root: PathBuf,
    state: Mutex<StoreState>,
}

#[derive(Default)]
struct StoreState {
    conn: Option<Connection>,
    error: Option<String>,
}

/// 事件表的列清单（顺序与 [`event_from_row`] 一致）。
const EVENT_COLUMNS: &str =
    "id, captured_at, content_hash, summary, text_content, source_app_id, source_title, pinned, copies";

impl ClipboardStore {
    /// 打开（或创建）剪贴板历史库。
    pub fn open(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let store = Self {
            root,
            state: Mutex::new(StoreState::default()),
        };
        // 立即尝试一次：让「存储不可用」尽早暴露，但失败不 panic（宿主仍可用）。
        let opened = Self::connect(&store.root);
        let mut state = store.state.lock().unwrap_or_else(|p| p.into_inner());
        match opened {
            Ok(conn) => state.conn = Some(conn),
            Err(error) => state.error = Some(error.to_string()),
        }
        drop(state);
        store
    }

    /// 设备本地目录下的剪贴板历史根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.join(CLIPBOARD_DB_FILE)
    }

    pub fn attachments_dir(&self) -> PathBuf {
        self.root.join(ATTACHMENTS_DIR)
    }

    /// 最近一次存储失败的中文原因。
    ///
    /// 已经记下错误时会**主动重试一次**访问：上次失败的原因（权限、磁盘、占位文件）
    /// 可能已经被修好，此时这里要如实回到可用状态，而不是一直显示旧错误。
    /// 重试走 [`Self::stats`]（它自己拿锁），因此这里必须先放开锁再调用。
    pub fn storage_error(&self) -> Option<String> {
        let recorded = self
            .state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .error
            .clone();
        if recorded.is_some() {
            let _ = self.stats();
        }
        self.state
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .error
            .clone()
    }

    /// 存储当前是否可用。
    pub fn is_available(&self) -> bool {
        self.storage_error().is_none()
    }

    fn connect(root: &Path) -> Result<Connection, ClipboardStoreError> {
        std::fs::create_dir_all(root).map_err(|error| {
            ClipboardStoreError::Unavailable(format!(
                "无法创建本机数据目录 {}：{error}",
                root.display()
            ))
        })?;
        let path = root.join(CLIPBOARD_DB_FILE);
        let conn = Connection::open(&path).map_err(|error| {
            ClipboardStoreError::Unavailable(format!("无法打开 {}：{error}", path.display()))
        })?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )
        .map_err(|error| ClipboardStoreError::Unavailable(error.to_string()))?;
        migrate(&conn)?;
        Ok(conn)
    }

    /// 借用连接。连接缺失时惰性重试一次（上次失败的原因可能已经修复）。
    fn with_conn<T>(
        &self,
        work: impl FnOnce(&Connection) -> rusqlite::Result<T>,
    ) -> Result<T, ClipboardStoreError> {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if state.conn.is_none() {
            // 注意：这里调用的是**关联函数**，不是在持有 `state` 的同时调用宿主方法。
            match Self::connect(&self.root) {
                Ok(conn) => {
                    state.conn = Some(conn);
                    state.error = None;
                }
                Err(error) => {
                    state.error = Some(error.to_string());
                    return Err(error);
                }
            }
        }
        let conn = state.conn.as_ref().expect("连接刚刚被填好");
        work(conn).map_err(|error| ClipboardStoreError::Database(error.to_string()))
    }

    /// 历史规模统计。
    pub fn stats(&self) -> Result<ClipboardStats, ClipboardStoreError> {
        self.with_conn(|conn| {
            let total: i64 =
                conn.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                    row.get(0)
                })?;
            let pinned: i64 = conn.query_row(
                "SELECT COUNT(*) FROM clipboard_events WHERE pinned = 1",
                [],
                |row| row.get(0),
            )?;
            let attachments: i64 =
                conn.query_row("SELECT COUNT(*) FROM clipboard_attachments", [], |row| {
                    row.get(0)
                })?;
            Ok(ClipboardStats {
                total: total.max(0) as usize,
                pinned: pinned.max(0) as usize,
                attachments: attachments.max(0) as usize,
            })
        })
    }

    /// 插入一次复制事件。
    ///
    /// 顺序不可颠倒：**先去重**（同一内容不新增条目），再检查容量（超过时先回收最旧的
    /// 未置顶条目，只剩下置顶条目时如实拒绝）。
    pub fn insert(
        &self,
        event: &ClipboardEvent,
        capacity: usize,
    ) -> Result<InsertOutcome, ClipboardStoreError> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            if let Some((id, copies)) = existing_by_hash(&tx, &event.content_hash)? {
                let copies = copies.saturating_add(1);
                tx.execute(
                    "UPDATE clipboard_events SET captured_at = ?1, copies = ?2 WHERE id = ?3",
                    params![event.captured_at_ms, i64::from(copies), id],
                )?;
                // 后一次复制带来了富文本载荷时补进已有条目（见 `enrich_payloads`）。
                enrich_payloads(&tx, &id, event)?;
                tx.commit()?;
                return Ok(InsertOutcome::Deduplicated { id, copies });
            }
            let total: i64 = tx.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                row.get(0)
            })?;
            let total = total.max(0) as usize;
            if capacity > 0 && total >= capacity {
                let needed = total - capacity + 1;
                let evicted = tx.execute(
                    "DELETE FROM clipboard_events WHERE id IN (
                         SELECT id FROM clipboard_events WHERE pinned = 0
                         ORDER BY captured_at ASC, rowid ASC LIMIT ?1)",
                    params![needed as i64],
                )?;
                if evicted < needed {
                    let remaining: i64 =
                        tx.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                            row.get(0)
                        })?;
                    // 容量已满且全是置顶条目：不写入，由宿主给出准确状态。
                    return Ok(InsertOutcome::CapacityReached {
                        entries: remaining.max(0) as usize,
                        capacity,
                    });
                }
            }
            insert_event(&tx, event)?;
            tx.commit()?;
            Ok(InsertOutcome::Inserted {
                id: event.id.clone(),
            })
        })
    }

    /// 按标识取出一条事件（含格式、附件与载荷）。
    pub fn find(&self, id: &str) -> Result<Option<ClipboardEvent>, ClipboardStoreError> {
        self.with_conn(|conn| {
            let sql = format!("SELECT {EVENT_COLUMNS} FROM clipboard_events WHERE id = ?1");
            let mut event = conn
                .query_row(&sql, params![id], event_from_row)
                .optional()?;
            if let Some(event) = event.as_mut() {
                hydrate(conn, event)?;
            }
            Ok(event)
        })
    }

    /// 列出历史：置顶优先，然后按捕获时间倒序。`query` 非空时按文字、摘要与来源应用过滤。
    ///
    /// `query` 里的 `%` 与 `_` 会被转义，用户输入不会被当成通配符。
    pub fn list(
        &self,
        query: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ClipboardEvent>, ClipboardStoreError> {
        let pattern = query
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                format!(
                    "%{}%",
                    value
                        .replace('\\', "\\\\")
                        .replace('%', "\\%")
                        .replace('_', "\\_")
                )
            });
        self.with_conn(|conn| {
            let mut events = Vec::new();
            match &pattern {
                Some(pattern) => {
                    // 文件名称存在 `clipboard_formats.names`（JSON 数组）里，摘要可能因为
                    // 截断而看不到后面的文件，因此名称**单独**参与检索（ticket 12）。
                    let sql = format!(
                        "SELECT {EVENT_COLUMNS} FROM clipboard_events
                         WHERE text_content LIKE ?1 ESCAPE '\\'
                            OR summary LIKE ?1 ESCAPE '\\'
                            OR source_app_id LIKE ?1 ESCAPE '\\'
                            OR source_title LIKE ?1 ESCAPE '\\'
                            OR EXISTS (
                                 SELECT 1 FROM clipboard_formats fmt
                                 WHERE fmt.event_id = clipboard_events.id
                                   AND fmt.names LIKE ?1 ESCAPE '\\')
                         ORDER BY pinned DESC, captured_at DESC, rowid DESC LIMIT ?2"
                    );
                    let mut stmt = conn.prepare(&sql)?;
                    let rows = stmt.query_map(params![pattern, limit as i64], event_from_row)?;
                    for row in rows {
                        events.push(row?);
                    }
                }
                None => {
                    let sql = format!(
                        "SELECT {EVENT_COLUMNS} FROM clipboard_events
                         ORDER BY pinned DESC, captured_at DESC, rowid DESC LIMIT ?1"
                    );
                    let mut stmt = conn.prepare(&sql)?;
                    let rows = stmt.query_map(params![limit as i64], event_from_row)?;
                    for row in rows {
                        events.push(row?);
                    }
                }
            }
            for event in events.iter_mut() {
                hydrate(conn, event)?;
            }
            Ok(events)
        })
    }

    /// 置顶 / 取消置顶。返回是否真的有一条记录被改动。
    pub fn set_pinned(&self, id: &str, pinned: bool) -> Result<bool, ClipboardStoreError> {
        self.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE clipboard_events SET pinned = ?1, pinned_at = ?2 WHERE id = ?3",
                params![
                    if pinned { 1 } else { 0 },
                    if pinned { Some(now_ms()) } else { None },
                    id
                ],
            )?;
            Ok(changed > 0)
        })
    }

    /// 删除一条（含它的格式、载荷与附件行）。
    ///
    /// 条目消失之后它的附件文件不再被任何行引用，因此同步回收
    /// （spec「历史删除与过期清理同步回收不再引用的附件」）。
    pub fn delete(&self, id: &str) -> Result<bool, ClipboardStoreError> {
        let changed = self.with_conn(|conn| {
            let changed =
                conn.execute("DELETE FROM clipboard_events WHERE id = ?1", params![id])?;
            Ok(changed)
        })?;
        if changed > 0 {
            let _ = self.reclaim_attachment_files();
        }
        Ok(changed > 0)
    }

    /// 清空历史（用户在设置里的显式操作，置顶条目也会被清掉）。
    pub fn clear(&self) -> Result<usize, ClipboardStoreError> {
        let removed = self.with_conn(|conn| {
            let changed = conn.execute("DELETE FROM clipboard_events", [])?;
            Ok(changed)
        })?;
        // 附件文件随条目一起回收。
        let _ = self.reclaim_attachment_files();
        Ok(removed)
    }

    /// 公开的孤儿附件回收：删除一条历史之后由宿主调用（spec「历史删除与过期清理同步
    /// 回收不再引用的附件」）。
    ///
    /// 只删除**本机附件目录内**、且没有任何附件行（任何事件）指向的文件：两份历史共享
    /// 同一个副本文件时，删掉其中一条不会把另一条还在用的副本删掉。
    pub fn reclaim_orphan_files(&self) -> Result<usize, ClipboardStoreError> {
        self.reclaim_attachment_files()
    }

    /// 一个原文件对应的副本落点（去重键 + 安全文件名）。
    ///
    /// 对调用方（宿主）没有用，公开只是为了测试能构造「复制中断」这种真实故障：
    /// 在落点上放一个同名目录，原子改名就会失败并如实报「复制中断」。
    #[doc(hidden)]
    pub fn file_copy_target_path(&self, source: &Path, bytes: u64, name: &str) -> PathBuf {
        self.attachments_dir().join(format!(
            "{}-{}",
            copy_key(source, bytes),
            safe_file_name(name)
        ))
    }

    /// 附件目录里所有副本文件占用的字节数（共享的副本只算一次）。
    pub fn copy_dir_bytes(&self) -> Result<u64, ClipboardStoreError> {
        let dir = self.attachments_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(ClipboardStoreError::Unavailable(format!(
                    "无法读取附件目录 {}：{error}",
                    dir.display()
                )))
            }
        };
        let mut total = 0u64;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                total = total.saturating_add(entry.metadata().map(|meta| meta.len()).unwrap_or(0));
            }
        }
        Ok(total)
    }

    /// 为一个**文件引用**显式保存本机副本（spec「用户可以明确保存受容量限制的本机副本」）。
    ///
    /// 语义逐条对应 spec：
    ///
    /// - **不自动**：只有调用方显式要求才会复制，捕获路径永远不会调用它；
    /// - **不动原文件**：只从原文件读，从不移动或删除它；
    /// - **容量受限**：单份超过 [`MAX_COPY_BYTES`] 或总量超过 [`MAX_COPY_TOTAL_BYTES`]
    ///   时如实拒绝，并在失败原因里给出数字；
    /// - **去重共享**：副本路径由「原文件路径 + 大小」决定，同一原文件被两条历史各保存
    ///   一次时**复用同一个文件**，因此删除其中一条不能删掉另一条还在用的副本；
    /// - **中断可恢复**：先写临时文件再原子改名，任何一步失败都清掉临时文件并如实报
    ///   「复制中断」，不会留下半份副本，也不会留下半行记录。
    pub fn save_file_copy(
        &self,
        event_id: &str,
        attachment_id: &str,
        per_file_limit: u64,
        total_limit: u64,
    ) -> Result<ClipboardAttachment, ClipboardCopyError> {
        let event = self
            .find(event_id)
            .map_err(|error| ClipboardCopyError::Storage(error.to_string()))?
            .ok_or_else(|| ClipboardCopyError::NotFound(event_id.to_string()))?;
        let attachment = event
            .attachments
            .iter()
            .find(|attachment| attachment.id == attachment_id)
            .cloned()
            .ok_or_else(|| ClipboardCopyError::NotFound(attachment_id.to_string()))?;
        if attachment.kind == AttachmentKind::FileCopy {
            return Err(ClipboardCopyError::AlreadySaved(attachment.name));
        }
        if !is_file_attachment(&attachment) {
            return Err(ClipboardCopyError::UnsupportedType(attachment.path));
        }
        let source = attachment.path.clone();
        let metadata = std::fs::metadata(&source).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ClipboardCopyError::SourceMissing(source.clone())
            } else {
                ClipboardCopyError::AccessFailed {
                    path: source.clone(),
                    reason: error.to_string(),
                }
            }
        })?;
        if !metadata.is_file() {
            return Err(ClipboardCopyError::UnsupportedType(source));
        }
        let bytes = metadata.len();
        if per_file_limit > 0 && bytes > per_file_limit {
            return Err(ClipboardCopyError::TooLarge {
                bytes,
                limit: per_file_limit,
            });
        }
        let dir = self.attachments_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            ClipboardCopyError::Storage(format!("无法创建附件目录 {}：{error}", dir.display()))
        })?;
        let target = dir.join(format!(
            "{}-{}",
            copy_key(&source, bytes),
            safe_file_name(&attachment.name)
        ));
        if !target.is_file() {
            let used = self
                .copy_dir_bytes()
                .map_err(|error| ClipboardCopyError::Storage(error.to_string()))?;
            if total_limit > 0 && used.saturating_add(bytes) > total_limit {
                return Err(ClipboardCopyError::TotalLimitReached {
                    used,
                    limit: total_limit,
                    bytes,
                });
            }
            copy_atomically(&source, &target, &dir)?;
        }
        let copy = ClipboardAttachment {
            // 引用行**原地**变成副本行：一个文件在列表里只出现一次，状态从「引用」
            // 变成「已保存副本」，不会同时显示一条不可恢复的引用和一条可恢复的副本。
            id: attachment.id.clone(),
            kind: AttachmentKind::FileCopy,
            path: target,
            name: attachment.name.clone(),
            mime: attachment.mime.clone(),
            bytes,
            depends_on_source: false,
            created_at_ms: now_ms(),
        };
        self.replace_reference_with_copy(event_id, &copy, &source)
            .map_err(|error| ClipboardCopyError::Storage(error.to_string()))?;
        Ok(copy)
    }

    /// 把一行文件引用改写成已保存副本，并记录它对应的原文件路径。
    ///
    /// 在同一个事务里完成：不会出现「行改了一半」或「副本没有来源记录」的中间状态。
    fn replace_reference_with_copy(
        &self,
        event_id: &str,
        copy: &ClipboardAttachment,
        source: &Path,
    ) -> Result<(), ClipboardStoreError> {
        self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let ordinal: i64 = tx.query_row(
                "SELECT ordinal FROM clipboard_attachments WHERE id = ?1 AND event_id = ?2",
                params![copy.id, event_id],
                |row| row.get(0),
            )?;
            tx.execute(
                "UPDATE clipboard_attachments
                    SET kind = ?1, path = ?2, name = ?3, mime = ?4, bytes = ?5,
                        depends_on_source = 0, created_at = ?6
                  WHERE id = ?7 AND event_id = ?8",
                params![
                    copy.kind.tag(),
                    copy.path.to_string_lossy(),
                    copy.name,
                    copy.mime,
                    copy.bytes as i64,
                    copy.created_at_ms,
                    copy.id,
                    event_id,
                ],
            )?;
            tx.execute(
                "INSERT OR REPLACE INTO clipboard_payloads
                     (event_id, role, ordinal, inline_text, attachment_id, mime, bytes)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    event_id,
                    PAYLOAD_ROLE_FILE_SOURCE,
                    ordinal,
                    source.to_string_lossy(),
                    copy.id,
                    Option::<String>::None,
                    copy.bytes as i64,
                ],
            )?;
            tx.commit()?;
            Ok(())
        })
    }

    /// 回收过期条目与超容量条目，并回收不再被引用的附件文件。
    pub fn reclaim(
        &self,
        retention_days: u32,
        capacity: usize,
        now: i64,
    ) -> Result<ReclaimReport, ClipboardStoreError> {
        let mut report = self.with_conn(|conn| {
            let tx = conn.unchecked_transaction()?;
            let cutoff = now - i64::from(retention_days) * 86_400_000;
            let expired = tx.execute(
                "DELETE FROM clipboard_events WHERE pinned = 0 AND captured_at < ?1",
                params![cutoff],
            )?;
            let mut over_capacity = 0usize;
            if capacity > 0 {
                let total: i64 =
                    tx.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                        row.get(0)
                    })?;
                let total = total.max(0) as usize;
                if total > capacity {
                    over_capacity = tx.execute(
                        "DELETE FROM clipboard_events WHERE id IN (
                             SELECT id FROM clipboard_events WHERE pinned = 0
                             ORDER BY captured_at ASC, rowid ASC LIMIT ?1)",
                        params![(total - capacity) as i64],
                    )?;
                }
            }
            // 事件已经不在的附件行（外键关闭过的历史库）先清掉。
            tx.execute(
                "DELETE FROM clipboard_attachments
                 WHERE event_id NOT IN (SELECT id FROM clipboard_events)",
                [],
            )?;
            let total: i64 = tx.query_row("SELECT COUNT(*) FROM clipboard_events", [], |row| {
                row.get(0)
            })?;
            let pinned: i64 = tx.query_row(
                "SELECT COUNT(*) FROM clipboard_events WHERE pinned = 1",
                [],
                |row| row.get(0),
            )?;
            tx.commit()?;
            Ok(ReclaimReport {
                expired,
                over_capacity,
                attachments: 0,
                remaining: total.max(0) as usize,
                pinned: pinned.max(0) as usize,
                capacity_reached: capacity > 0 && total.max(0) as usize > capacity,
            })
        })?;
        report.attachments = self.reclaim_attachment_files()?;
        Ok(report)
    }

    /// 把附件字节写进本机附件目录（ticket 10 的图片）。
    ///
    /// 先写临时文件再 `rename`：`rename` 在同一文件系统内是原子的，因此不会出现
    /// 「数据库里有一条指向半截文件的附件」这种状态。目录不存在时创建。
    pub fn write_attachment(&self, path: &Path, bytes: &[u8]) -> Result<(), ClipboardStoreError> {
        let dir = self.attachments_dir();
        std::fs::create_dir_all(&dir).map_err(|error| {
            ClipboardStoreError::Unavailable(format!("无法创建附件目录 {}：{error}", dir.display()))
        })?;
        let temp = match path.file_name() {
            Some(name) => dir.join(format!("{}.part", name.to_string_lossy())),
            None => {
                return Err(ClipboardStoreError::Unavailable(format!(
                    "附件路径没有文件名：{}",
                    path.display()
                )))
            }
        };
        std::fs::write(&temp, bytes).map_err(|error| {
            ClipboardStoreError::Unavailable(format!("无法写入附件 {}：{error}", temp.display()))
        })?;
        std::fs::rename(&temp, path).map_err(|error| {
            let _ = std::fs::remove_file(&temp);
            ClipboardStoreError::Unavailable(format!("无法保存附件 {}：{error}", path.display()))
        })
    }

    /// 删除附件目录里不再被任何附件行引用的文件。
    fn reclaim_attachment_files(&self) -> Result<usize, ClipboardStoreError> {
        let referenced: Vec<PathBuf> = self.with_conn(|conn| {
            let mut stmt = conn.prepare("SELECT path FROM clipboard_attachments")?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            let mut paths = Vec::new();
            for row in rows {
                paths.push(PathBuf::from(row?));
            }
            Ok(paths)
        })?;
        let dir = self.attachments_dir();
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(ClipboardStoreError::Unavailable(format!(
                    "无法读取附件目录 {}：{error}",
                    dir.display()
                )))
            }
        };
        let mut removed = 0usize;
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            // 只有在**本机附件目录内**、且没有被引用的文件才会被删。
            if referenced.iter().any(|item| item == &path) {
                continue;
            }
            if std::fs::remove_file(&path).is_ok() {
                removed += 1;
            }
        }
        Ok(removed)
    }
}

/// 副本文件名的去重键：原文件路径 + 观察到的字节数。
///
/// 同一原文件保存两次会得到同一个键，因此副本文件被复用（而不是复制两份）；原文件内容
/// 变了（大小不同）就是另一份副本。键只影响本机文件名，不参与历史条目的去重。
fn copy_key(path: &Path, bytes: u64) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |data: &[u8]| {
        for byte in data {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    mix(path.to_string_lossy().as_bytes());
    mix(&[0]);
    mix(&bytes.to_le_bytes());
    format!("{hash:016x}")
}

/// 把用户文件名变成安全的单层文件名：去掉路径分隔符与控制字符，非 ASCII 原样保留。
fn safe_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|character| {
            if character == '/' || character == '\\' || character.is_control() {
                '_'
            } else {
                character
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['.', ' ']).to_string();
    if cleaned.is_empty() {
        return "file".to_string();
    }
    cleaned.chars().take(120).collect()
}

/// 原子复制：先写同目录下的临时文件，成功后再改名。
///
/// 任何一步失败都删掉临时文件并返回 [`ClipboardCopyError::CopyFailed`]（「复制中断」），
/// 不留下半份副本；`rename` 在同一目录内是原子的，因此不会出现「文件名存在但内容不全」。
fn copy_atomically(source: &Path, target: &Path, dir: &Path) -> Result<(), ClipboardCopyError> {
    use std::io::{Read, Write};
    let temp = dir.join(format!(".part-{}-{}", std::process::id(), now_ms()));
    let result = (|| -> std::io::Result<()> {
        let mut input = std::fs::File::open(source)?;
        let mut output = std::fs::File::create(&temp)?;
        let mut buffer = vec![0u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        drop(output);
        std::fs::rename(&temp, target)
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(ClipboardCopyError::CopyFailed {
                reason: format!("写入 {} 失败：{error}", target.display()),
            })
        }
    }
}

/// 建表。schema 从 ticket 09 起就是完整的（见模块文档）。
fn migrate(conn: &Connection) -> Result<(), ClipboardStoreError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS clipboard_meta (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS clipboard_events (
             id            TEXT PRIMARY KEY,
             captured_at   INTEGER NOT NULL,
             content_hash  TEXT NOT NULL,
             summary       TEXT NOT NULL,
             text_content  TEXT,
             source_app_id TEXT,
             source_title  TEXT,
             pinned        INTEGER NOT NULL DEFAULT 0,
             pinned_at     INTEGER,
             copies        INTEGER NOT NULL DEFAULT 1
         );
         CREATE INDEX IF NOT EXISTS idx_clipboard_events_order
             ON clipboard_events(pinned DESC, captured_at DESC);
         CREATE INDEX IF NOT EXISTS idx_clipboard_events_hash
             ON clipboard_events(content_hash);
         CREATE TABLE IF NOT EXISTS clipboard_formats (
             event_id   TEXT NOT NULL REFERENCES clipboard_events(id) ON DELETE CASCADE,
             ordinal    INTEGER NOT NULL,
             kind       TEXT NOT NULL,
             bytes      INTEGER NOT NULL DEFAULT 0,
             mime       TEXT,
             width      INTEGER,
             height     INTEGER,
             item_count INTEGER,
             names      TEXT,
             PRIMARY KEY (event_id, ordinal)
         );
         CREATE TABLE IF NOT EXISTS clipboard_payloads (
             event_id      TEXT NOT NULL REFERENCES clipboard_events(id) ON DELETE CASCADE,
             role          TEXT NOT NULL,
             ordinal       INTEGER NOT NULL DEFAULT 0,
             inline_text   TEXT,
             attachment_id TEXT,
             mime          TEXT,
             bytes         INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (event_id, role, ordinal)
         );
         CREATE TABLE IF NOT EXISTS clipboard_attachments (
             id                 TEXT PRIMARY KEY,
             event_id           TEXT NOT NULL REFERENCES clipboard_events(id) ON DELETE CASCADE,
             ordinal            INTEGER NOT NULL,
             kind               TEXT NOT NULL,
             path               TEXT NOT NULL,
             name               TEXT NOT NULL,
             mime               TEXT,
             bytes              INTEGER NOT NULL DEFAULT 0,
             depends_on_source  INTEGER NOT NULL DEFAULT 0,
             created_at         INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_clipboard_attachments_event
             ON clipboard_attachments(event_id);",
    )
    .map_err(|error| ClipboardStoreError::Database(error.to_string()))?;
    let found: Option<String> = conn
        .query_row(
            "SELECT value FROM clipboard_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| ClipboardStoreError::Database(error.to_string()))?;
    match found {
        None => {
            conn.execute(
                "INSERT INTO clipboard_meta (key, value) VALUES ('schema_version', ?1)",
                params![SCHEMA_VERSION.to_string()],
            )
            .map_err(|error| ClipboardStoreError::Database(error.to_string()))?;
        }
        Some(value) => {
            let version: i64 = value.parse().unwrap_or(-1);
            if version > SCHEMA_VERSION {
                return Err(ClipboardStoreError::Corrupt(format!(
                    "数据库版本 {version} 高于本应用支持的 {SCHEMA_VERSION}，请升级 Flashcast"
                )));
            }
        }
    }
    Ok(())
}

fn event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ClipboardEvent> {
    let app_id: Option<String> = row.get("source_app_id")?;
    let title: Option<String> = row.get("source_title")?;
    Ok(ClipboardEvent {
        id: row.get("id")?,
        captured_at_ms: row.get("captured_at")?,
        content_hash: row.get("content_hash")?,
        summary: row.get("summary")?,
        text: row.get("text_content")?,
        formats: Vec::new(),
        attachments: Vec::new(),
        payloads: Vec::new(),
        source: app_id.map(|app_id| ClipboardSourceApp { app_id, title }),
        pinned: row.get::<_, i64>("pinned")? != 0,
        copies: row.get::<_, i64>("copies")?.max(0) as u32,
    })
}

/// 读出事件的全部从表内容。
fn hydrate(conn: &Connection, event: &mut ClipboardEvent) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT kind, bytes, mime, width, height, item_count, names
         FROM clipboard_formats WHERE event_id = ?1 ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map(params![event.id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, Option<i64>>(3)?,
            row.get::<_, Option<i64>>(4)?,
            row.get::<_, Option<i64>>(5)?,
            row.get::<_, Option<String>>(6)?,
        ))
    })?;
    for row in rows {
        let (kind, bytes, mime, width, height, item_count, names) = row?;
        event.formats.push(format_from_parts(
            kind, bytes, mime, width, height, item_count, names,
        ));
    }

    let mut stmt = conn.prepare(
        "SELECT role, inline_text, attachment_id, mime, bytes
         FROM clipboard_payloads WHERE event_id = ?1 ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map(params![event.id], |row| {
        Ok(ClipboardPayload {
            role: PayloadRole::from_tag(&row.get::<_, String>(0)?),
            inline: row.get(1)?,
            attachment_id: row.get(2)?,
            mime: row.get(3)?,
            bytes: row.get::<_, i64>(4)?.max(0) as usize,
        })
    })?;
    for row in rows {
        event.payloads.push(row?);
    }

    let mut stmt = conn.prepare(
        "SELECT id, kind, path, name, mime, bytes, depends_on_source, created_at
         FROM clipboard_attachments WHERE event_id = ?1 ORDER BY ordinal ASC",
    )?;
    let rows = stmt.query_map(params![event.id], |row| {
        Ok(ClipboardAttachment {
            id: row.get(0)?,
            kind: AttachmentKind::from_tag(&row.get::<_, String>(1)?),
            path: PathBuf::from(row.get::<_, String>(2)?),
            name: row.get(3)?,
            mime: row.get(4)?,
            bytes: row.get::<_, i64>(5)?.max(0) as u64,
            depends_on_source: row.get::<_, i64>(6)? != 0,
            created_at_ms: row.get(7)?,
        })
    })?;
    for row in rows {
        event.attachments.push(row?);
    }
    Ok(())
}

fn format_from_parts(
    kind: String,
    bytes: i64,
    mime: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    item_count: Option<i64>,
    names: Option<String>,
) -> ClipboardFormat {
    let bytes = bytes.max(0) as usize;
    match kind.as_str() {
        "text" => ClipboardFormat::Text { bytes },
        "html" => ClipboardFormat::Html { bytes },
        "rtf" => ClipboardFormat::Rtf { bytes },
        "image" => ClipboardFormat::Image {
            mime: mime.unwrap_or_else(|| "image/png".to_string()),
            width: width.unwrap_or(0).max(0) as u32,
            height: height.unwrap_or(0).max(0) as u32,
            bytes,
        },
        "files" => ClipboardFormat::Files {
            count: item_count.unwrap_or(0).max(0) as usize,
            names: names
                .and_then(|names| serde_json::from_str(&names).ok())
                .unwrap_or_default(),
        },
        other => ClipboardFormat::Other {
            name: other.to_string(),
            bytes,
        },
    }
}

fn existing_by_hash(conn: &Connection, hash: &str) -> rusqlite::Result<Option<(String, u32)>> {
    conn.query_row(
        "SELECT id, copies FROM clipboard_events WHERE content_hash = ?1
         ORDER BY captured_at DESC LIMIT 1",
        params![hash],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?.max(0) as u32,
            ))
        },
    )
    .optional()
}

fn insert_event(conn: &Connection, event: &ClipboardEvent) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO clipboard_events
             (id, captured_at, content_hash, summary, text_content, source_app_id, source_title,
              pinned, pinned_at, copies)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            event.id,
            event.captured_at_ms,
            event.content_hash,
            event.summary,
            event.text,
            event.source.as_ref().map(|source| source.app_id.clone()),
            event
                .source
                .as_ref()
                .and_then(|source| source.title.clone()),
            if event.pinned { 1 } else { 0 },
            Option::<i64>::None,
            i64::from(event.copies.max(1)),
        ],
    )?;
    insert_formats(conn, event)?;
    insert_payloads(conn, event)?;
    insert_attachments(conn, event)?;
    Ok(())
}

fn insert_formats(conn: &Connection, event: &ClipboardEvent) -> rusqlite::Result<()> {
    for (ordinal, format) in event.formats.iter().enumerate() {
        let (mime, width, height, item_count, names) = match format {
            ClipboardFormat::Image {
                mime,
                width,
                height,
                ..
            } => (
                Some(mime.clone()),
                Some(i64::from(*width)),
                Some(i64::from(*height)),
                None,
                None,
            ),
            ClipboardFormat::Files { count, names } => (
                None,
                None,
                None,
                Some(*count as i64),
                serde_json::to_string(names).ok(),
            ),
            _ => (None, None, None, None, None),
        };
        conn.execute(
            "INSERT INTO clipboard_formats
                 (event_id, ordinal, kind, bytes, mime, width, height, item_count, names)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                event.id,
                ordinal as i64,
                format.tag(),
                format.bytes() as i64,
                mime,
                width,
                height,
                item_count,
                names,
            ],
        )?;
    }
    Ok(())
}

fn insert_payloads(conn: &Connection, event: &ClipboardEvent) -> rusqlite::Result<()> {
    for (ordinal, payload) in event.payloads.iter().enumerate() {
        conn.execute(
            "INSERT INTO clipboard_payloads
                 (event_id, role, ordinal, inline_text, attachment_id, mime, bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                event.id,
                payload.role.tag(),
                ordinal as i64,
                payload.inline,
                payload.attachment_id,
                payload.mime,
                payload.bytes as i64,
            ],
        )?;
    }
    Ok(())
}

fn insert_attachments(conn: &Connection, event: &ClipboardEvent) -> rusqlite::Result<()> {
    for (ordinal, attachment) in event.attachments.iter().enumerate() {
        conn.execute(
            "INSERT INTO clipboard_attachments
                 (id, event_id, ordinal, kind, path, name, mime, bytes, depends_on_source,
                  created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                attachment.id,
                event.id,
                ordinal as i64,
                attachment.kind.tag(),
                attachment.path.to_string_lossy(),
                attachment.name,
                attachment.mime,
                attachment.bytes as i64,
                if attachment.depends_on_source { 1 } else { 0 },
                attachment.created_at_ms,
            ],
        )?;
    }
    Ok(())
}

/// 去重命中时，把后一次复制带来的富文本载荷补进已有条目。
///
/// 场景：用户先复制了纯文本版本，之后又从富文本应用复制了**同一段文字**。去重把它们
/// 合成一条（同一条历史，不出现重复记录），但如果什么都不做，刚捕获到的 HTML/RTF 就
/// 白丢了。这里只在已有条目**还没有任何载荷**、而新事件有时才补写，因此：
///
/// - 不会用后一次的载荷覆盖已有载荷（先到的富文本版本保留）；
/// - 不会改变去重键（仍是纯文本指纹），纯文本条目的去重语义完全不变。
fn enrich_payloads(conn: &Connection, id: &str, event: &ClipboardEvent) -> rusqlite::Result<()> {
    if event.payloads.is_empty() {
        return Ok(());
    }
    let existing: i64 = conn.query_row(
        "SELECT COUNT(*) FROM clipboard_payloads WHERE event_id = ?1",
        params![id],
        |row| row.get(0),
    )?;
    if existing > 0 {
        return Ok(());
    }
    // 从表用已有条目的 id 重建，格式集合与载荷因此一起更新（副标题也要显示 HTML/RTF）。
    conn.execute(
        "DELETE FROM clipboard_formats WHERE event_id = ?1",
        params![id],
    )?;
    let mut stored = event.clone();
    stored.id = id.to_string();
    insert_formats(conn, &stored)?;
    insert_payloads(conn, &stored)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (ClipboardStore, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("临时目录");
        let store = ClipboardStore::open(dir.path().join(CLIPBOARD_DIR));
        (store, dir)
    }

    fn text_event(id: &str, text: &str, at: i64) -> ClipboardEvent {
        ClipboardEvent {
            id: id.to_string(),
            captured_at_ms: at,
            content_hash: content_hash_text(text),
            summary: summary_for_text(text),
            text: Some(text.to_string()),
            formats: vec![ClipboardFormat::Text { bytes: text.len() }],
            attachments: Vec::new(),
            payloads: Vec::new(),
            source: Some(ClipboardSourceApp {
                app_id: "firefox".to_string(),
                title: Some("Mozilla Firefox".to_string()),
            }),
            pinned: false,
            copies: 1,
        }
    }

    #[test]
    fn text_event_round_trips_with_every_field() {
        let (store, _dir) = store();
        let event = text_event("clip-1", "第一行\n第二行", 1_700_000_000_000);
        assert!(matches!(
            store.insert(&event, 100).expect("插入"),
            InsertOutcome::Inserted { .. }
        ));
        let found = store.find("clip-1").expect("查询").expect("必须存在");
        assert_eq!(found, event, "写进去的一整条必须原样读回来");
        assert_eq!(found.summary, "第一行 第二行");
        assert_eq!(
            found.source.as_ref().map(|s| s.app_id.as_str()),
            Some("firefox")
        );
    }

    /// tickets 10–12 的字段从 ticket 09 起就在 schema 里：图片尺寸、文件名称、
    /// HTML 载荷、附件引用都能往返，因此它们**不需要迁移**。
    #[test]
    fn media_and_file_fields_round_trip_without_migration() {
        let (store, _dir) = store();
        let event = ClipboardEvent {
            id: "clip-media".to_string(),
            captured_at_ms: 1_700_000_000_500,
            content_hash: "blob:image-1".to_string(),
            summary: "图片 800×600".to_string(),
            text: None,
            formats: vec![
                ClipboardFormat::Image {
                    mime: "image/png".to_string(),
                    width: 800,
                    height: 600,
                    bytes: 4096,
                },
                ClipboardFormat::Files {
                    count: 2,
                    names: vec!["a.txt".to_string(), "b.txt".to_string()],
                },
                ClipboardFormat::Html { bytes: 21 },
            ],
            attachments: vec![
                ClipboardAttachment {
                    id: "att-1".to_string(),
                    kind: AttachmentKind::Image,
                    path: PathBuf::from("/tmp/att-1.png"),
                    name: "截图.png".to_string(),
                    mime: Some("image/png".to_string()),
                    bytes: 4096,
                    depends_on_source: false,
                    created_at_ms: 1_700_000_000_500,
                },
                ClipboardAttachment {
                    id: "att-2".to_string(),
                    kind: AttachmentKind::FileReference,
                    path: PathBuf::from("/home/user/报告.pdf"),
                    name: "报告.pdf".to_string(),
                    mime: None,
                    bytes: 10,
                    depends_on_source: true,
                    created_at_ms: 1_700_000_000_501,
                },
            ],
            payloads: vec![ClipboardPayload {
                role: PayloadRole::Html,
                inline: Some("<b>加粗</b>".to_string()),
                attachment_id: None,
                mime: Some("text/html".to_string()),
                bytes: 21,
            }],
            source: None,
            pinned: true,
            copies: 1,
        };
        store.insert(&event, 100).expect("插入");
        let found = store.find("clip-media").expect("查询").expect("必须存在");
        assert_eq!(found, event);
        // 未知格式标签也不会丢数据（前向兼容）。
        let unknown = ClipboardFormat::Other {
            name: "application/x-future".to_string(),
            bytes: 7,
        };
        let mut event = event;
        event.id = "clip-future".to_string();
        event.content_hash = "future:1".to_string();
        event.formats = vec![unknown.clone()];
        // 附件标识是全库主键：另一条事件要给它自己的附件。
        event.attachments.clear();
        event.payloads.clear();
        store.insert(&event, 100).expect("插入");
        let found = store.find("clip-future").expect("查询").expect("必须存在");
        assert_eq!(found.formats, vec![unknown]);
    }

    #[test]
    fn duplicate_content_updates_one_entry_and_counts_copies() {
        let (store, _dir) = store();
        store
            .insert(&text_event("clip-1", "同一段内容", 100), 100)
            .expect("插入");
        let outcome = store
            .insert(&text_event("clip-2", "同一段内容", 200), 100)
            .expect("插入");
        assert_eq!(
            outcome,
            InsertOutcome::Deduplicated {
                id: "clip-1".to_string(),
                copies: 2
            }
        );
        assert_eq!(store.stats().expect("统计").total, 1);
        let found = store.find("clip-1").expect("查询").expect("必须存在");
        assert_eq!(found.captured_at_ms, 200, "去重要把已有条目提到最新");
        assert_eq!(found.copies, 2);
        assert!(store.find("clip-2").expect("查询").is_none());
    }

    #[test]
    fn capacity_evicts_oldest_unpinned_then_refuses_when_all_pinned() {
        let (store, _dir) = store();
        store
            .insert(&text_event("clip-1", "一", 100), 2)
            .expect("插入");
        store
            .insert(&text_event("clip-2", "二", 200), 2)
            .expect("插入");
        // 第三条：容量 2，最旧的一条被回收。
        store
            .insert(&text_event("clip-3", "三", 300), 2)
            .expect("插入");
        assert_eq!(store.stats().expect("统计").total, 2);
        assert!(store.find("clip-1").expect("查询").is_none());
        // 两条都置顶后：容量已满且无可回收，如实拒绝。
        assert!(store.set_pinned("clip-2", true).expect("置顶"));
        assert!(store.set_pinned("clip-3", true).expect("置顶"));
        assert_eq!(
            store
                .insert(&text_event("clip-4", "四", 400), 2)
                .expect("插入"),
            InsertOutcome::CapacityReached {
                entries: 2,
                capacity: 2
            }
        );
        assert!(store.find("clip-4").expect("查询").is_none());
    }

    #[test]
    fn reclaim_removes_expired_and_over_capacity_entries_and_orphan_files() {
        let (store, _dir) = store();
        let now = 10_000_000_000_000i64;
        store
            .insert(&text_event("clip-old", "过期", now - 10 * 86_400_000), 100)
            .expect("插入");
        store
            .insert(&text_event("clip-new", "新鲜", now), 100)
            .expect("插入");
        store.set_pinned("clip-old", true).expect("置顶");
        // 置顶条目不受保留期限影响。
        let report = store.reclaim(3, 100, now).expect("回收");
        assert_eq!(report.expired, 0, "置顶条目不会被保留期限回收");
        assert_eq!(report.remaining, 2);
        assert!(store.set_pinned("clip-old", false).expect("取消置顶"));
        let report = store.reclaim(3, 100, now).expect("回收");
        assert_eq!(report.expired, 1);
        assert_eq!(report.remaining, 1);

        // 附件文件：没有被任何条目引用时在回收中删除。
        let attachments = store.attachments_dir();
        std::fs::create_dir_all(&attachments).expect("附件目录");
        let orphan = attachments.join("orphan.bin");
        std::fs::write(&orphan, b"x").expect("写入附件");
        let report = store.reclaim(3, 100, now).expect("回收");
        assert_eq!(report.attachments, 1);
        assert!(!orphan.exists(), "不再被引用的附件必须被回收");
    }

    #[test]
    fn storage_failure_is_reported_and_recovers_when_fixed() {
        let dir = tempfile::tempdir().expect("临时目录");
        // 用一个普通文件占住目录路径：create_dir_all 会失败。
        let blocked = dir.path().join(CLIPBOARD_DIR);
        std::fs::write(&blocked, b"not a directory").expect("写入占位文件");
        let store = ClipboardStore::open(&blocked);
        assert!(!store.is_available());
        assert!(store.storage_error().is_some());
        let error = store.insert(&text_event("clip-1", "正文", 1), 100);
        assert!(matches!(error, Err(ClipboardStoreError::Unavailable(_))));
        // 修好之后下一次访问会自己恢复，不需要重建宿主。
        std::fs::remove_file(&blocked).expect("移除占位文件");
        assert!(store.is_available(), "重试打开成功后必须恢复可用");
        store
            .insert(&text_event("clip-1", "正文", 1), 100)
            .expect("插入");
    }

    #[test]
    fn list_searches_text_summary_and_source_and_orders_pinned_first() {
        let (store, _dir) = store();
        store
            .insert(&text_event("clip-1", "苹果 香蕉", 100), 100)
            .expect("插入");
        store
            .insert(&text_event("clip-2", "橘子", 200), 100)
            .expect("插入");
        assert_eq!(store.list(None, 10).expect("列表").len(), 2);
        let hits = store.list(Some("香蕉"), 10).expect("搜索");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].id, "clip-1");
        // 来源应用也参与检索。
        let hits = store.list(Some("firefox"), 10).expect("搜索");
        assert_eq!(hits.len(), 2);
        // 百分号不是通配符。
        assert!(store.list(Some("%"), 10).expect("搜索").is_empty());
        store.set_pinned("clip-1", true).expect("置顶");
        assert_eq!(store.list(None, 10).expect("列表")[0].id, "clip-1");
    }
}

// ---------------------------------------------------------------------------
// 后台捕获运行时
// ---------------------------------------------------------------------------

/// 一次捕获尝试的结果。每一种都必须能被调用方区分：
/// 「没有新内容」「暂停了」「存进去了」「被自己抑制了」「容量满了」「失败了」是六件事。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardCaptureOutcome {
    /// 插件未启用：不轮询、不保存（spec「停用插件停止后台活动」）。
    Disabled,
    /// 没有新的复制事件。
    Unchanged,
    /// 有新的复制事件，但内容不是本版本能保存的（例如只有非文本格式）。
    Uncapturable,
    /// 正在暂停记录：轮询照常推进，但不保存。
    Paused,
    /// 已保存为新条目。
    Captured { id: String, summary: String },
    /// 同一内容已经存在：合并到已有条目。
    Deduplicated { id: String, copies: u32 },
    /// 这次写入来自 Flashcast 自己：丢弃，避免自身写入循环。
    Suppressed,
    /// 容量已满且全部是置顶条目：如实拒绝。
    CapacityReached { entries: usize, capacity: usize },
    /// 读取或存储失败，附中文原因。
    Failed { message: String },
}

/// 面向 UI 与诊断的剪贴板历史状态。
///
/// 每个字段都必须如实反映当前情况：存储失败、容量触顶、捕获失败各有自己的字段，
/// 不能都折成「正常」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardState {
    /// 插件是否已启用（用户在清单里打开）。
    pub enabled: bool,
    /// 是否暂停记录。
    pub paused: bool,
    /// 后台捕获线程是否正在运行。
    pub capture_active: bool,
    /// 存储是否可用。
    pub storage_ok: bool,
    /// 存储失败的中文原因。
    pub storage_error: Option<String>,
    /// 本机数据库路径（设备本地目录，不在配置工作区里）。
    pub storage_path: PathBuf,
    pub entries: usize,
    pub pinned: usize,
    pub attachments: usize,
    pub capacity: usize,
    pub retention_days: u32,
    /// 容量已满的说明；`None` 表示没有触顶。
    pub capacity_reached: Option<String>,
    /// 最近一次捕获或存储失败的中文原因。
    pub last_error: Option<String>,
    /// 最近一次成功保存的时间（Unix 毫秒）。
    pub last_capture_ms: Option<i64>,
    /// 被自身写入抑制丢弃的次数。
    pub suppressed: u64,
}

/// 后台捕获线程的观测快照。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClipboardRuntimeSnapshot {
    pub paused: bool,
    pub retention_days: u32,
    pub capacity: usize,
    /// 最近一次失败的中文原因（成功一次即清空）。
    pub last_error: Option<String>,
    /// 最近一次成功保存的时间（Unix 毫秒）。
    pub last_capture_ms: Option<i64>,
    /// 被自身写入抑制丢弃的次数。
    pub suppressed: u64,
    /// 容量已满的说明；`None` 表示没有触顶。
    pub capacity_reached: Option<String>,
}

struct ClipboardRuntimeState {
    paused: bool,
    retention_days: u32,
    capacity: usize,
    last_error: Option<String>,
    last_capture_ms: Option<i64>,
    suppressed: u64,
    capacity_reached: Option<String>,
    /// 自身写入的内容指纹，一次性消费。
    own_writes: Vec<String>,
}

impl Default for ClipboardRuntimeState {
    fn default() -> Self {
        Self {
            paused: false,
            retention_days: 0,
            capacity: 0,
            last_error: None,
            last_capture_ms: None,
            suppressed: 0,
            capacity_reached: None,
            own_writes: Vec::new(),
        }
    }
}

/// 后台捕获线程的句柄。
struct ClipboardPump {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl ClipboardPump {
    fn stop_and_join(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::SeqCst);
        // 先标记为「已停止」再 join：`is_running()` 不必等线程退出就已经准确。
        self.running
            .store(false, std::sync::atomic::Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// 剪贴板后台捕获运行时：把「轮询 → 去重 → 自身写入抑制 → 落库 → 回收」这条管线
/// 集中在一处，宿主与后台线程共用同一个实例。
///
/// 线程只持有这个运行时（不持有 `Host`），因此不会反过来调用宿主方法，
/// 也就不可能在与宿主锁的交互中形成死锁。
pub struct ClipboardRuntime {
    store: Arc<ClipboardStore>,
    watcher: Arc<dyn flashcast_platform::clipboard::ClipboardWatcher>,
    poll_interval: Duration,
    state: Mutex<ClipboardRuntimeState>,
    pump: Mutex<Option<ClipboardPump>>,
}

impl ClipboardRuntime {
    pub fn new(
        store: Arc<ClipboardStore>,
        watcher: Arc<dyn flashcast_platform::clipboard::ClipboardWatcher>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            store,
            watcher,
            poll_interval,
            state: Mutex::new(ClipboardRuntimeState::default()),
            pump: Mutex::new(None),
        }
    }

    pub fn store(&self) -> &Arc<ClipboardStore> {
        &self.store
    }

    /// 更新暂停开关、保留期限与容量。每次成功保存后按它们回收。
    pub fn configure(&self, paused: bool, retention_days: u32, capacity: usize) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.paused = paused;
        state.retention_days = retention_days;
        state.capacity = capacity;
    }

    pub fn snapshot(&self) -> ClipboardRuntimeSnapshot {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        ClipboardRuntimeSnapshot {
            paused: state.paused,
            retention_days: state.retention_days,
            capacity: state.capacity,
            last_error: state.last_error.clone(),
            last_capture_ms: state.last_capture_ms,
            suppressed: state.suppressed,
            capacity_reached: state.capacity_reached.clone(),
        }
    }

    /// 登记一次由 Flashcast 自己发起的写入。
    ///
    /// 两层抑制：告知适配层（它按平台的变更序号/指纹跳过），并在宿主侧按内容指纹
    /// 兜底消费一次。两层各自成立：适配层失效时宿主仍然不会把自身写入收成新条目。
    pub fn note_own_write(&self, text: &str) {
        self.watcher.note_own_write(text);
        if text.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let hash = content_hash_text(text);
        state.own_writes.push(hash);
        // 只保留最近若干次：抑制窗口不需要无限长，太多会误伤用户随后真的复制同样内容。
        const MAX_REMEMBERED: usize = 8;
        if state.own_writes.len() > MAX_REMEMBERED {
            let excess = state.own_writes.len() - MAX_REMEMBERED;
            state.own_writes.drain(..excess);
        }
    }

    /// 登记一次由 Flashcast 自己发起的**文件列表**写入（与 [`Self::note_own_write`] 对称）。
    ///
    /// 兜底一层用 [`content_hash_files`]，与 `event_from_capture` 算出的键完全一致，
    /// 因此适配层没有抑制成功时宿主仍然不会把自己写入的文件列表收成新条目。
    pub fn note_own_write_files(&self, paths: &[std::path::PathBuf]) {
        self.watcher.note_own_write_files(paths);
        if paths.is_empty() {
            return;
        }
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.own_writes.push(content_hash_files(paths));
        const MAX_REMEMBERED: usize = 8;
        if state.own_writes.len() > MAX_REMEMBERED {
            let excess = state.own_writes.len() - MAX_REMEMBERED;
            state.own_writes.drain(..excess);
        }
    }

    /// 轮询一次并处理结果。暂停时仍然轮询（推进适配层的变更序号），只是不保存。
    pub fn capture_once(&self) -> ClipboardCaptureOutcome {
        // 平台调用不能在持有状态锁时进行：读取可能阻塞数秒。
        let poll = self.watcher.poll();
        let (paused, capacity, retention_days) = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            (state.paused, state.capacity, state.retention_days)
        };
        let capture = match poll {
            Err(error) => {
                let message = error.to_string();
                self.record_failure(message.clone());
                return ClipboardCaptureOutcome::Failed { message };
            }
            Ok(flashcast_platform::clipboard::ClipboardPoll::Unchanged) => {
                return ClipboardCaptureOutcome::Unchanged
            }
            Ok(flashcast_platform::clipboard::ClipboardPoll::Changed(capture)) => capture,
        };
        if paused {
            return ClipboardCaptureOutcome::Paused;
        }
        // 有图片但无法保存（超大 / 头部损坏 / 格式不支持）：如实报一次失败，绝不生成
        // 一条「看起来保存成功、实际恢复不了」的历史（spec 用户故事 53）。
        let attachments_dir = self.store.attachments_dir();
        let Some(event) = event_from_capture(&capture, now_ms(), &attachments_dir) else {
            if let Some(problem) = capture.image_problem.clone() {
                self.record_failure(problem.clone());
                return ClipboardCaptureOutcome::Failed { message: problem };
            }
            return ClipboardCaptureOutcome::Uncapturable;
        };
        if self.consume_own_write(&event.content_hash) {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.suppressed += 1;
            return ClipboardCaptureOutcome::Suppressed;
        }
        match self.store.insert(&event, capacity) {
            Ok(InsertOutcome::Inserted { id }) => {
                // 图片字节必须真的落盘，而且必须在**入库成功之后**写：写不进去时把刚插入
                // 的条目删掉并如实报错，而不是留下一条恢复不了的历史。
                if let (Some(image), Some(attachment)) =
                    (capture.image.as_ref(), event.image_attachment())
                {
                    if let Err(error) = self.store.write_attachment(&attachment.path, &image.bytes)
                    {
                        let message = format!("无法保存图片附件：{error}");
                        let _ = self.store.delete(&event.id);
                        self.record_failure(message.clone());
                        return ClipboardCaptureOutcome::Failed { message };
                    }
                }
                let summary = event.summary.clone();
                self.after_insert(capacity, retention_days);
                ClipboardCaptureOutcome::Captured { id, summary }
            }
            Ok(InsertOutcome::Deduplicated { id, copies }) => {
                self.after_insert(capacity, retention_days);
                ClipboardCaptureOutcome::Deduplicated { id, copies }
            }
            Ok(InsertOutcome::CapacityReached { entries, capacity }) => {
                let message = format!(
                    "剪贴板历史已满（{entries}/{capacity}，剩下的都是置顶条目），这次复制不会被保存"
                );
                let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
                state.capacity_reached = Some(message);
                ClipboardCaptureOutcome::CapacityReached { entries, capacity }
            }
            Err(error) => {
                let message = error.to_string();
                self.record_failure(message.clone());
                ClipboardCaptureOutcome::Failed { message }
            }
        }
    }

    /// 成功保存之后：更新观测值，并按保留期限与容量回收。
    fn after_insert(&self, capacity: usize, retention_days: u32) {
        let report = self.store.reclaim(retention_days, capacity, now_ms());
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.last_capture_ms = Some(now_ms());
        state.last_error = None;
        match report {
            Ok(report) => {
                state.capacity_reached = if report.capacity_reached {
                    Some(format!(
                        "剪贴板历史已满（{}/{}，全部为置顶条目）",
                        report.remaining, capacity
                    ))
                } else {
                    None
                };
            }
            Err(error) => state.last_error = Some(error.to_string()),
        }
    }

    fn record_failure(&self, message: String) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        state.last_error = Some(message);
    }

    fn consume_own_write(&self, hash: &str) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        match state.own_writes.iter().position(|item| item == hash) {
            Some(index) => {
                state.own_writes.remove(index);
                true
            }
            None => false,
        }
    }

    /// 启动后台轮询。已经在跑时是空操作。
    pub fn start(self: &Arc<Self>) {
        let mut pump = self.pump.lock().unwrap_or_else(|p| p.into_inner());
        if pump
            .as_ref()
            .map(|pump| pump.running.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
        {
            return;
        }
        // 取出旧句柄并在锁外结束它：join 可能等一个轮询周期。
        let previous = pump.take();
        drop(pump);
        if let Some(mut previous) = previous {
            previous.stop_and_join();
        }

        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let runtime = Arc::clone(self);
        let thread_stop = std::sync::Arc::clone(&stop);
        let thread_running = std::sync::Arc::clone(&running);
        let spawned = std::thread::Builder::new()
            .name("flashcast-clipboard-capture".to_string())
            .spawn(move || {
                while !thread_stop.load(std::sync::atomic::Ordering::SeqCst) {
                    let outcome = runtime.capture_once();
                    if let ClipboardCaptureOutcome::Failed { message } = &outcome {
                        // 失败已经记进状态；这里只留一条诊断线索，不重复上报。
                        let _ = message;
                    }
                    // 分片睡眠：停止请求最多等一个分片就能生效。
                    let step = Duration::from_millis(20);
                    let mut waited = Duration::ZERO;
                    while waited < runtime.poll_interval
                        && !thread_stop.load(std::sync::atomic::Ordering::SeqCst)
                    {
                        std::thread::sleep(step);
                        waited += step;
                    }
                }
                thread_running.store(false, std::sync::atomic::Ordering::SeqCst);
            });
        match spawned {
            Ok(handle) => {
                *self.pump.lock().unwrap_or_else(|p| p.into_inner()) = Some(ClipboardPump {
                    stop,
                    running,
                    handle: Some(handle),
                });
            }
            Err(error) => {
                let message = format!("无法启动剪贴板后台捕获线程：{error}");
                self.record_failure(message);
            }
        }
    }

    /// 停止后台轮询并等待线程退出（最多一个轮询周期）。没有在跑时是空操作。
    pub fn stop(&self) {
        // 先把句柄从互斥量里取出来，再在锁外 join：避免长时间持锁。
        let pump = self.pump.lock().unwrap_or_else(|p| p.into_inner()).take();
        if let Some(mut pump) = pump {
            pump.stop_and_join();
        }
    }

    /// 后台轮询是否正在运行。
    pub fn is_running(&self) -> bool {
        self.pump
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .as_ref()
            .map(|pump| pump.running.load(std::sync::atomic::Ordering::SeqCst))
            .unwrap_or(false)
    }
}
