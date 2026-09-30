//! 工作区文件监听：外部修改后重新加载有效配置（ADR §8）。
//!
//! 设计依据 `notes/research/git-workspace.md` §6。这里逐条实现它要求的「四层」自写
//! 抑制，任何单独一层都会漏：
//!
//! 1. **原子写入**（[`crate::workspace::write_atomic`]）：同目录临时文件 + `rename`，
//!    监听侧只看到一次 `Create` + `Rename`，去抖器可以合并。
//! 2. **内容哈希账本**（[`ChangeFilter::record_self_write`]）：按「路径 → 新内容哈希」
//!    记录自身写入。事件到达时重新读取该文件，内容哈希一致就吞掉这次事件。
//!    用内容哈希而不是 mtime，因为 macOS / Windows 的 mtime 粒度会说谎。
//!    宿主**自己重载时读到的内容**也记进同一账本（[`ChangeFilter::record_own_read`]），
//!    原因见下文「重载自己的读」。
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
//! 最后一层是「已应用内容相等」：宿主重载时先读**原始字节**，与「当前已应用内容」的哈希
//! 比较，相同则不计重载、不应用、也不对外发出任何事件（见
//! [`crate::host::Host::reload_from_workspace`]）。因此不会出现
//! 「写入 → 事件 → 应用 → 再写入」的循环。
//!
//! ## 正确性不依赖上面四层（跨平台缺陷 2 之后的判据）
//!
//! 上面第 1–4 层与 [`ChangeFilter`] 的账本、静默窗口都只是**廉价的第一道过滤**：
//! 它们依赖事件路径、事件类型与到达时间，而这三点在 macOS（FSEvents）上都不可靠——
//! 上报的路径可能与写入路径不同（大小写、软链接、目录项规范化）、一次逻辑写入拆成
//! 多条记录、读文件本身被上报成 `EventKind::Any`、事件晚于静默窗口才到达。
//! 真正保证「一次外部修改只重载一次」的是 [`crate::host::Host::reload_from_workspace`]
//! 的**内容判据**：磁盘字节与已应用内容一致就是无事发生。把本模块的第 1–4 层全部删掉，
//! 单测仍然必须通过。
//!
//! ## 跨平台：账本条目保留整个 TTL，命中即删是不可靠的
//!
//! 账本条目在第一次命中后**不删除**，而是保留到 `LEDGER_TTL` 到期（命中时只刷新静默
//! 时间戳）。inotify 对一次 `rename` 基本只上报一条记录，而 macOS 的 FSEvents 与 Windows
//! 的 `ReadDirectoryChangesW` 常把同一次逻辑写入拆成**多条**记录，且相邻记录之间可能隔着
//! 几百毫秒到 1 秒（FSEvents 的 latency）。若第一条记录命中内容哈希后就把条目删掉，后面的
//! 记录只能落到第 3 层静默窗口上；一旦它们晚于 `QUIET_WINDOW` 到达，就会被当成「外部修改」
//! 产生一次内容并未变化的多余重载（CI 实测：macOS arm64 / x86_64 与 Windows x64 的
//! `workspace_watch` 用例因此失败）。反过来，条目也不在命中时续期：`LEDGER_TTL` 从「记录
//! 内容」那一刻起算，保证抑制不会因为持续的事件而无限延长，内存也始终有界。
//!
//! `QUIET_WINDOW` 保持 600ms，**不**按平台放宽。它只是第 3 层的兜底，真正的判据是第 2 层的
//! 内容哈希；把它放到 1 秒以上只会把「用户在自身写入后立刻做的第二次真实外部修改」也一起
//! 吞掉——静默窗口既挡住噪声，也决定多快能看到真实的连续两次修改，盲目加长是拿正确性换
//! 稳定。跨平台的多记录问题由账本 TTL 解决，不靠放宽这个窗口。
//!
//! ## 重载自己的读也必须记账（macOS 上尤其致命）
//!
//! 宿主重载配置时要读 `settings.toml`，这次**读**在 macOS 的 FSEvents 上常被上报为
//! `EventKind::Any`（FSEvents 经常不给更细的类型），因此 [`is_modification`] 必须接受
//! `Any`。于是「重载 → 读 → 事件 → 重载」会自我维持：读取的内容与刚应用的内容字节相同，
//! 却仍被当成一次外部修改送进通道，测试表现为「一次外部修改不得产生持续的事件流」失败。
//! 修正办法是把这次读取的字节哈希也记进账本（[`ChangeFilter::record_own_read`]）：此后任何
//! 内容字节一致的事件都被第 2 层吞掉，与平台上报的事件类型无关。它与
//! [`ChangeFilter::record_self_write`] 共用账本，只是**不**刷新静默时间戳——重载是读而不是
//! 写，不该顺带屏蔽内容确实变了的紧随修改。
//!
//! ## 只读事件必须丢弃（实测踩到的自伤循环）
//!
//! notify 的 inotify 后端会把 `IN_ACCESS` / `IN_OPEN` / `IN_CLOSE` 一并上报为
//! `EventKind::Access`。第 2 层要读文件内容算哈希，宿主重载时也要读设置文件，
//! 这些**读操作本身**又会生成 `Access` 事件；把它们当成「文件改了」就形成
//! 「读 → 事件 → 再读」的无限事件流（实测：一次写入后事件每隔约 500ms 再来一次，
//! 且永远不收敛）。因此 [`is_modification`] 只接受 `Create` / `Modify` / `Remove` /
//! `Any`，丢弃 `Access` 与 `Other`。这是 Linux 侧的一道保险；macOS 的 `Any` 无法这样
//! 过滤，只能靠上面的「重载自己的读也要记账」兜住。

