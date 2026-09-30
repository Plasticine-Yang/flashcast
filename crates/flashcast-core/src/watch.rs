//! 工作区文件监听：外部修改后重新加载有效配置（ADR §8）。
//!
//! 设计依据 `notes/research/git-workspace.md` §6。这里逐条实现它要求的「四层」自写
//! 抑制，任何单独一层都会漏：
//!
//! 1. **原子写入**（[`crate::workspace::write_atomic`]）：同目录临时文件 + `rename`，
//!    监听侧只看到一次 `Create` + `Rename`，去抖器可以合并。
//! 2. **内容哈希自写账本**（[`ChangeFilter::record_self_write`]）：按「路径 → 新内容哈希」
//!    记录自身写入。事件到达时重新读取该文件，内容哈希一致就吞掉这次事件。
//!    用内容哈希而不是 mtime，因为 macOS / Windows 的 mtime 粒度会说谎。
//! 3. **每路径静默窗口**：自身写入后的一小段时间内忽略该路径的事件，兜住编辑器
//!    与杀毒软件在 `rename` 之后再次触碰文件的情况。
//! 4. **Git 操作忙标志**：`git` 操作期间丢弃全部事件，并在操作结束后按 Git 状态显式
//!    重建界面，而不是依赖事件。ticket 05 的 `set_git_busy` 就是这个seam，
//!    ticket 15/16 的 status / commit / pull 会用到它。
//!
//! 另外**排除 gitdir**：`StatusOptions::update_index(true)` 会在每次 status 查询时
//! 改写 `.git/index`，是典型的自伤事件风暴来源。ticket 15/16 引入 status 查询前，
//! 这一层就已经生效（[`ChangeFilter`] 的 `git_dir` 检查）。
//!
//! 最后一层是「幂等应用」：真正接受外部修改时会先与内存中的模型比较，相同则不做
//! 任何事（见 [`crate::host::Host::reload_from_workspace`]）。因此不会出现
//! 「写入 → 事件 → 应用 → 再写入」的循环。
//!
//! ## 只读事件必须丢弃（实测踩到的自伤循环）
//!
//! notify 的 inotify 后端会把 `IN_ACCESS` / `IN_OPEN` / `IN_CLOSE` 一并上报为
//! `EventKind::Access`。第 2 层要读文件内容算哈希，宿主重载时也要读设置文件，
//! 这些**读操作本身**又会生成 `Access` 事件；把它们当成「文件改了」就形成
//! 「读 → 事件 → 再读」的无限事件流（实测：一次写入后事件每隔约 500ms 再来一次，
//! 且永远不收敛）。因此 [`is_modification`] 只接受 `Create` / `Modify` / `Remove` /
//! `Any`，丢弃 `Access` 与 `Other`。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use notify_debouncer_full::notify::{
    EventKind, RecommendedWatcher, RecursiveMode,
};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::workspace::{MEMOS_DIR, TEMP_SUFFIX, THEMES_DIR, THEME_FILE, WORKSPACE_FILES};

/// 去抖窗口。编辑器保存是突发写入，500ms 足以合并成一次。
pub const DEBOUNCE: Duration = Duration::from_millis(500);
/// 自身写入之后的静默窗口。
pub const QUIET_WINDOW: Duration = Duration::from_millis(600);
/// 自写账本条目的最长保留时间，避免长期占用内存。
const LEDGER_TTL: Duration = Duration::from_secs(10);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Copy)]
struct LedgerEntry {
    hash: u64,
    at: Instant,
}

/// 事件过滤状态。由监听回调（notify 自己的线程）与宿主共享。
pub struct ChangeFilter {
    root: PathBuf,
    git_dir: Option<PathBuf>,
    ledger: Mutex<HashMap<PathBuf, LedgerEntry>>,
    quiet: Mutex<HashMap<PathBuf, Instant>>,
    git_busy: AtomicBool,
}

impl ChangeFilter {
    pub fn new(root: PathBuf, git_dir: Option<PathBuf>) -> Self {
        Self {
            root,
            git_dir,
            ledger: Mutex::new(HashMap::new()),
            quiet: Mutex::new(HashMap::new()),
            git_busy: AtomicBool::new(false),
        }
    }

    /// 记录一次自身写入。写文件**之前**调用。
    pub fn record_self_write(&self, path: &Path, bytes: &[u8]) {
        let now = Instant::now();
        lock(&self.ledger).insert(
            path.to_path_buf(),
            LedgerEntry {
                hash: hash_bytes(bytes),
                at: now,
            },
        );
        lock(&self.quiet).insert(path.to_path_buf(), now);
    }

    /// Git 操作忙标志。为真时丢弃全部事件。
    pub fn set_git_busy(&self, busy: bool) {
        self.git_busy.store(busy, Ordering::SeqCst);
    }

