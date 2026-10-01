//! 备忘录：稳定标识、标题、多个标签与文字正文（GLOSSARY「备忘录」）。
//!
//! ## 工作区格式
//!
//! 每条备忘录是配置工作区 `memos/<id>.md` 下的一个文件，**人类可读的 Markdown**，
//! 带 front matter（ADR §8、spec「配置工作区保存……带标签的 Markdown 备忘录」）：
//!
//! ```markdown
//! ---
//! id: memo-1927f3a1c04-1
//! title: 常用回复
//! tags: [回复, 工作]
//! ---
//!
//! 收到，我看一下再回复你。
//! ```
//!
//! 设计取舍：
//!
//! - **手写友好**：没有 front matter 的普通 Markdown 也接受（标题取第一行 `#` 标题，
//!   或文件名），这样用户可以先用编辑器写下内容再交给应用；第一次经应用保存时
//!   会补上 front matter。
//! - **错误可见**：front matter 缺少结束标记、出现不认识的键、标识非法时返回中文原因，
//!   该文件不会被纳入生效内容，也不会被静默覆盖（[`MemoProblem`] 如实报告）。
//! - **写入原子**：文件由宿主经 [`crate::workspace::write_atomic`] 写入并登记自写抑制，
//!   与其它工作区文件一致，可被 ticket 15 的提交流程纳入提交。

use std::path::{Path, PathBuf};
use std::sync::RwLock;

use crate::watch::hash_bytes;
use crate::workspace::WorkspaceError;

/// 备忘录文件的扩展名。
pub const MEMO_EXTENSION: &str = "md";

/// front matter 的分隔线。
const FRONT_MATTER: &str = "---";

/// 一条备忘录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Memo {
    /// 稳定标识，跨查询与重启一致；同时是文件名 `<id>.md`。
    pub id: String,
    pub title: String,
    /// 多个标签。顺序稳定，重复项在解析时去掉。
    pub tags: Vec<String>,
    /// 文字正文。
    pub body: String,
}

impl Memo {
    /// 序列化为人类可读的 Markdown（front matter + 正文）。
    pub fn to_markdown(&self) -> String {
        let mut text = String::new();
        text.push_str(FRONT_MATTER);
        text.push('\n');
        text.push_str(&format!("id: {}\n", self.id));
        text.push_str(&format!("title: {}\n", self.title));
        text.push_str(&format!("tags: [{}]\n", self.tags.join(", ")));
        text.push_str(FRONT_MATTER);
        text.push('\n');
        if !self.body.is_empty() {
            text.push('\n');
            text.push_str(&self.body);
            if !self.body.ends_with('\n') {
                text.push('\n');
            }
        }
        text
    }

    /// 参与「已应用内容」比对的指纹：文件里的空白差异不算一次重新加载。
    pub fn fingerprint(&self) -> String {
        format!(
            "{}\u{1}{}\u{1}{}\u{1}{}",
            self.id,
            self.title,
            self.tags.join("\u{2}"),
            self.body
        )
    }
}

/// 一份无法读取的备忘录文件。宿主保留可用内容，并如实报告原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoProblem {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MemoError {
    #[error("备忘录文件无效：{0}")]
    Invalid(String),
    #[error("备忘录文件读写失败：{0}")]
    Io(String),
    #[error("尚未关联配置工作区，无法保存备忘录")]
    NoWorkspace,
    #[error("备忘录插件已停用，无法创建或修改备忘录")]
    PluginDisabled,
    #[error("找不到这条备忘录：{0}")]
    NotFound(String),
}

impl From<WorkspaceError> for MemoError {
    fn from(error: WorkspaceError) -> Self {
        MemoError::Io(error.to_string())
    }
}

impl From<std::io::Error> for MemoError {
    fn from(error: std::io::Error) -> Self {
        MemoError::Io(error.to_string())
    }
}

/// 备忘录标识是否合法，同时用于文件名安全：小写字母、数字、点、下划线与连字符，
/// 不得为空、不得以点开头、不得包含路径分隔符（不能借标识穿越目录）。
pub fn is_valid_memo_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 96
        && !id.starts_with('.')
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// 生成一个新的稳定标识。时间戳保证可读与大致有序，进程内计数器保证并发下唯一。
pub fn new_memo_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos() as u64)
        .unwrap_or(0);
    format!("memo-{nanos:x}-{counter:x}")
}

