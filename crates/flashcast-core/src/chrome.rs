//! Chrome 书签的读取、可重建索引与检索（ticket 13）。
//!
//! 依据 `notes/research/chrome-bookmarks.md`：
//!
//! - **`Bookmarks` 与 `AccountBookmarks` 是事实来源**，索引可以随时重建，因此这里只保存
//!   内存索引 + 文件指纹（mtime + size），不落盘、也不改动 Chrome 的任何文件；
//! - **`checksum` / `checksum_sha256` 永不校验**：序列化细节是 Chromium 内部实现，
//!   校验失败只会让用户看不到自己的书签；
//! - **文件缺失是正常的空状态**，不是错误（全新 profile 就没有这个文件）；
//! - **解析失败先当作「写入中途读到半个文件」**：等约 500ms 重读一次，仍然失败才
//!   报告损坏，并保留上一次可用的索引；
//! - **绝不长期持有文件**：每次都是 `open → read_to_end → drop`；
//! - 未知字段一律容忍（`#[serde(default)]` + `flatten`），`version != 1` 也照样解析。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Score;
use crate::ranking::score_match;

/// 解析失败后重读一次的等待时间（研究笔记 §3）。
pub const PARSE_RETRY_DELAY: Duration = Duration::from_millis(500);

/// 设备本地存储里记录关联的键。
pub const KEY_CHROME_ASSOCIATION: &str = "chrome.association";

/// 结果条目标识的前缀。
pub const BOOKMARK_ITEM_PREFIX: &str = "chrome-bookmark:";

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// Bookmarks JSON（宽松模型）
// ---------------------------------------------------------------------------

/// `Bookmarks` 文件的顶层。未知字段（`sync_metadata` 等）一律忽略。
#[derive(Debug, Clone, Deserialize)]
pub struct BookmarksFile {
    #[serde(default)]
    pub roots: Roots,
    #[serde(default)]
    pub version: Option<i64>,
    /// 永不校验。
    #[serde(default)]
    pub checksum: Option<String>,
    /// 永不校验。
    #[serde(default)]
    pub checksum_sha256: Option<String>,
    /// 未文档化的字符串块，通常不存在。
    #[serde(default)]
    pub sync_metadata: Option<String>,
}

/// `roots`：三个已知根都是可选的（手写或新建的文件可能缺少），另外容忍未来的根。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Roots {
    #[serde(default)]
    pub bookmark_bar: Option<Node>,
    #[serde(default)]
    pub other: Option<Node>,
    #[serde(default)]
    pub synced: Option<Node>,
    /// 未来可能新增的根：也按节点解析，不丢内容。
    #[serde(flatten)]
    pub unknown: BTreeMap<String, serde_json::Value>,
}

/// 一个书签或目录节点。`type` 为 `url` 或 `folder`。
#[derive(Debug, Clone, Deserialize)]
pub struct Node {
    /// Chrome 写的是十进制字符串；保持字符串以保稳定。
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub children: Vec<Node>,
    /// 永不使用的旧字段：只在文件里出现时被忽略。
    #[serde(default)]
    pub meta_info: BTreeMap<String, serde_json::Value>,
}

impl Node {
    fn display_name(&self, fallback: &str) -> String {
        self.name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| fallback.to_string())
    }
}

/// 索引里的一条书签。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkEntry {
    /// 本地节点保留 Chrome id；账号节点使用 `account:<id>`，避免跨文件冲突。
    pub id: String,
    pub title: String,
    pub url: String,
    /// 目录路径，例如 `书签栏 / 开发 / Rust`；根目录本身也可能为空。
    pub folder: String,
}

impl BookmarkEntry {
    /// 结果条目的稳定标识。
    pub fn item_id(&self) -> String {
        format!("{BOOKMARK_ITEM_PREFIX}{}", self.id)
    }

    /// 从结果标识还原书签标识。
    pub fn id_from_item_id(item_id: &str) -> Option<&str> {
        item_id
            .strip_prefix(BOOKMARK_ITEM_PREFIX)
            .filter(|id| !id.is_empty())
    }
}