use std::collections::HashMap;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use notify_debouncer_full::notify::{EventKind, RecommendedWatcher, RecursiveMode};
use notify_debouncer_full::{new_debouncer, DebounceEventResult, Debouncer, RecommendedCache};

use crate::workspace::{MEMOS_DIR, TEMP_SUFFIX, THEMES_DIR, THEME_FILE, WORKSPACE_FILES};

/// 去抖窗口。编辑器保存是突发写入，500ms 足以合并成一次。
pub const DEBOUNCE: Duration = Duration::from_millis(500);
/// 自身写入之后的静默窗口。
///
/// 刻意**不**按平台放宽：跨平台的多事件记录问题由内容哈希账本的 `LEDGER_TTL` 解决，
/// 而放宽这个窗口会连「自身写入后紧接着的真实外部修改」一起吞掉（见模块文档）。
pub const QUIET_WINDOW: Duration = Duration::from_millis(600);
/// 内容哈希账本条目的最长保留时间，避免长期占用内存。
const LEDGER_TTL: Duration = Duration::from_secs(10);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Copy)]
struct LedgerEntry {
    hash: u64,
    at: Instant,
}

/// 一次文件事件的诊断轨迹。
///
/// 「多了一次重载」时，只看断言里的计数说明不了任何问题：需要知道是**哪条事件**、
/// 走的**哪个决策分支**漏出来的。这里记录最后一次被接受的事件（路径 + 事件类型 + 宿主
/// 的处理结果）与最后一次过滤决策，测试可以把它放进断言消息。事件已经过去 500ms 去抖，
/// 记录开销可以忽略。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchEventTrace {
    /// 事件涉及的路径（平台上报的原始路径）。
    pub path: PathBuf,
    /// 事件类型，即 `EventKind` 的 `Debug` 形式（macOS 上常见 `Any`）。
    pub kind: String,
    /// 过滤层的决策：`accepted`，或拒绝原因（`ledger-hit`、`quiet-window`、
    /// `git-busy`、`gitdir`、`noise`、`not-workspace-file`、`read-only`）。
    pub decision: String,
    /// 宿主对这条已接受事件的处理结果；尚未处理时为 `None`。
    pub outcome: Option<String>,
}

impl WatchEventTrace {
    /// 断言消息里的一行可读描述。
    pub fn describe(&self) -> String {
        format!(
            "path={:?} kind={} decision={}{}",
            self.path,
            self.kind,
            self.decision,
            match &self.outcome {
                Some(outcome) => format!(" outcome={outcome}"),
                None => String::new(),
            }
        )
    }
}

/// 过滤器的决策名：事件被放行。
const DECISION_ACCEPTED: &str = "accepted";

/// 诊断轨迹。只保留最近一条，内存有界。
#[derive(Default)]
struct Trace {
    last_accepted: Option<WatchEventTrace>,
    last_decision: Option<WatchEventTrace>,
}