/// 从 Markdown 文本解析一条备忘录。`file_stem` 是文件名（不含扩展名），
/// 用作缺少 front matter 且没有 `#` 标题时的标题。
pub fn parse_markdown(file_stem: &str, text: &str) -> Result<Memo, MemoError> {
    let normalized = text.replace("\r\n", "\n");
    match split_front_matter(&normalized)? {
        Some((front, body)) => {
            let mut id: Option<String> = None;
            let mut title: Option<String> = None;
            let mut tags: Vec<String> = Vec::new();
            for (index, line) in front.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let (key, value) = line.split_once(':').ok_or_else(|| {
                    MemoError::Invalid(format!(
                        "front matter 第 {} 行缺少「键: 值」结构",
                        index + 1
                    ))
                })?;
                let value = value.trim();
                match key.trim() {
                    "id" => id = Some(unquote(value)),
                    "title" => title = Some(unquote(value)),
                    "tags" => tags = parse_tags(value),
                    other => {
                        return Err(MemoError::Invalid(format!(
                            "front matter 出现不认识的键「{other}」：只支持 id、title、tags"
                        )))
                    }
                }
            }
            let id = match id {
                Some(id) => {
                    if !is_valid_memo_id(&id) {
                        return Err(MemoError::Invalid(format!(
                            "标识「{id}」不合法：只能使用字母、数字、点、下划线与连字符，且不能以点开头"
                        )));
                    }
                    id
                }
                None => {
                    let derived = file_stem.to_string();
                    if !is_valid_memo_id(&derived) {
                        return Err(MemoError::Invalid(format!(
                            "缺少 id 字段，且文件名「{file_stem}」不能作为标识"
                        )));
                    }
                    derived
                }
            };
            let title = match title {
                Some(title) if !title.trim().is_empty() => title,
                _ => file_stem.to_string(),
            };
            Ok(Memo {
                id,
                title,
                tags,
                body: body.to_string(),
            })
        }
        // 没有 front matter：普通 Markdown。标题取第一行 ATX 一级标题，正文去掉它。
        None => {
            let (title, body) = split_leading_heading(&normalized, file_stem);
            Ok(Memo {
                id: file_stem.to_string(),
                title,
                tags: Vec::new(),
                body,
            })
        }
    }
}

/// 分隔 front matter 与正文。返回 `None` 表示没有 front matter。
fn split_front_matter(text: &str) -> Result<Option<(&str, &str)>, MemoError> {
    let Some(rest) = text.strip_prefix(FRONT_MATTER) else {
        return Ok(None);
    };
    // `---` 之后必须紧跟换行，否则是正文里的分隔线。
    let Some(rest) = rest.strip_prefix('\n') else {
        return Ok(None);
    };
    let Some(end) = rest.find(&format!("\n{FRONT_MATTER}")) else {
        return Err(MemoError::Invalid(
            "front matter 没有结束标记「---」".to_string(),
        ));
    };
    let front = &rest[..end];
    let after = &rest[end + 1 + FRONT_MATTER.len()..];
    // 结束标记之后依次是：该行的换行、正文前的空行分隔（`to_markdown` 写出的就是
    // 「---\n」+「\n」+ 正文 +「\n」），最后是文件末尾的换行。三者都是结构分隔而不是
    // 正文内容，必须一并去掉，否则每次「读—写」都会给正文增加一个前导换行，
    // 正文会随重启不断增长（往返不稳定）。正文自身以空行开头属于结构，不保留。
    let body = after.strip_prefix('\n').unwrap_or(after);
    let body = body.strip_prefix('\n').unwrap_or(body);
    let body = body.strip_suffix('\n').unwrap_or(body);
    Ok(Some((front, body)))
}

/// 取第一行 `# ` 标题作为标题，正文为去掉该行后的内容。
fn split_leading_heading(text: &str, fallback: &str) -> (String, String) {
    let lines: Vec<&str> = text.lines().collect();
    let Some(index) = lines.iter().position(|line| !line.trim().is_empty()) else {
        return (fallback.to_string(), String::new());
    };
    match lines[index].trim().strip_prefix("# ") {
        Some(title) if !title.trim().is_empty() => {
            let rest: Vec<&str> = lines
                .iter()
                .enumerate()
                .filter(|(line_index, _)| *line_index != index)
                .map(|(_, line)| *line)
                .collect();
            (
                title.trim().to_string(),
                rest.join("\n").trim_matches('\n').to_string(),
            )
        }
        _ => (fallback.to_string(), text.trim_matches('\n').to_string()),
    }
}