/// 书签文件的当前状态。面向用户的说明由 [`BookmarksStatus::label_zh`] 给出。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BookmarksStatus {
    /// 还没有关联 profile。
    NotAssociated,
    /// 两份文件都不存在：全新 profile 的正常空状态。
    Missing,
    /// 已读到 N 条书签。
    Ok { count: usize },
    /// JSON 解析失败（含重读一次后仍失败）。
    Corrupt { reason: String },
    /// 文件存在但读不了（权限、是目录……）。
    Unreadable { reason: String },
}

impl BookmarksStatus {
    pub fn label_zh(&self) -> String {
        match self {
            BookmarksStatus::NotAssociated => "尚未关联 Chrome profile".to_string(),
            BookmarksStatus::Missing => "该 profile 还没有书签文件（正常空状态）".to_string(),
            BookmarksStatus::Ok { count } => format!("已索引 {count} 条书签"),
            BookmarksStatus::Corrupt { reason } => format!("书签文件无法解析：{reason}"),
            BookmarksStatus::Unreadable { reason } => format!("书签文件无法读取：{reason}"),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, BookmarksStatus::Ok { .. } | BookmarksStatus::Missing)
    }
}

/// 索引快照。插件与 UI 只看到它。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkSnapshot {
    pub path: Option<PathBuf>,
    pub status: BookmarksStatus,
    pub entries: Vec<BookmarkEntry>,
}

/// 一次刷新的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookmarkRefresh {
    /// 文件是否发生了变化（含第一次建立索引）。
    pub changed: bool,
    pub status: BookmarksStatus,
}

/// 关联到本机的 Chrome profile。**只存在于设备本地存储**，不进入配置工作区。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeAssociation {
    /// profile 目录名（`--profile-directory` 用的就是它）。
    pub profile_dir: String,
    /// 显示名，仅用于 UI 与反馈。
    pub display_name: String,
    /// 用户数据目录（本机路径）。
    pub user_data_dir: PathBuf,
    /// Chrome 可执行文件（本机路径）。
    pub binary: PathBuf,
    /// 启动时是否需要显式传 `--user-data-dir`。
    pub pass_user_data_dir: bool,
}

impl ChromeAssociation {
    /// `Bookmarks` 文件路径。
    pub fn bookmarks_path(&self) -> PathBuf {
        self.user_data_dir.join(&self.profile_dir).join("Bookmarks")
    }

    /// profile 目录是否真的存在。
    ///
    /// Chrome 在 `--profile-directory` 指向不存在的目录时会**静默新建**一个空 profile，
    /// 因此启动前必须校验（研究笔记 §4）。
    pub fn profile_dir_exists(&self) -> bool {
        self.user_data_dir.join(&self.profile_dir).is_dir()
    }
}

/// 设置界面里的一个 profile。
///
/// 本机路径**不**出现在这里：UI 只需要目录名、显示名与「能不能读」，路径留在设备本地
/// 存储与宿主内部（ticket 要求本地路径保持设备本地）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeProfileView {
    pub dir: String,
    pub name: String,
    pub user_name: Option<String>,
    pub managed: bool,
    pub has_bookmarks: bool,
    pub bookmarks_readable: bool,
    pub unreadable_reason: Option<String>,
    /// 是否就是当前关联的 profile。
    pub associated: bool,
}