/// 事件过滤状态。由监听回调（notify 自己的线程）与宿主共享。
pub struct ChangeFilter {
    root: PathBuf,
    git_dir: Option<PathBuf>,
    ledger: Mutex<HashMap<PathBuf, LedgerEntry>>,
    quiet: Mutex<HashMap<PathBuf, Instant>>,
    git_busy: AtomicBool,
    trace: Mutex<Trace>,
}

impl ChangeFilter {
    pub fn new(root: PathBuf, git_dir: Option<PathBuf>) -> Self {
        Self {
            root,
            git_dir,
            ledger: Mutex::new(HashMap::new()),
            quiet: Mutex::new(HashMap::new()),
            git_busy: AtomicBool::new(false),
            trace: Mutex::new(Trace::default()),
        }
    }

    /// 记录一次自身写入。写文件**之前**调用。
    ///
    /// 同时开一个静默窗口：`rename` 之后编辑器与杀毒软件可能再触碰一次文件，那时内容
    /// 未必还是刚写下的字节，哈希对不上，只能靠静默窗口兜住。
    pub fn record_self_write(&self, path: &Path, bytes: &[u8]) {
        let now = Instant::now();
        self.remember_content(path, bytes, now);
        lock(&self.quiet).insert(path.to_path_buf(), now);
    }

    /// 记录宿主**自己读到的**文件内容，算作自写。
    ///
    /// 宿主重载配置时读文件，这次读在 macOS/Windows 上会被上报成一次像模像样的修改事件
    /// （macOS 常是 `EventKind::Any`，无法按事件类型丢弃）。把读到的字节记进账本就够了：
    /// 随后的重复事件内容字节相同，被第 2 层吞掉。
    ///
    /// 与 [`Self::record_self_write`] 不同，这里**不**开静默窗口：重载是一次读，文件内容
    /// 没有变化，不该顺带屏蔽内容确实变了的紧随修改。
    pub fn record_own_read(&self, path: &Path, bytes: &[u8]) {
        self.remember_content(path, bytes, Instant::now());
    }

    fn remember_content(&self, path: &Path, bytes: &[u8], at: Instant) {
        lock(&self.ledger).insert(
            path.to_path_buf(),
            LedgerEntry {
                hash: hash_bytes(bytes),
                at,
            },
        );
    }

    /// Git 操作忙标志。为真时丢弃全部事件。
    pub fn set_git_busy(&self, busy: bool) {
        self.git_busy.store(busy, Ordering::SeqCst);
    }

    /// 该路径的变更是否应视为「需要处理的外部修改」。
    ///
    /// 每次决策都记进诊断轨迹（见 [`WatchEventTrace`]）：跨平台缺陷 2 的剩余问题正是
    /// 「不知道是哪种事件、从哪个分支漏出去的」。
    pub fn accept(&self, path: &Path, kind: &EventKind) -> bool {
        let decision = self.decide(path);
        let trace = WatchEventTrace {
            path: path.to_path_buf(),
            kind: format!("{kind:?}"),
            decision: decision.to_string(),
            outcome: None,
        };
        {
            let mut state = lock(&self.trace);
            if decision == DECISION_ACCEPTED {
                state.last_accepted = Some(trace.clone());
            }
            state.last_decision = Some(trace);
        }
        decision == DECISION_ACCEPTED
    }

    /// 记录一条被 `is_modification` 丢掉的只读/其它事件（诊断用）。
    pub fn record_dropped(&self, path: &Path, kind: &EventKind, decision: &str) {
        let trace = WatchEventTrace {
            path: path.to_path_buf(),
            kind: format!("{kind:?}"),
            decision: decision.to_string(),
            outcome: None,
        };
        lock(&self.trace).last_decision = Some(trace);
    }

    /// 记录宿主对一条已接受事件的处理结果（诊断用），例如内容未变的无操作。
    pub fn record_outcome(&self, path: &Path, outcome: &str) {
        let mut state = lock(&self.trace);
        if let Some(accepted) = state.last_accepted.as_mut() {
            if accepted.path == path {
                accepted.outcome = Some(outcome.to_string());
            }
        }
    }