/// 解析 `tags`：接受 `[a, b]` 与 `a, b` 两种写法。
fn parse_tags(value: &str) -> Vec<String> {
    let inner = value
        .trim()
        .strip_prefix('[')
        .and_then(|rest| rest.strip_suffix(']'))
        .unwrap_or(value);
    let mut tags: Vec<String> = Vec::new();
    for tag in inner.split(',') {
        let tag = unquote(tag.trim());
        if !tag.is_empty() && !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    let unquoted = if (trimmed.starts_with('"') && trimmed.ends_with('"') && trimmed.len() >= 2)
        || (trimmed.starts_with('\'') && trimmed.ends_with('\'') && trimmed.len() >= 2)
    {
        &trimmed[1..trimmed.len() - 1]
    } else {
        trimmed
    };
    unquoted.trim().to_string()
}

/// 一次工作区读取的结果：可用的备忘录与无法读取的文件。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemoSnapshot {
    pub memos: Vec<Memo>,
    pub problems: Vec<MemoProblem>,
}

impl MemoSnapshot {
    /// 参与「已应用内容」比对的指纹。`None` 表示读取失败（要保留上一次可用内容）。
    pub fn fingerprint(&self) -> u64 {
        let joined = self
            .memos
            .iter()
            .map(Memo::fingerprint)
            .collect::<Vec<String>>()
            .join("\u{3}");
        hash_bytes(joined.as_bytes())
    }
}

/// 从目录读取全部备忘录。文件按文件名排序，保证结果与目录读取顺序无关。
///
/// 目录不存在按「空集合」处理（工作区还没有备忘录），不是错误。
pub fn read_dir(dir: &Path) -> MemoSnapshot {
    read_dir_with(dir, &|_, _| {})
}

/// 同 [`read_dir`]，但每读到一个文件就回调一次（路径与原始字节）。
///
/// 宿主用这个回调把「自己读到的内容」记进文件监听账本：macOS 的 FSEvents 会把读文件
/// 上报成修改事件（见 `crate::watch` 的模块文档）。回调在解析之前触发，因此解析失败
/// 的文件同样记账。
pub fn read_dir_with(dir: &Path, on_read: &dyn Fn(&Path, &[u8])) -> MemoSnapshot {
    let mut snapshot = MemoSnapshot::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return snapshot,
        Err(error) => {
            snapshot.problems.push(MemoProblem {
                path: dir.to_path_buf(),
                reason: format!("无法读取备忘录目录：{error}"),
            });
            return snapshot;
        }
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .map(|extension| extension.eq_ignore_ascii_case(MEMO_EXTENSION))
                    .unwrap_or(false)
        })
        .collect();
    paths.sort();
    for path in paths {
        let stem = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                snapshot.problems.push(MemoProblem {
                    path,
                    reason: format!("无法读取：{error}"),
                });
                continue;
            }
        };
        on_read(&path, &bytes);
        let text = match String::from_utf8(bytes) {
            Ok(text) => text,
            Err(error) => {
                snapshot.problems.push(MemoProblem {
                    path,
                    reason: format!("不是有效的 UTF-8 文本：{error}"),
                });
                continue;
            }
        };
        match parse_markdown(&stem, &text) {
            Ok(memo) => snapshot.memos.push(memo),
            Err(error) => snapshot.problems.push(MemoProblem {
                path,
                reason: error.to_string(),
            }),
        }
    }
    snapshot
}

/// 宿主与备忘录插件共享的当前生效内容。
///
/// 宿主只在成功写入工作区之后调用 [`MemoBook::replace`]，因此「生效内容」永远等于
/// 最后一次成功读写的结果；插件只读取快照，不持有工作区路径，也就无法绕过宿主
/// 直接碰文件（ADR §6：原生能力只能经宿主授权接口）。
#[derive(Default)]
pub struct MemoBook {
    snapshot: RwLock<MemoSnapshot>,
}

impl MemoBook {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn replace(&self, snapshot: MemoSnapshot) {
        *self
            .snapshot
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot;
    }

    pub fn snapshot(&self) -> MemoSnapshot {
        self.snapshot
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn find(&self, id: &str) -> Option<Memo> {
        self.snapshot().memos.into_iter().find(|memo| memo.id == id)
    }

    /// 已应用内容的指纹。
    pub fn fingerprint(&self) -> u64 {
        self.snapshot().fingerprint()
    }
}