/// 面向 UI 的 Chrome 状态：发现结果、profile 列表、关联状态与索引状态。
///
/// 关联的**本机路径**留在宿主内部：这里只给 UI 需要的展示信息，避免把
/// 「任意路径 + 任意 argv」暴露给前端（研究笔记 §5）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeState {
    /// 是否发现了 Chrome 可执行文件。
    pub available: bool,
    /// 浏览器品牌的中文名（Chrome / Chromium / Edge）。
    pub brand_label: Option<String>,
    /// 用户数据目录是否属于默认位置之外（决定是否传 `--user-data-dir`）。
    pub custom_user_data_dir: bool,
    /// 可执行文件路径（本机，仅供设置界面显示与排查）。
    pub binary: Option<PathBuf>,
    /// 用户数据目录（本机）。
    pub user_data_dir: Option<PathBuf>,
    pub profiles: Vec<ChromeProfileView>,
    /// 当前关联的 profile 目录名。
    pub associated: Option<String>,
    /// 关联当前的显示名。
    pub associated_name: Option<String>,
    /// 关联或发现过程的问题（Chrome 未安装、profile 消失、设备本地记录损坏……）。
    pub error: Option<String>,
    /// 非致命说明（`Local State` 解析失败、目录尚不存在……）。
    pub warnings: Vec<String>,
    /// 书签索引状态。
    pub bookmarks: BookmarkSnapshot,
    /// 索引状态的中文说明。
    pub bookmarks_label: String,
}

impl ChromeState {
    /// 完全没有开始关联时的状态。
    pub fn not_associated() -> Self {
        let status = BookmarksStatus::NotAssociated;
        Self {
            available: false,
            brand_label: None,
            custom_user_data_dir: false,
            binary: None,
            user_data_dir: None,
            profiles: Vec::new(),
            associated: None,
            associated_name: None,
            error: None,
            warnings: Vec::new(),
            bookmarks_label: status.label_zh(),
            bookmarks: BookmarkSnapshot {
                path: None,
                status,
                entries: Vec::new(),
            },
        }
    }
}

/// 关联 / 打开书签时的可操作失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChromeBookmarkError {
    #[error("设备本地数据读写失败：{0}")]
    Device(String),
    #[error("尚未关联 Chrome profile：请在设置里选择一个 profile")]
    NoAssociation,
    #[error(
        "profile 目录不存在：{0}（Chrome 会为不存在的目录静默新建空 profile，因此已拒绝启动）"
    )]
    ProfileMissing(String),
    #[error("profile 不可读：{0}")]
    ProfileUnreadable(String),
    #[error("书签文件无法解析：{0}")]
    BookmarksCorrupt(String),
    #[error("书签文件无法读取：{0}")]
    BookmarksUnreadable(String),
    #[error("找不到这条书签：{0}（书签可能已在 Chrome 里变化，请重新查询）")]
    BookmarkMissing(String),
    #[error("链接不受支持：{0}")]
    InvalidUrl(String),
    #[error("插件「{0}」已停用，已拒绝执行")]
    PluginDisabled(String),
    #[error("{0}")]
    SourceRejected(String),
    #[error("{0}")]
    CapabilityMissing(String),
    #[error("无法启动 Chrome：{0}")]
    LaunchFailed(String),
}

impl From<flashcast_platform::ChromeError> for ChromeBookmarkError {
    fn from(error: flashcast_platform::ChromeError) -> Self {
        use flashcast_platform::ChromeError as Platform;
        match error {
            Platform::NotInstalled { .. } | Platform::Unsupported(_) => {
                ChromeBookmarkError::ProfileUnreadable(error.to_string())
            }
            Platform::UserDataDirUnreadable(reason) => {
                ChromeBookmarkError::ProfileUnreadable(reason)
            }
            Platform::ProfileMissing(path) => ChromeBookmarkError::ProfileMissing(path),
            Platform::ProfileUnreadable(reason) => ChromeBookmarkError::ProfileUnreadable(reason),
            Platform::BookmarksUnreadable(reason) => {
                ChromeBookmarkError::BookmarksUnreadable(reason)
            }
            Platform::InvalidUrl(reason) => ChromeBookmarkError::InvalidUrl(reason),
            Platform::LaunchFailed(reason) => ChromeBookmarkError::LaunchFailed(reason),
        }
    }
}

// ---------------------------------------------------------------------------
// 解析
// ---------------------------------------------------------------------------