    /// 过滤决策本体，返回决策名。
    fn decide(&self, path: &Path) -> &'static str {
        if self.git_busy.load(Ordering::SeqCst) {
            return "git-busy";
        }
        // gitdir（`.git/`）内部的变化一律忽略：status 查询会改写 `.git/index`。
        if let Some(git_dir) = &self.git_dir {
            if path.starts_with(git_dir) {
                return "gitdir";
            }
        }
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        if is_noise(&name) {
            return "noise";
        }
        if !self.is_owned(path) {
            return "not-workspace-file";
        }

        let now = Instant::now();
        // 第 2 层：内容哈希。命中后**保留**条目到 `LEDGER_TTL` 到期，而不是删掉它：
        // 一次逻辑写入在 macOS/Windows 上会产生多条事件记录，每条都得能继续对上哈希
        // （见模块文档「跨平台」一节）。命中只刷新静默时间戳，给第 3 层兜底。
        let matched = {
            let mut ledger = lock(&self.ledger);
            ledger.retain(|_, entry| now.duration_since(entry.at) < LEDGER_TTL);
            match ledger.get(path) {
                Some(entry) if content_hash(path) == Some(entry.hash) => true,
                _ => false,
            }
        };
        if matched {
            lock(&self.quiet).insert(path.to_path_buf(), now);
            return "ledger-hit";
        }
        // 第 3 层：每路径静默窗口。内容哈希对不上（文件在 `rename` 后又被改写）或文件
        // 已被删除时，靠它吞掉写入后紧随的噪声事件。
        {
            let mut quiet = lock(&self.quiet);
            quiet.retain(|_, at| now.duration_since(*at) < LEDGER_TTL);
            if let Some(at) = quiet.get(path) {
                if now.duration_since(*at) < QUIET_WINDOW {
                    return "quiet-window";
                }
            }
        }
        DECISION_ACCEPTED
    }

    /// 最近一次被接受的事件（诊断用）。
    pub fn last_accepted_event(&self) -> Option<WatchEventTrace> {
        lock(&self.trace).last_accepted.clone()
    }

    /// 最近一次过滤决策，包括被拒绝的事件（诊断用）。
    pub fn last_decision(&self) -> Option<WatchEventTrace> {
        lock(&self.trace).last_decision.clone()
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
fn is_noise(name: &str) -> bool {
    name.ends_with(TEMP_SUFFIX)
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

/// 字节内容的哈希。宿主的「已应用内容」判据与账本共用同一个实现。
pub(crate) fn hash_bytes(bytes: &[u8]) -> u64 {
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
        let mut debouncer = new_debouncer(DEBOUNCE, None, move |result: DebounceEventResult| {
            let events = match result {
                Ok(events) => events,
                // 监听错误（例如 inotify 上限）不改变宿主状态：等下一次有效事件。
                Err(_) => return,
            };
            for event in events {
                let kind = &event.event.kind;
                // 只读事件不算修改：算哈希时读文件本身会再生成 Access 事件。
                let modification = is_modification(kind);
                for path in &event.event.paths {
                    if !modification {
                        callback_filter.record_dropped(path, kind, "read-only");
                        continue;
                    }
                    if callback_filter.accept(path, kind) {
                        let _ = sender.send(path.clone());
                    }
                }
            }
        })
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

    /// 记录宿主自己读到的文件内容（重载读），让随之而来的同内容事件被吞掉。
    pub fn record_own_read(&self, path: &Path, bytes: &[u8]) {
        self.filter.record_own_read(path, bytes);
    }

    /// 记录宿主对一条已接受事件的处理结果（诊断用）。
    pub fn record_outcome(&self, path: &Path, outcome: &str) {
        self.filter.record_outcome(path, outcome);
    }

    /// 最近一次被接受的事件（诊断用）。
    pub fn last_accepted_event(&self) -> Option<WatchEventTrace> {
        self.filter.last_accepted_event()
    }

    /// 最近一次过滤决策，包括被拒绝的事件（诊断用）。
    pub fn last_decision(&self) -> Option<WatchEventTrace> {
        self.filter.last_decision()
    }

    /// Git 操作忙标志：操作期间丢弃事件（见模块文档第 4 层）。
    pub fn set_git_busy(&self, busy: bool) {
        self.filter.set_git_busy(busy);
    }
}