    /// 该路径的变更是否应视为「需要处理的外部修改」。
    pub fn accept(&self, path: &Path) -> bool {
        if self.git_busy.load(Ordering::SeqCst) {
            return false;
        }
        // gitdir（`.git/`）内部的变化一律忽略：status 查询会改写 `.git/index`。
        if let Some(git_dir) = &self.git_dir {
            if path.starts_with(git_dir) {
                return false;
            }
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_noise(&name) {
            return false;
        }
        if !self.is_owned(path) {
            return false;
        }

        let now = Instant::now();
        {
            let mut ledger = lock(&self.ledger);
            ledger.retain(|_, entry| now.duration_since(entry.at) < LEDGER_TTL);
            if let Some(entry) = ledger.get(path) {
                if content_hash(path) == Some(entry.hash) {
                    ledger.remove(path);
                    return false;
                }
            }
        }
        {
            let mut quiet = lock(&self.quiet);
            quiet.retain(|_, at| now.duration_since(*at) < LEDGER_TTL);
            if let Some(at) = quiet.get(path) {
                if now.duration_since(*at) < QUIET_WINDOW {
                    return false;
                }
            }
        }
        true
    }

    /// 路径是否是应用真正维护的工作区文件。
    fn is_owned(&self, path: &Path) -> bool {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return false;
        };
        let parts: Vec<&OsStr> = relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(name) => Some(name),
                _ => None,
            })
            .collect();
        match parts.as_slice() {
            [file] => WORKSPACE_FILES.iter().any(|name| OsStr::new(name) == *file),
            [dir, file] => {
                if *dir == OsStr::new(MEMOS_DIR) {
                    return file.to_string_lossy().ends_with(".md");
                }
                // 已安装的本地主题包：themes/<主题 id>/theme.json
                *dir == OsStr::new(THEMES_DIR) && *file == OsStr::new(THEME_FILE)
            }
            [dir, id, file] => {
                *dir == OsStr::new(THEMES_DIR)
                    && *file == OsStr::new(THEME_FILE)
                    && !id.is_empty()
            }
            _ => false,
        }
    }
}

/// 该事件是否代表文件内容/名字可能发生了变化。
///
/// 只读事件（`Access`）必须丢弃，否则「读文件算哈希」本身会再次触发事件，
/// 形成永不收敛的自伤事件流（见模块文档）。
fn is_modification(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// 编辑器与系统噪声：交换文件、备份文件、临时文件、锁文件与目录元数据。
fn is_noise(name: &str) -> bool {    name.ends_with(TEMP_SUFFIX)
        || name.ends_with('~')
        || name.ends_with(".swp")
        || name.ends_with(".swx")
        || name.ends_with(".tmp")
        || name.ends_with(".lock")
        || name.starts_with(".#")
        || matches!(name, ".DS_Store" | "Thumbs.db" | "desktop.ini")
        // `.git`、`.gitignore` 等点文件不属于工作区配置文件。
        || name.starts_with('.')
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

fn content_hash(path: &Path) -> Option<u64> {
    std::fs::read(path).ok().map(|bytes| hash_bytes(&bytes))
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WatchError {
    #[error("无法监听工作区目录：{0}")]
    Notify(String),
}

/// 一个正在运行的工作区监听器。丢弃它即停止监听。
pub struct WorkspaceWatcher {
    filter: Arc<ChangeFilter>,
    // 必须保活：`Debouncer` 被丢弃后监听立即停止。
    #[allow(dead_code)]
    debouncer: Debouncer<RecommendedWatcher, RecommendedCache>,
    receiver: Receiver<PathBuf>,
    root: PathBuf,
}

impl WorkspaceWatcher {
    /// 递归监听工作区根目录，只把应用维护的文件的变更送进通道。
    pub fn start(root: &Path, git_dir: Option<PathBuf>) -> Result<Self, WatchError> {
        let filter = Arc::new(ChangeFilter::new(root.to_path_buf(), git_dir));
        let (sender, receiver): (Sender<PathBuf>, Receiver<PathBuf>) = channel();
        let callback_filter = Arc::clone(&filter);
        let mut debouncer = new_debouncer(
            DEBOUNCE,
            None,
            move |result: DebounceEventResult| {
                let events = match result {
                    Ok(events) => events,
                    // 监听错误（例如 inotify 上限）不改变宿主状态：等下一次有效事件。
                    Err(_) => return,
                };
                for event in events {
                    // 只读事件不算修改：算哈希时读文件本身会再生成 Access 事件。
                    if !is_modification(&event.event.kind) {
                        continue;
                    }
                    for path in &event.event.paths {
                        if callback_filter.accept(path) {
                            let _ = sender.send(path.clone());
                        }
                    }
                }
            },
        )
        .map_err(|error| WatchError::Notify(error.to_string()))?;
        debouncer
            .watch(root, RecursiveMode::Recursive)
            .map_err(|error| WatchError::Notify(error.to_string()))?;
        Ok(Self {
            filter,
            debouncer,
            receiver,
            root: root.to_path_buf(),
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 取走一个已就绪的外部变更；没有则立即返回 `None`。
    ///
    /// 刻意不提供阻塞等待：宿主用短轮询调用它，这样等待期间**不握着**工作区锁，
    /// 保存设置或切换工作区不会被监听线程卡住（去抖窗口本身已有 500ms 延迟，
    /// 25ms 的轮询间隔不增加可感知的时延）。
    pub fn try_next_change(&self) -> Option<PathBuf> {
        self.receiver.try_recv().ok()
    }

    pub fn record_self_write(&self, path: &Path, bytes: &[u8]) {
        self.filter.record_self_write(path, bytes);
    }

    /// Git 操作忙标志：操作期间丢弃事件（见模块文档第 4 层）。
    pub fn set_git_busy(&self, busy: bool) {
        self.filter.set_git_busy(busy);
    }
}