/// 解析 `Bookmarks` 字节，返回按**文件顺序**（三个根：书签栏 → 其他 → 移动设备）排列的书签。
///
/// 未知字段、未知根、`version != 1`、缺失的 `id` / `name` 都不会导致失败。
pub fn parse_bookmarks(bytes: &[u8]) -> Result<Vec<BookmarkEntry>, String> {
    let file: BookmarksFile =
        serde_json::from_slice(bytes).map_err(|error| format!("JSON 解析失败：{error}"))?;

    let mut entries = Vec::new();
    let mut seen_ids: Vec<String> = Vec::new();
    let mut auto_id = 0usize;

    // 三个已知根按固定顺序；根自身的名字就是第一层目录名。
    if let Some(node) = &file.roots.bookmark_bar {
        walk_root(node, "书签栏", &mut entries, &mut seen_ids, &mut auto_id);
    }
    if let Some(node) = &file.roots.other {
        walk_root(node, "其他书签", &mut entries, &mut seen_ids, &mut auto_id);
    }
    if let Some(node) = &file.roots.synced {
        walk_root(
            node,
            "移动设备书签",
            &mut entries,
            &mut seen_ids,
            &mut auto_id,
        );
    }
    for (key, value) in &file.roots.unknown {
        // 未来的根：值可能不是节点，解析失败就跳过这一个，不影响其它根。
        if let Ok(node) = serde_json::from_value::<Node>(value.clone()) {
            walk_root(&node, key, &mut entries, &mut seen_ids, &mut auto_id);
        }
    }
    Ok(entries)
}

/// 从一个根目录开始：根的名字就是第一层目录名。
fn walk_root(
    node: &Node,
    fallback: &str,
    entries: &mut Vec<BookmarkEntry>,
    seen_ids: &mut Vec<String>,
    auto_id: &mut usize,
) {
    let folder = node.display_name(fallback);
    for child in &node.children {
        walk_node(child, &folder, entries, seen_ids, auto_id);
    }
}

fn walk_node(
    node: &Node,
    folder: &str,
    entries: &mut Vec<BookmarkEntry>,
    seen_ids: &mut Vec<String>,
    auto_id: &mut usize,
) {
    let is_folder = node.kind.as_deref() == Some("folder") || node.url.is_none();
    if !is_folder {
        // `type` 缺失时按 URL 节点处理：只要有非空 url 就收进来。
        if let Some(url) = node
            .url
            .as_deref()
            .map(str::trim)
            .filter(|url| !url.is_empty())
        {
            let id = match node.id.as_deref().filter(|id| !id.is_empty()) {
                Some(id) if !seen_ids.iter().any(|seen| seen == id) => id.to_string(),
                Some(id) => {
                    // 重复 id：Chrome 自己会重排，但手改过的文件可能重复。
                    *auto_id += 1;
                    format!("{id}-dup{auto_id}")
                }
                None => {
                    *auto_id += 1;
                    format!("auto-{auto_id}")
                }
            };
            seen_ids.push(id.clone());
            entries.push(BookmarkEntry {
                id,
                title: node.display_name(url),
                url: url.to_string(),
                folder: folder.to_string(),
            });
        }
        // 极少数文件会把 url 节点也写成带 children 的形式，下面照样下钻。
    }
    if node.children.is_empty() {
        return;
    }
    // 目录节点的名字追加到目录路径；没有名字时保持父目录不变（不产生「/ /」）。
    let name = node
        .name
        .as_deref()
        .map(str::trim)
        .filter(|name| !name.is_empty());
    let child_folder = match name {
        Some(name) if is_folder => {
            if folder.is_empty() {
                name.to_string()
            } else {
                format!("{folder} / {name}")
            }
        }
        _ => folder.to_string(),
    };
    for child in &node.children {
        walk_node(child, &child_folder, entries, seen_ids, auto_id);
    }
}

/// 按标题、URL 与目录中的任意一项匹配。返回与排序无关的 `(条目, 分数)` 列表。
pub fn search_bookmarks<'a>(
    entries: &'a [BookmarkEntry],
    query: &str,
) -> Vec<(&'a BookmarkEntry, Score)> {
    let query = query.trim().to_lowercase();
    let mut found = Vec::new();
    if query.is_empty() {
        return entries
            .iter()
            .map(|entry| (entry, Score::unordered()))
            .collect();
    }
    for entry in entries {
        // 元数据顺序即相关度：URL 比目录更贴近「找这个链接」的意图。
        let metadata = [entry.url.as_str(), entry.folder.as_str()];
        if let Some(score) = score_match(&query, &entry.title, &metadata) {
            found.push((entry, score));
        }
    }
    found
}

// ---------------------------------------------------------------------------
// 索引
// ---------------------------------------------------------------------------

/// 文件指纹：mtime + size。任何一项变化都触发重读。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Fingerprint {
    len: u64,
    /// 修改时间（自 UNIX_EPOCH 的纳秒）；文件系统不支持时为 `None`。
    modified_nanos: Option<u128>,
}

#[derive(Debug)]
struct IndexInner {
    path: Option<PathBuf>,
    entries: Vec<BookmarkEntry>,
    status: BookmarksStatus,
    fingerprint: Option<Vec<Option<Fingerprint>>>,
}

/// 可重建的书签索引。
///
/// 索引是**内存**的：本地与账号书签文件是事实来源，进程重启后重新读取即可
/// 重建（ticket 13 的关联记录保存在设备本地存储里）。不落盘的另一层原因是索引
/// 可能包含敏感 URL（研究笔记 §5），少一处副本就少一处泄露面。
pub struct BookmarkIndex {
    inner: Mutex<IndexInner>,
}

impl Default for BookmarkIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl BookmarkIndex {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(IndexInner {
                path: None,
                entries: Vec::new(),
                status: BookmarksStatus::NotAssociated,
                fingerprint: None,
            }),
        }
    }

    /// 切换索引指向的文件。下一次刷新会重新读取。
    pub fn set_path(&self, path: Option<PathBuf>) {
        let mut inner = lock(&self.inner);
        if inner.path == path {
            return;
        }
        inner.path = path;
        inner.entries.clear();
        inner.fingerprint = None;
        inner.status = if inner.path.is_some() {
            BookmarksStatus::Missing
        } else {
            BookmarksStatus::NotAssociated
        };
    }

    pub fn path(&self) -> Option<PathBuf> {
        lock(&self.inner).path.clone()
    }

    pub fn snapshot(&self) -> BookmarkSnapshot {
        let inner = lock(&self.inner);
        BookmarkSnapshot {
            path: inner.path.clone(),
            status: inner.status.clone(),
            entries: inner.entries.clone(),
        }
    }

    pub fn entries(&self) -> Vec<BookmarkEntry> {
        lock(&self.inner).entries.clone()
    }

    pub fn status(&self) -> BookmarksStatus {
        lock(&self.inner).status.clone()
    }

    pub fn find(&self, id: &str) -> Option<BookmarkEntry> {
        lock(&self.inner)
            .entries
            .iter()
            .find(|entry| entry.id == id)
            .cloned()
    }

    /// 文件变化时重读；解析失败会等 [`PARSE_RETRY_DELAY`] 后重读一次。
    ///
    /// 宿主入口（查看关联状态、打开书签前）使用它。插件搜索用
    /// [`Self::refresh_if_changed`]，避免把 500ms 的重试带进有超时的搜索线程。
    pub fn refresh(&self) -> BookmarkRefresh {
        self.reload(true)
    }

    /// 文件变化时重读一次，不重试。
    pub fn refresh_if_changed(&self) -> BookmarkRefresh {
        self.reload(false)
    }

    /// 丢弃文件指纹，让下一次 [`Self::refresh`] 一定重新读取。
    ///
    /// 「重新读取书签」入口用它：外部可能把文件改回与上次相同的大小 / 时间戳，
    /// 显式刷新不应该被指纹挡掉。
    pub fn invalidate(&self) {
        lock(&self.inner).fingerprint = None;
    }

    fn reload(&self, retry: bool) -> BookmarkRefresh {
        let path = { lock(&self.inner).path.clone() };
        let Some(path) = path else {
            return BookmarkRefresh {
                changed: false,
                status: BookmarksStatus::NotAssociated,
            };
        };

        let paths = source_paths(&path);
        let current = Some(paths.iter().map(|p| fingerprint_of(p)).collect());
        {
            let inner = lock(&self.inner);
            if inner.fingerprint == current && inner.status.is_ok() {
                return BookmarkRefresh {
                    changed: false,
                    status: inner.status.clone(),
                };
            }
        }

        let mut attempt = read_sources(&paths);
        if retry {
            if let Err(reason) = &attempt {
                // 可能是「写入中途读到半个文件」：等约 500ms 再读一次。
                // 这里刻意不释放任何锁的情况下休眠是安全的：reload 不持有 inner 守卫。
                let _ = reason;
                std::thread::sleep(PARSE_RETRY_DELAY);
                attempt = read_sources(&paths);
            }
        }

        let mut inner = lock(&self.inner);
        // 方向一致：刷新期间路径被换掉（切换了关联 profile）时丢弃本轮结果。
        if inner.path.as_deref() != Some(path.as_path()) {
            return BookmarkRefresh {
                changed: false,
                status: inner.status.clone(),
            };
        }
        inner.fingerprint = current;
        let status = match attempt {
            Ok(parsed) => {
                let status = match &parsed {
                    Parsed::Missing => BookmarksStatus::Missing,
                    Parsed::Entries(entries) => BookmarksStatus::Ok {
                        count: entries.len(),
                    },
                };
                inner.entries = match parsed {
                    Parsed::Missing => Vec::new(),
                    Parsed::Entries(entries) => entries,
                };
                status
            }
            Err(failure) => match failure {
                // 读不到文件：保留上一次可用的索引，并如实报告。
                ReadFailure::Unreadable(reason) => BookmarksStatus::Unreadable { reason },
                ReadFailure::Corrupt(reason) => BookmarksStatus::Corrupt { reason },
            },
        };
        inner.status = status.clone();
        BookmarkRefresh {
            changed: true,
            status,
        }
    }
}

enum Parsed {
    Missing,
    Entries(Vec<BookmarkEntry>),
}

enum ReadFailure {
    Unreadable(String),
    Corrupt(String),
}

/// 旧关联仍保存 Bookmarks 路径；读取时纳入同 profile 的账号文件。
fn source_paths(path: &Path) -> Vec<PathBuf> {
    if path.file_name().is_some_and(|name| name == "Bookmarks") {
        vec![path.to_path_buf(), path.with_file_name("AccountBookmarks")]
    } else {
        vec![path.to_path_buf()]
    }
}

fn read_sources(paths: &[PathBuf]) -> Result<Parsed, ReadFailure> {
    let mut entries = Vec::new();
    let mut found = false;
    for path in paths {
        match read_bookmarks(path)? {
            Parsed::Missing => {}
            Parsed::Entries(mut source_entries) => {
                found = true;
                if path
                    .file_name()
                    .is_some_and(|name| name == "AccountBookmarks")
                {
                    for entry in &mut source_entries {
                        entry.id = format!("account:{}", entry.id);
                    }
                }
                entries.extend(source_entries);
            }
        }
    }
    Ok(if found {
        Parsed::Entries(entries)
    } else {
        Parsed::Missing
    })
}

/// 读取并解析一次。`Bookmarks` 缺失是**正常**结果（[`Parsed::Missing`]）。
///
/// 文件在读取前打开、读完立刻 drop：绝不长期持有，也绝不 `mmap`。
fn read_bookmarks(path: &Path) -> Result<Parsed, ReadFailure> {
    use std::io::Read;

    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Parsed::Missing),
        Err(error) => {
            return Err(ReadFailure::Unreadable(format!(
                "{}：{error}",
                path.display()
            )))
        }
    };
    let mut bytes = Vec::new();
    if let Err(error) = file.read_to_end(&mut bytes) {
        return Err(ReadFailure::Unreadable(format!(
            "{}：{error}",
            path.display()
        )));
    }
    drop(file);
    match parse_bookmarks(&bytes) {
        Ok(entries) => Ok(Parsed::Entries(entries)),
        Err(reason) => Err(ReadFailure::Corrupt(format!(
            "{}：{reason}",
            path.display()
        ))),
    }
}

fn fingerprint_of(path: &Path) -> Option<Fingerprint> {
    let metadata = std::fs::metadata(path).ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    Some(Fingerprint {
        len: metadata.len(),
        modified_nanos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "checksum": "忽略",
      "checksum_sha256": "忽略",
      "roots": {
        "bookmark_bar": {
          "children": [
            { "id": "7", "name": "Rust 官网", "type": "url", "url": "https://www.rust-lang.org/" },
            { "id": "8", "name": "开发", "type": "folder", "children": [
                { "id": "9", "name": "文档", "type": "url", "url": "https://doc.rust-lang.org/?q=中文&x=1" }
            ] }
          ],
          "id": "1", "name": "书签栏", "type": "folder"
        },
        "other": { "children": [], "id": "2", "name": "其他书签", "type": "folder" },
        "synced": { "children": [], "id": "3", "name": "移动设备书签", "type": "folder" }
      },
      "sync_metadata": "忽略",
      "version": 1
    }"#;

    #[test]
    fn parse_keeps_folders_and_non_ascii() {
        let entries = parse_bookmarks(SAMPLE.as_bytes()).expect("必须能解析");
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].title, "Rust 官网");
        assert_eq!(entries[0].folder, "书签栏");
        assert_eq!(entries[1].folder, "书签栏 / 开发");
        assert_eq!(entries[1].url, "https://doc.rust-lang.org/?q=中文&x=1");
    }

    #[test]
    fn parse_tolerates_missing_and_unknown_pieces() {
        // 只有 roots 里的一个未知根，且 id / name / type 全缺。
        let text = r#"{"roots":{"future_root":{"children":[{"url":"https://a.example/"}]}}}"#;
        let entries = parse_bookmarks(text.as_bytes()).expect("必须能解析");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].title, "https://a.example/");
        assert_eq!(entries[0].id, "auto-1");
        // version 不是 1 也照常解析；checksum 完全不看。
        assert!(parse_bookmarks(br#"{"roots":{},"version":99}"#)
            .unwrap()
            .is_empty());
        assert!(parse_bookmarks(b"not json").is_err());
    }

    #[test]
    fn search_matches_title_url_and_folder() {
        let entries = parse_bookmarks(SAMPLE.as_bytes()).unwrap();
        let title = search_bookmarks(&entries, "rust");
        assert_eq!(title.len(), 2, "标题与 URL 都能命中");
        assert_eq!(
            title[0].0.title, "Rust 官网",
            "标题匹配必须排在 URL 匹配之前"
        );
        let url = search_bookmarks(&entries, "doc.rust-lang.org");
        assert_eq!(url.len(), 1);
        assert_eq!(url[0].0.id, "9");
        let folder = search_bookmarks(&entries, "开发");
        assert_eq!(folder.len(), 1);
        assert_eq!(folder[0].0.id, "9");
        assert!(search_bookmarks(&entries, "不存在的词").is_empty());
    }

    #[test]
    fn item_ids_round_trip() {
        let entry = BookmarkEntry {
            id: "42".to_string(),
            title: "t".to_string(),
            url: "https://a.example/".to_string(),
            folder: "书签栏".to_string(),
        };
        let item_id = entry.item_id();
        assert_eq!(BookmarkEntry::id_from_item_id(&item_id), Some("42"));
        assert_eq!(BookmarkEntry::id_from_item_id("memo:1"), None);
    }
}
