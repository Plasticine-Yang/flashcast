//! 配置工作区的远端同步：状态、仅快进拉取、显式推送与阻塞状态处理
//! （ADR §9，spec「Git 同步优先处理无冲突的快进拉取与推送」）。
//!
//! 设计要点：
//!
//! - **错误分类先于文案**：网络失败 / 鉴权失败 / 远端拒绝 / 分叉 / 冲突 /
//!   进行中的 Git 操作各是一个独立分类（[`SyncBlockKind`]），界面据此分支，
//!   中文指引只是分类的具体说法，绝不用错误文本做唯一判断。
//! - **拉取只快进**：`fetch` 之后用 `MergeAnalysis` 决策。`ANALYSIS_NORMAL`
//!   是**分叉**（两边都有对方没有的提交），此时拒绝并让用户在外部处理；
//!   没有内置三方合并编辑器。
//! - **绝不 force、绝不丢弃**：检出只用 `CheckoutBuilder::safe()`（默认策略，
//!   允许新建文件但不覆盖已有修改）。拉到一半失败时把分支引用与
//!   `.git/index` 按字节还原。所有阻塞路径在动手之前就返回，工作区与
//!   暂存内容逐字节不变。
//! - **拉取前先判脏**：任何未提交改动（已暂存 / 未暂存 / 未跟踪 / 冲突）
//!   都先于网络操作被拦下；被忽略的文件不算脏。这比 `git pull` 更严格
//!   （git 只在会被覆盖时才拒绝），代价是用户必须先提交才能拉取，
//!   换来的是「任何情况下都不会丢内容」。
//! - **推送不搬运工作区**：推送只传输已提交的对象，未提交修改不会被丢弃，
//!   因此不做阻塞，只在状态里如实呈现。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};

use crate::clone::{self, CredentialProvider};
use crate::git;
use crate::workspace::{write_atomic, Workspace, WorkspaceReload, WorkspaceRemote};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

// ---------------------------------------------------------------------------
// 进度与取消
// ---------------------------------------------------------------------------

/// 一次同步所处的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncPhase {
    Idle,
    Fetching,
    Pushing,
    Done,
    Failed,
    Cancelled,
}

impl SyncPhase {
    pub fn label_zh(self) -> &'static str {
        match self {
            SyncPhase::Idle => "空闲",
            SyncPhase::Fetching => "拉取中",
            SyncPhase::Pushing => "推送中",
            SyncPhase::Done => "已完成",
            SyncPhase::Failed => "失败",
            SyncPhase::Cancelled => "已取消",
        }
    }

    pub fn is_running(self) -> bool {
        matches!(self, SyncPhase::Fetching | SyncPhase::Pushing)
    }
}

/// 同步进度快照（UI 轮询）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub phase: SyncPhase,
    /// 已收到的对象数（网络传输才会更新；本地路径传输为 0）。
    pub received_objects: usize,
    pub total_objects: usize,
    pub received_bytes: usize,
    /// 回调触发次数，便于判断进度是否在推进。
    pub updates: u64,
    /// 结束时的中文说明。
    pub message: Option<String>,
}

impl Default for SyncProgress {
    fn default() -> Self {
        Self {
            phase: SyncPhase::Idle,
            received_objects: 0,
            total_objects: 0,
            received_bytes: 0,
            updates: 0,
            message: None,
        }
    }
}

/// 同步的进度与取消信号。克隆用的 [`crate::clone::CloneControl`] 是同一模式，
/// 区别是这里还要区分「拉取」与「推送」两个阶段。
///
/// 取消方式：fetch 让 `transfer_progress` 返回 `false`；push 让
/// `push_negotiation` 返回 `Err`（它的错误会中止推送）。
#[derive(Debug, Clone, Default)]
pub struct SyncControl {
    progress: Arc<Mutex<SyncProgress>>,
    cancelled: Arc<AtomicBool>,
}

impl SyncControl {
    pub fn new() -> Self {
        Self::default()
    }

    /// 进入某个阶段并清空计数。**不**清除取消标志：调用方可以在开始前先请求取消
    /// （与 `CloneControl` 的用法一致），每次同步由 [`crate::Host`] 使用新的信号。
    pub fn reset(&self, phase: SyncPhase) {
        *lock(&self.progress) = SyncProgress {
            phase,
            ..SyncProgress::default()
        };
    }

    pub fn progress(&self) -> SyncProgress {
        lock(&self.progress).clone()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn finish(&self, phase: SyncPhase, message: Option<String>) {
        let mut progress = lock(&self.progress);
        progress.phase = phase;
        progress.message = message;
    }

    fn update(&self, change: impl FnOnce(&mut SyncProgress)) {
        change(&mut lock(&self.progress));
    }
}

// ---------------------------------------------------------------------------
// 阻塞 / 失败分类
// ---------------------------------------------------------------------------

/// 机器可读的同步阻塞 / 失败分类。界面据此分支，不解析中文文案。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncBlockKind {
    /// 尚未关联配置工作区。
    NoWorkspace,
    /// 工作区不是 Git 仓库。
    NotARepository,
    /// 有未提交改动（已暂存 / 未暂存 / 未跟踪）。
    DirtyWorktree,
    /// 本地与远端各自都有对方没有的提交。
    Diverged,
    /// 索引里存在未解决的合并冲突。
    Conflicts,
    /// 合并 / 变基 / 拣选 / 回退 / 二分查找进行中。
    OperationInProgress,
    /// HEAD 处于分离状态。
    DetachedHead,
    /// 仓库没有配置远端。
    NoRemote,
    /// 当前分支没有上游关系（不知道往哪里拉 / 推）。
    NoUpstream,
    /// 远端没有这个分支（还没推送过）。
    RemoteBranchMissing,
    /// 鉴权失败。
    AuthFailed,
    /// 离线 / DNS / 连接被拒 / 超时。
    Offline,
    /// TLS 证书问题。
    Certificate,
    /// 远端返回错误（非鉴权类，例如地址不存在、无权限）。
    RemoteError,
    /// 远端拒绝本次推送（非快进等）。
    RemoteRejected,
    /// 没有需要推送的提交。
    NothingToPush,
    /// 历史无关，无法合并。
    UnrelatedHistories,
    /// 其它 Git 失败。
    Git,
}

impl SyncBlockKind {
    /// 稳定的机器可读代码（UI 用它选择文案与图标）。
    pub fn code(self) -> &'static str {
        match self {
            SyncBlockKind::NoWorkspace => "noWorkspace",
            SyncBlockKind::NotARepository => "notARepository",
            SyncBlockKind::DirtyWorktree => "dirtyWorktree",
            SyncBlockKind::Diverged => "diverged",
            SyncBlockKind::Conflicts => "conflicts",
            SyncBlockKind::OperationInProgress => "operationInProgress",
            SyncBlockKind::DetachedHead => "detachedHead",
            SyncBlockKind::NoRemote => "noRemote",
            SyncBlockKind::NoUpstream => "noUpstream",
            SyncBlockKind::RemoteBranchMissing => "remoteBranchMissing",
            SyncBlockKind::AuthFailed => "authFailed",
            SyncBlockKind::Offline => "offline",
            SyncBlockKind::Certificate => "certificate",
            SyncBlockKind::RemoteError => "remoteError",
            SyncBlockKind::RemoteRejected => "remoteRejected",
            SyncBlockKind::NothingToPush => "nothingToPush",
            SyncBlockKind::UnrelatedHistories => "unrelatedHistories",
            SyncBlockKind::Git => "git",
        }
    }

    pub fn label_zh(self) -> &'static str {
        match self {
            SyncBlockKind::NoWorkspace => "尚未关联工作区",
            SyncBlockKind::NotARepository => "不是 Git 仓库",
            SyncBlockKind::DirtyWorktree => "有未提交修改",
            SyncBlockKind::Diverged => "历史已分叉",
            SyncBlockKind::Conflicts => "存在合并冲突",
            SyncBlockKind::OperationInProgress => "Git 操作进行中",
            SyncBlockKind::DetachedHead => "分离 HEAD",
            SyncBlockKind::NoRemote => "未配置远端",
            SyncBlockKind::NoUpstream => "未配置上游分支",
            SyncBlockKind::RemoteBranchMissing => "远端还没有该分支",
            SyncBlockKind::AuthFailed => "鉴权失败",
            SyncBlockKind::Offline => "网络不可用",
            SyncBlockKind::Certificate => "证书校验失败",
            SyncBlockKind::RemoteError => "远端返回错误",
            SyncBlockKind::RemoteRejected => "远端拒绝了推送",
            SyncBlockKind::NothingToPush => "没有需要推送的提交",
            SyncBlockKind::UnrelatedHistories => "历史无关",
            SyncBlockKind::Git => "Git 操作失败",
        }
    }
}

/// 一个同步阻塞状态：分类 + 具体情况 + 可操作的中文指引。
///
/// 指引一律指向「在应用外部处理」，例如用系统 Git 解决冲突或在中止合并后
/// 回到本应用点「重新检测」。首版不提供内置三方合并编辑器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncBlock {
    /// 分类的机器可读代码（等于 [`SyncBlockKind::code`]）。
    pub code: String,
    /// 分类的中文短标签。
    pub label: String,
    /// 具体情况（已脱敏）。
    pub detail: String,
    /// 可操作的中文指引。
    pub hint: String,
}

/// 同步失败 / 阻塞的原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SyncError {
    #[error("尚未关联配置工作区，无法同步")]
    NoWorkspace,
    #[error("当前工作区不是 Git 仓库，无法同步：{}", .0.display())]
    NotARepository(PathBuf),
    #[error("工作区有未提交改动（{}），请先提交或移除后再同步", .0)]
    DirtyWorktree(DirtyDetail),
    #[error("本地与远端历史已分叉（本地领先 {ahead} 个提交、落后 {behind} 个提交），已停止同步：分叉需要人工合并，首版不自动合并、不强推")]
    Diverged { ahead: usize, behind: usize },
    #[error("索引里存在未解决的合并冲突：{}", .0.join("、"))]
    Conflicts(Vec<String>),
    #[error("{0}")]
    OperationInProgress(String),
    #[error("HEAD 处于分离状态（未指向任何分支），无法同步；请先在外部切换回分支")]
    DetachedHead,
    #[error("当前工作区还没有配置远端仓库，无法同步；请先在外部执行 git remote add origin <地址>")]
    NoRemote,
    #[error("分支 {branch} 还没有上游关系，无法判断同步目标；请先在外部执行 git push -u <远端> {branch} 或 git branch --set-upstream-to=<远端>/{branch}")]
    NoUpstream { branch: String },
    #[error("远端还没有分支 {0}，请先推送一次")]
    RemoteBranchMissing(String),
    #[error("鉴权失败：{0}")]
    AuthFailed(String),
    #[error("网络不可用：{0}")]
    Offline(String),
    #[error("TLS 证书校验失败：{0}")]
    Certificate(String),
    #[error("远端返回错误：{0}")]
    RemoteError(String),
    #[error("远端拒绝了本次推送：{0}")]
    RemoteRejected(String),
    #[error("没有需要推送的提交：本地与远端已经一致")]
    NothingToPush,
    #[error("本地与远端历史无关，无法快进合并；请先在外部处理两个仓库的关系")]
    UnrelatedHistories,
    #[error("同步已取消，未改动任何内容")]
    Cancelled,
    #[error("Git 操作失败：{0}")]
    Git(String),
}

/// 未提交改动的构成。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DirtyDetail {
    pub staged: bool,
    pub unstaged: bool,
    pub untracked: bool,
}

impl DirtyDetail {
    fn describe(&self) -> String {
        let mut parts = Vec::new();
        if self.staged {
            parts.push("已暂存改动");
        }
        if self.unstaged {
            parts.push("未暂存改动");
        }
        if self.untracked {
            parts.push("未跟踪文件");
        }
        if parts.is_empty() {
            parts.push("改动");
        }
        parts.join(" + ")
    }
}

impl std::fmt::Display for DirtyDetail {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.describe())
    }
}

impl SyncError {
    pub fn kind(&self) -> SyncBlockKind {
        match self {
            SyncError::NoWorkspace => SyncBlockKind::NoWorkspace,
            SyncError::NotARepository(_) => SyncBlockKind::NotARepository,
            SyncError::DirtyWorktree(_) => SyncBlockKind::DirtyWorktree,
            SyncError::Diverged { .. } => SyncBlockKind::Diverged,
            SyncError::Conflicts(_) => SyncBlockKind::Conflicts,
            SyncError::OperationInProgress(_) => SyncBlockKind::OperationInProgress,
            SyncError::DetachedHead => SyncBlockKind::DetachedHead,
            SyncError::NoRemote => SyncBlockKind::NoRemote,
            SyncError::NoUpstream { .. } => SyncBlockKind::NoUpstream,
            SyncError::RemoteBranchMissing(_) => SyncBlockKind::RemoteBranchMissing,
            SyncError::AuthFailed(_) => SyncBlockKind::AuthFailed,
            SyncError::Offline(_) => SyncBlockKind::Offline,
            SyncError::Certificate(_) => SyncBlockKind::Certificate,
            SyncError::RemoteError(_) => SyncBlockKind::RemoteError,
            SyncError::RemoteRejected(_) => SyncBlockKind::RemoteRejected,
            SyncError::NothingToPush => SyncBlockKind::NothingToPush,
            SyncError::UnrelatedHistories => SyncBlockKind::UnrelatedHistories,
            SyncError::Cancelled => SyncBlockKind::Git,
            SyncError::Git(_) => SyncBlockKind::Git,
        }
    }

    /// 具体情况（面向用户，已脱敏）。
    pub fn detail(&self) -> String {
        match self {
            SyncError::NoWorkspace => "尚未关联配置工作区".to_string(),
            SyncError::NotARepository(path) => path.display().to_string(),
            SyncError::DirtyWorktree(detail) => detail.describe(),
            SyncError::Conflicts(files) => files.join("、"),
            SyncError::Diverged { ahead, behind } => format!("本地领先 {ahead}、落后 {behind}"),
            SyncError::OperationInProgress(state) => state.clone(),
            SyncError::NoUpstream { branch } => format!("分支 {branch}"),
            SyncError::RemoteBranchMissing(branch) => branch.clone(),
            SyncError::AuthFailed(detail)
            | SyncError::Offline(detail)
            | SyncError::Certificate(detail)
            | SyncError::RemoteError(detail)
            | SyncError::RemoteRejected(detail)
            | SyncError::Git(detail) => detail.clone(),
            _ => String::new(),
        }
    }

    /// 可操作的中文指引（在应用外部处理，然后回到本应用点「重新检测」）。
    pub fn hint(&self) -> String {
        let redetect = "处理完成后回到本应用点击「重新检测」即可恢复同步。";
        match self {
            SyncError::NoWorkspace => {
                "先在设置的「工作区」区段选择或克隆一个配置工作区。".to_string()
            }
            SyncError::NotARepository(_) => {
                "在外部执行 git init 把它变成仓库，或重新选择正确的目录。".to_string()
            }
            SyncError::DirtyWorktree(_) => format!(
                "在「变更与提交」区段提交这些改动（或在外部 git commit / git stash），然后重新检测。{redetect}"
            ),
            SyncError::Diverged { .. } => format!(
                "本地与远端都有对方没有的提交，需要人工合并：在外部执行 git pull --no-rebase（或 git fetch 后 git merge / git rebase）解决，首版不提供内置三方合并编辑器，也绝不强推覆盖对方。{redetect}"
            ),
            SyncError::Conflicts(_) => format!(
                "在外部编辑冲突文件并 git add，然后 git commit（合并）或 git merge --abort / git rebase --abort 放弃本次操作。{redetect}"
            ),
            SyncError::OperationInProgress(_) => format!(
                "先在外部完成（git commit）或中止（git merge --abort、git rebase --abort、git cherry-pick --abort）这个操作。{redetect}"
            ),
            SyncError::DetachedHead => format!(
                "在外部执行 git switch <分支> 回到分支。{redetect}"
            ),
            SyncError::NoRemote => format!(
                "在外部执行 git remote add origin <远端地址>，或直接克隆远端仓库作为工作区。{redetect}"
            ),
            SyncError::NoUpstream { branch } => format!(
                "在外部执行 git push -u origin {branch} 建立上游关系（或 git branch --set-upstream-to=origin/{branch}）。{redetect}"
            ),
            SyncError::RemoteBranchMissing(branch) => format!(
                "远端还没有 {branch} 分支，先点「推送」把本地分支推上去，再拉取。"
            ),
            SyncError::AuthFailed(_) => format!(
                "在设置的同步区段为该主机填写访问令牌（保存在本机设备目录），或确认系统 git 的凭证 helper / ssh-agent 可用；令牌不会写入工作区或日志。{redetect}"
            ),
            SyncError::Offline(_) => format!(
                "检查网络、代理与远端地址；离线时本地搜索、插件与设置仍然可用，稍后重试即可。{redetect}"
            ),
            SyncError::Certificate(_) => {
                "检查系统时间与根证书；自签名证书不受支持。".to_string()
            }
            SyncError::RemoteError(_) => format!(
                "确认远端地址、访问权限与分支是否存在。{redetect}"
            ),
            SyncError::RemoteRejected(_) => format!(
                "远端有你没有的提交，请先拉取；本应用不会强推覆盖远端历史。{redetect}"
            ),
            SyncError::NothingToPush => "本地没有新的提交，无需推送。".to_string(),
            SyncError::UnrelatedHistories => format!(
                "两个仓库的历史没有共同祖先，请先在外部确认它们是否属于同一个配置仓库。{redetect}"
            ),
            SyncError::Cancelled => "已取消，未改动任何内容。".to_string(),
            SyncError::Git(_) => format!("Git 命令失败，请查看具体情况。{redetect}"),
        }
    }

    pub fn block(&self) -> SyncBlock {
        SyncBlock {
            code: self.kind().code().to_string(),
            label: self.kind().label_zh().to_string(),
            detail: self.detail(),
            hint: self.hint(),
        }
    }
}

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

/// 当前工作区的同步状态（设置页据此展示，也用于判断「能否同步」）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// 当前工作区是不是可读取的 Git 仓库。
    pub repository: bool,
    /// 当前分支名（short）；分离 HEAD 时为 `None`。
    pub branch: Option<String>,
    /// HEAD 是否为分离状态。
    pub detached: bool,
    /// 远端关系（地址已去掉 userinfo）。
    pub remote: Option<WorkspaceRemote>,
    /// 远端名，例如 `origin`。
    pub remote_name: Option<String>,
    /// 远端地址（已脱敏）。
    pub remote_url: Option<String>,
    /// 上游跟踪分支，例如 `origin/main`。
    pub upstream: Option<String>,
    /// 本地领先远端的提交数（上游跟踪引用不存在时为 0）。
    pub ahead: usize,
    /// 本地落后远端的提交数（上游跟踪引用不存在时为 0）。
    pub behind: usize,
    /// 本地是否已经有上游跟踪引用（决定 ahead/behind 是否可信）。
    pub tracking: bool,
    /// 是否存在未提交改动（含未跟踪文件；被忽略的文件不算）。
    pub dirty: bool,
    /// 已暂存改动。
    pub staged: bool,
    /// 未暂存改动。
    pub unstaged: bool,
    /// 未跟踪文件。
    pub untracked: bool,
    /// 索引里存在未解决的冲突。
    pub conflicted: bool,
    /// 进行中的 Git 操作的中文说明。
    pub state: Option<String>,
    /// 首次拉取是否可以安全执行。
    pub can_pull: bool,
    /// 推送是否可以执行（未提交修改不阻塞推送）。
    pub can_push: bool,
    /// 本地是否没有需要推送的提交。
    pub nothing_to_push: bool,
    /// 拉取的阻塞原因（含指引）；`None` 表示可以拉取。
    pub blocking: Option<SyncBlock>,
    /// 推送的阻塞原因（含指引）；`None` 表示可以推送。
    pub push_blocking: Option<SyncBlock>,
    /// 是否有同步操作正在进行（网络操作期间为 `true`）。
    pub busy: bool,
    /// 读取状态失败时的中文原因。
    pub error: Option<String>,
}

impl SyncStatus {
    /// 尚未关联工作区。
    pub fn unlinked() -> Self {
        Self {
            repository: false,
            branch: None,
            detached: false,
            remote: None,
            remote_name: None,
            remote_url: None,
            upstream: None,
            ahead: 0,
            behind: 0,
            tracking: false,
            dirty: false,
            staged: false,
            unstaged: false,
            untracked: false,
            conflicted: false,
            state: None,
            can_pull: false,
            can_push: false,
            nothing_to_push: false,
            blocking: Some(SyncError::NoWorkspace.block()),
            push_blocking: Some(SyncError::NoWorkspace.block()),
            busy: false,
            error: None,
        }
    }

    /// 无法读取 Git 状态。
    pub fn failed(error: String) -> Self {
        let block = SyncError::Git(error.clone()).block();
        Self {
            blocking: Some(block.clone()),
            push_blocking: Some(block),
            error: Some(error),
            ..Self::unlinked()
        }
    }

    /// 是否有可能同步（拉取或推送至少一个可用）。
    pub fn sync_possible(&self) -> bool {
        self.can_pull || self.can_push
    }
}

/// 读取工作区的同步状态。**只读**：不发网络请求，也不写 `.git/index`。
///
/// `recorded` 是设备本地记录的远端关系（ticket 14 的克隆关系）；仓库里能读到
/// 远端时以仓库为准，读不到时用它兜底，这样界面仍能显示「应该同步到哪里」。
pub fn status(workspace: &Workspace, recorded: Option<&WorkspaceRemote>) -> SyncStatus {
    let repo = match git::open(workspace) {
        Ok(repo) => repo,
        Err(error) if matches!(error, git::GitError::NotARepository(_)) => {
            return SyncStatus {
                blocking: Some(SyncError::NotARepository(workspace.root().to_path_buf()).block()),
                push_blocking: Some(
                    SyncError::NotARepository(workspace.root().to_path_buf()).block(),
                ),
                ..SyncStatus::unlinked()
            }
        }
        Err(error) => return SyncStatus::failed(error.to_string()),
    };

    let live_remote = workspace.remote();
    let remote = live_remote.clone().or_else(|| recorded.cloned());

    let inspected = match inspect(&repo) {
        Ok(inspected) => inspected,
        Err(error) => return SyncStatus::failed(error.to_string()),
    };

    let pull_blocker = status_blocker(&inspected);
    let push_blocker = push_blocker(&inspected);

    SyncStatus {
        repository: true,
        branch: inspected.branch.clone(),
        detached: inspected.detached,
        remote_name: remote.as_ref().map(|remote| remote.name.clone()),
        remote_url: remote.as_ref().map(|remote| remote.url.clone()),
        upstream: inspected
            .upstream
            .as_ref()
            .map(|upstream| format!("{}/{}", upstream.remote, upstream.branch)),
        remote,
        ahead: inspected.ahead,
        behind: inspected.behind,
        tracking: inspected.tracking,
        dirty: inspected.staged || inspected.unstaged || inspected.untracked,
        staged: inspected.staged,
        unstaged: inspected.unstaged,
        untracked: inspected.untracked,
        conflicted: inspected.conflicted,
        state: inspected.in_progress.clone(),
        can_pull: pull_blocker.is_none(),
        can_push: push_blocker.is_none(),
        nothing_to_push: push_blocker.is_none() && inspected.tracking && inspected.ahead == 0,
        blocking: pull_blocker.map(|error| error.block()),
        push_blocking: push_blocker.map(|error| error.block()),
        busy: false,
        error: None,
    }
}

// ---------------------------------------------------------------------------
// 仓库探测
// ---------------------------------------------------------------------------

/// 同步目标：远端名 + 远端分支 + 本地跟踪引用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Upstream {
    /// 远端名，例如 `origin`。
    pub remote: String,
    /// 远端分支短名，例如 `main`。
    pub branch: String,
    /// 本地跟踪引用，例如 `refs/remotes/origin/main`。
    pub tracking: String,
}

/// 一次只读探测的结果。
struct Inspected {
    branch: Option<String>,
    detached: bool,
    has_remote: bool,
    upstream: Option<Upstream>,
    staged: bool,
    unstaged: bool,
    untracked: bool,
    conflicted: bool,
    conflict_paths: Vec<String>,
    in_progress: Option<String>,
    ahead: usize,
    behind: usize,
    tracking: bool,
}

/// 只读探测仓库：分支、上游、脏状态、冲突、进行中的操作、领先 / 落后。
fn inspect(repo: &git2::Repository) -> Result<Inspected, SyncError> {
    let detached = repo.head_detached().unwrap_or(false);
    let branch = if detached {
        None
    } else {
        repo.head()
            .ok()
            .and_then(|head| head.shorthand().ok().map(str::to_string))
            .or_else(|| symbolic_branch(repo))
    };

    let (staged, unstaged, untracked) = dirty_flags(repo)?;
    let (conflicted, conflict_paths) = conflicts(repo)?;
    let in_progress = git::in_progress_state(repo);

    // `StringArray::iter()` 的条目是 `Result<Option<&str>>`（可能不是 UTF-8）。
    let remotes: Vec<String> = repo
        .remotes()
        .map(|names| {
            names
                .iter()
                .filter_map(|name| name.ok().flatten().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let has_remote = !remotes.is_empty();

    let upstream = branch
        .as_deref()
        .and_then(|branch| resolve_upstream(repo, branch, &remotes));

    let (ahead, behind, tracking) = match (&branch, &upstream) {
        (Some(branch), Some(upstream)) => ahead_behind(repo, branch, &upstream.tracking),
        _ => (0, 0, false),
    };

    Ok(Inspected {
        branch,
        detached,
        has_remote,
        upstream,
        staged,
        unstaged,
        untracked,
        conflicted,
        conflict_paths,
        in_progress,
        ahead,
        behind,
        tracking,
    })
}

/// 尚未有提交时从 `HEAD` 的符号引用取分支名。
fn symbolic_branch(repo: &git2::Repository) -> Option<String> {
    let target = repo
        .find_reference("HEAD")
        .ok()?
        .symbolic_target()
        .ok()
        .flatten()?
        .to_string();
    target.strip_prefix("refs/heads/").map(str::to_string)
}

/// 未提交改动的三个维度。只读：`no_refresh` 不写 `.git/index`。
///
/// 被忽略的文件不算脏（研究记录的建议：配置工作区里常有被忽略的缓存）。
fn dirty_flags(repo: &git2::Repository) -> Result<(bool, bool, bool), SyncError> {
    let mut options = git2::StatusOptions::new();
    options
        .show(git2::StatusShow::IndexAndWorkdir)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .exclude_submodules(true)
        .no_refresh(true);

    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| SyncError::Git(error.message().to_string()))?;

    let mut staged = false;
    let mut unstaged = false;
    let mut untracked = false;
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_empty() || status == git2::Status::CURRENT {
            continue;
        }
        if status.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        ) {
            staged = true;
        }
        if status.intersects(
            git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        ) || status.contains(git2::Status::CONFLICTED)
        {
            unstaged = true;
        }
        if status.contains(git2::Status::WT_NEW) {
            untracked = true;
        }
    }
    Ok((staged, unstaged, untracked))
}

/// 索引里的未解决冲突。
fn conflicts(repo: &git2::Repository) -> Result<(bool, Vec<String>), SyncError> {
    let index = repo
        .index()
        .map_err(|error| SyncError::Git(error.message().to_string()))?;
    if !index.has_conflicts() {
        return Ok((false, Vec::new()));
    }
    let mut paths: Vec<String> = index
        .conflicts()
        .map_err(|error| SyncError::Git(error.message().to_string()))?
        .filter_map(|conflict| conflict.ok())
        .filter_map(|conflict| {
            let entry = conflict
                .our
                .as_ref()
                .or(conflict.their.as_ref())
                .or(conflict.ancestor.as_ref())?;
            Some(String::from_utf8_lossy(&entry.path).replace('\\', "/"))
        })
        .collect();
    paths.sort();
    paths.dedup();
    Ok((true, paths))
}

/// 上游关系：优先 `branch.<名>.remote` + `branch.<名>.merge`，
/// 其次找一个已经存在的远端跟踪引用 `refs/remotes/<远端>/<分支>`。
fn resolve_upstream(repo: &git2::Repository, branch: &str, remotes: &[String]) -> Option<Upstream> {
    if let Ok(config) = repo.config() {
        let remote = config
            .get_string(&format!("branch.{branch}.remote"))
            .ok()
            .filter(|remote| remote != ".");
        if let Some(remote) = remote {
            if let Ok(merge) = config.get_string(&format!("branch.{branch}.merge")) {
                let short = merge
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&merge)
                    .to_string();
                return Some(Upstream {
                    tracking: format!("refs/remotes/{remote}/{short}"),
                    remote,
                    branch: short,
                });
            }
        }
    }

    // 已经存在跟踪引用（例如外部执行过 git fetch）：据此判断同步目标。
    let mut candidates: Vec<(String, String)> = Vec::new();
    for remote in remotes {
        let prefix = format!("refs/remotes/{remote}/");
        let Ok(refs) = repo.references_glob(&format!("{prefix}*")) else {
            continue;
        };
        for reference in refs.flatten() {
            let Ok(name) = reference.name() else { continue };
            let Some(short) = name.strip_prefix(&prefix) else {
                continue;
            };
            if short == "HEAD" || short.is_empty() {
                continue;
            }
            candidates.push((remote.clone(), short.to_string()));
        }
    }
    candidates.sort();
    candidates.dedup();
    // 同名分支有歧义时不猜：只有唯一候选才认。
    if candidates.len() == 1 {
        let (remote, short) = candidates.into_iter().next()?;
        return Some(Upstream {
            tracking: format!("refs/remotes/{remote}/{short}"),
            remote,
            branch: short,
        });
    }
    None
}

/// 领先 / 落后（与 `git status -sb` 一致）。跟踪引用不存在时返回 `(0, 0, false)`。
fn ahead_behind(repo: &git2::Repository, branch: &str, tracking: &str) -> (usize, usize, bool) {
    let local = repo
        .find_reference(&format!("refs/heads/{branch}"))
        .ok()
        .and_then(|reference| reference.target());
    let upstream = repo
        .find_reference(tracking)
        .ok()
        .and_then(|reference| reference.target());
    match (local, upstream) {
        (Some(local), Some(upstream)) => match repo.graph_ahead_behind(local, upstream) {
            Ok((ahead, behind)) => (ahead, behind, true),
            Err(_) => (0, 0, false),
        },
        _ => (0, 0, false),
    }
}

/// 拉取的阻塞原因（按重要性排序：进行中 → 冲突 → 分离 HEAD → 脏 → 上游）。
fn pull_blocker(inspected: &Inspected) -> Option<SyncError> {
    if let Some(state) = &inspected.in_progress {
        return Some(SyncError::OperationInProgress(state.clone()));
    }
    if inspected.conflicted {
        return Some(SyncError::Conflicts(inspected.conflict_paths.clone()));
    }
    if inspected.detached {
        return Some(SyncError::DetachedHead);
    }
    if inspected.staged || inspected.unstaged || inspected.untracked {
        return Some(SyncError::DirtyWorktree(DirtyDetail {
            staged: inspected.staged,
            unstaged: inspected.unstaged,
            untracked: inspected.untracked,
        }));
    }
    if !inspected.has_remote {
        return Some(SyncError::NoRemote);
    }
    if inspected.upstream.is_none() {
        return Some(SyncError::NoUpstream {
            branch: inspected
                .branch
                .clone()
                .unwrap_or_else(|| "HEAD".to_string()),
        });
    }
    None
}

/// 状态查询的拉取阻塞原因：在 [`pull_blocker`] 之上补上「本地已知分叉」。
///
/// 分叉只有 `fetch` 之后才能确认，因此这里用**本地**远端跟踪引用推算：
/// 同时领先又落后说明两边各有一条对方没有的提交。跟踪引用可能陈旧，
/// 所以这个判断只用于**展示**（提示用户先在外部合并）；[`pull`] 自己仍然
/// 只按本地结构状态阻塞，然后 `fetch` 后按真实的 `MergeAnalysis` 决策，
/// 避免陈旧的跟踪引用把一次正常快进永久挡在门外。
fn status_blocker(inspected: &Inspected) -> Option<SyncError> {
    if let Some(blocker) = pull_blocker(inspected) {
        return Some(blocker);
    }
    if inspected.tracking && inspected.ahead > 0 && inspected.behind > 0 {
        return Some(SyncError::Diverged {
            ahead: inspected.ahead,
            behind: inspected.behind,
        });
    }
    None
}

/// 推送的阻塞原因。未提交改动**不是**阻塞：推送只搬运已提交对象。
fn push_blocker(inspected: &Inspected) -> Option<SyncError> {
    if let Some(state) = &inspected.in_progress {
        return Some(SyncError::OperationInProgress(state.clone()));
    }
    if inspected.conflicted {
        return Some(SyncError::Conflicts(inspected.conflict_paths.clone()));
    }
    if inspected.detached {
        return Some(SyncError::DetachedHead);
    }
    if !inspected.has_remote {
        return Some(SyncError::NoRemote);
    }
    if inspected.upstream.is_none() {
        return Some(SyncError::NoUpstream {
            branch: inspected
                .branch
                .clone()
                .unwrap_or_else(|| "HEAD".to_string()),
        });
    }
    None
}

// ---------------------------------------------------------------------------
// 错误分类
// ---------------------------------------------------------------------------

/// 把 `git2::Error` 分到具体的同步失败分类里，并把文本脱敏。
///
/// 先按 `(code, class)` 分桶，再用消息文本在少数候选之间挑选说法；
/// 绝不把原始消息当成唯一依据（libgit2 的文案会随版本与语言变化）。
pub fn classify(error: &git2::Error, secrets: &[String]) -> SyncError {
    use git2::{ErrorClass, ErrorCode};

    let detail = clone::redact_secrets(&clone::redact(error.message()), secrets);
    let lowered = detail.to_ascii_lowercase();
    let has = |words: &[&str]| words.iter().any(|word| lowered.contains(word));

    const NON_FAST_FORWARD: [&str; 5] = [
        "non-fast-forward",
        "not fast-forward",
        "fetch first",
        "cannot push",
        "failed to push some refs",
    ];
    const AUTH: [&str; 8] = [
        "permission denied",
        "authentication",
        "invalid credentials",
        "unauthorized",
        "401",
        "403",
        "no supported authentication",
        "publickey",
    ];
    // 各平台的连接失败措辞不同，都要认：
    // Linux/libcurl 说 "couldn't connect to server" / "connection refused"，
    // Windows 上 libgit2 走 WinHTTP，说的是
    // "A connection with the server could not be established"（CI Windows 腿实测）。
    const OFFLINE: [&str; 14] = [
        "could not resolve host",
        "failed to resolve",
        "connection refused",
        "connection timed out",
        "network is unreachable",
        "no route to host",
        "failed to connect",
        "operation timed out",
        "could not be established",
        "couldn't connect",
        "could not connect",
        "connection reset",
        "server name or address could not be resolved",
        "timed out",
    ];

    if error.code() == ErrorCode::NotFastForward || has(&NON_FAST_FORWARD) {
        return SyncError::RemoteRejected(detail);
    }
    if error.code() == ErrorCode::Certificate || error.class() == ErrorClass::Ssl {
        return SyncError::Certificate(detail);
    }
    if error.code() == ErrorCode::Timeout || has(&OFFLINE) {
        return SyncError::Offline(detail);
    }
    if error.code() == ErrorCode::Auth {
        return SyncError::AuthFailed(detail);
    }
    if error.class() == ErrorClass::Http {
        if has(&AUTH) {
            return SyncError::AuthFailed(detail);
        }
        return SyncError::RemoteError(detail);
    }
    if error.class() == ErrorClass::Net {
        return SyncError::Offline(detail);
    }
    if error.class() == ErrorClass::Ssh || has(&AUTH) {
        return SyncError::AuthFailed(detail);
    }
    if error.class() == ErrorClass::Callback {
        return SyncError::AuthFailed(detail);
    }
    SyncError::Git(detail)
}

// ---------------------------------------------------------------------------
// 拉取（仅快进）
// ---------------------------------------------------------------------------

/// 拉取的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PullResult {
    /// 远端没有新提交。
    UpToDate,
    /// 已快进到远端提交。
    FastForwarded {
        /// 拉取前的提交（空仓库为 `None`）。
        from: Option<String>,
        /// 拉取后的提交。
        to: String,
        /// 前 7 位，便于展示。
        short_to: String,
        /// 本次带入的提交数。
        commits: usize,
    },
}

impl PullResult {
    /// 拉取后的提交；`UpToDate` 时为 `None`。
    pub fn to(&self) -> Option<&str> {
        match self {
            PullResult::UpToDate => None,
            PullResult::FastForwarded { to, .. } => Some(to),
        }
    }

    /// 拉取前的提交；空仓库或 `UpToDate` 时为 `None`。
    pub fn from(&self) -> Option<&str> {
        match self {
            PullResult::UpToDate => None,
            PullResult::FastForwarded { from, .. } => from.as_deref(),
        }
    }

    /// 本次带入的提交数。
    pub fn commits(&self) -> usize {
        match self {
            PullResult::UpToDate => 0,
            PullResult::FastForwarded { commits, .. } => *commits,
        }
    }

    /// 是否真的移动了本地分支。
    pub fn fast_forwarded(&self) -> bool {
        matches!(self, PullResult::FastForwarded { .. })
    }
}

/// 核心层一次成功拉取的结果（还不含宿主侧的重新加载信息）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullReport {
    pub result: PullResult,
    /// 面向用户的中文结果。
    pub message: String,
}

/// 宿主层一次成功拉取的结果：核心报告 + 重新加载后的视图。
///
/// 拉取成功后设置、主题与备忘录都要重新读取，因此这里如实带上重新加载的
/// 结果（[`Host::pull_workspace`](crate::Host::pull_workspace) 填充）。
/// `reload.theme` 是重新解析后的**生效**主题状态；`theme` 是它的 id 摘要。
///
/// 不派生 `Eq`：`WorkspaceReload` 里的主题状态含浮点 token（ticket 06）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullOutcome {
    pub result: PullResult,
    /// 拉取后按真实 Git 状态重建的状态。
    pub status: SyncStatus,
    /// 重新加载生效配置（设置与主题）的结果。
    pub reload: WorkspaceReload,
    /// 拉取后生效的主题 id。
    pub theme: Option<String>,
    /// 拉取后 `memos/` 下的备忘录（仓库相对路径，已排序）。
    pub memos: Vec<String>,
    /// 面向用户的中文结果。
    pub message: String,
}

/// 仅快进拉取。任何阻塞状态都在网络操作之前返回，工作区与索引保持不变。
pub fn pull(
    workspace: &Workspace,
    provider: &CredentialProvider,
    control: &SyncControl,
) -> Result<PullReport, SyncError> {
    let repo = open_repo(workspace)?;
    let inspected = inspect(&repo)?;
    if let Some(blocker) = pull_blocker(&inspected) {
        control.finish(SyncPhase::Failed, Some(blocker.to_string()));
        return Err(blocker);
    }
    if control.is_cancelled() {
        control.finish(SyncPhase::Cancelled, Some("同步已取消".to_string()));
        return Err(SyncError::Cancelled);
    }
    let upstream = inspected
        .upstream
        .clone()
        .expect("pull_blocker 已确认上游存在");

    control.reset(SyncPhase::Fetching);
    let secrets = fetch(&repo, &upstream, provider, control)?;
    if control.is_cancelled() {
        control.finish(SyncPhase::Cancelled, Some("同步已取消".to_string()));
        return Err(SyncError::Cancelled);
    }

    // 实测：显式 refspec 指向远端不存在的分支时，libgit2 只更新不了跟踪引用却
    // 仍返回成功，因此这里自己判断「远端有没有这个分支」。
    let tracking = repo
        .find_reference(&upstream.tracking)
        .map_err(|_| SyncError::RemoteBranchMissing(upstream.branch.clone()))?;
    let annotated = repo
        .reference_to_annotated_commit(&tracking)
        .map_err(|error| SyncError::Git(classify(&error, &secrets).to_string()))?;
    let target = annotated.id();
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(|error| SyncError::Git(classify(&error, &secrets).to_string()))?;

    if analysis.is_up_to_date() {
        control.finish(SyncPhase::Done, Some("已是最新".to_string()));
        return Ok(PullReport {
            result: PullResult::UpToDate,
            message: "远端没有新的提交，本地已是最新".to_string(),
        });
    }

    // 顺序很重要：libgit2 1.9 在**可快进**时同时置位
    // `GIT_MERGE_ANALYSIS_FASTFORWARD | GIT_MERGE_ANALYSIS_NORMAL`
    // （源码 `merge.c`：`ancestor == our_head` 时两个位一起给），
    // 只有真正的分叉才是 `NORMAL` 单独出现。因此必须先判 FASTFORWARD /
    // UNBORN，最后才把 NORMAL 当作分叉——反过来会把每次正常快进都误判成分叉。
    let from = inspected.branch.as_deref().and_then(|branch| {
        repo.find_reference(&format!("refs/heads/{branch}"))
            .ok()
            .and_then(|reference| reference.target())
    });

    if analysis.is_unborn() || analysis.is_fast_forward() {
        return finish_fast_forward(&repo, &inspected, target, from, &secrets, control);
    }

    if analysis.contains(git2::MergeAnalysis::ANALYSIS_NORMAL) {
        let head = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .map(|commit| commit.id());
        let (ahead, behind) = match head {
            Some(head) => repo.graph_ahead_behind(head, target).unwrap_or((0, 0)),
            None => (0, 0),
        };
        control.finish(SyncPhase::Failed, Some("历史已分叉".to_string()));
        return Err(SyncError::Diverged { ahead, behind });
    }

    control.finish(SyncPhase::Failed, Some("无法快进".to_string()));
    Err(SyncError::UnrelatedHistories)
}

/// 执行快进检出并组装报告。
fn finish_fast_forward(
    repo: &git2::Repository,
    inspected: &Inspected,
    target: git2::Oid,
    from: Option<git2::Oid>,
    secrets: &[String],
    control: &SyncControl,
) -> Result<PullReport, SyncError> {
    let branch = inspected.branch.clone().ok_or(SyncError::DetachedHead)?;
    advance_branch(repo, &branch, target, secrets)?;
    let commits = match from {
        Some(from) => repo
            .graph_ahead_behind(target, from)
            .map(|(ahead, _)| ahead)
            .unwrap_or(1),
        None => 1,
    };
    control.finish(SyncPhase::Done, Some("拉取完成".to_string()));
    Ok(PullReport {
        result: PullResult::FastForwarded {
            from: from.map(|oid| oid.to_string()),
            to: target.to_string(),
            short_to: target.to_string()[..7].to_string(),
            commits,
        },
        message: format!(
            "已快进拉取到 {}，共 {} 个提交",
            &target.to_string()[..7],
            commits
        ),
    })
}

/// `fetch` 远端分支到本地跟踪引用。返回本次实际使用过的口令，供脱敏使用。
fn fetch(
    repo: &git2::Repository,
    upstream: &Upstream,
    provider: &CredentialProvider,
    control: &SyncControl,
) -> Result<Vec<String>, SyncError> {
    let secrets: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let mut remote = repo
        .find_remote(&upstream.remote)
        .map_err(|_| SyncError::NoRemote)?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials({
        let provider = provider.clone();
        let secrets = Arc::clone(&secrets);
        move |url, username, allowed| provider.credential(url, username, allowed, &secrets)
    });
    callbacks.transfer_progress({
        let control = control.clone();
        move |stats| {
            control.update(|progress| {
                progress.updates += 1;
                progress.received_objects = stats.received_objects();
                progress.total_objects = stats.total_objects();
                progress.received_bytes = stats.received_bytes();
            });
            !control.is_cancelled()
        }
    });
    callbacks.sideband_progress({
        let control = control.clone();
        move |_data| {
            control.update(|progress| progress.updates += 1);
            !control.is_cancelled()
        }
    });

    let mut options = git2::FetchOptions::new();
    options
        .remote_callbacks(callbacks)
        .download_tags(git2::AutotagOption::All)
        .follow_redirects(git2::RemoteRedirect::All)
        .update_fetchhead(true);

    // 显式 refspec：远端分支 → 本地跟踪引用。加 `+` 是让跟踪引用总是跟远端一致
    // （这是 fetch 的常规语义，与「不强推远端」无关）。
    let refspec = format!("+refs/heads/{}:{}", upstream.branch, upstream.tracking);
    let result = remote.fetch(&[refspec.as_str()], Some(&mut options), None);
    let secrets = lock(&secrets).clone();

    if control.is_cancelled() {
        return Err(SyncError::Cancelled);
    }
    if let Err(error) = result {
        let classified = classify(&error, &secrets);
        // 远端还没有这个分支：单独分类，指引是「先推送一次」。
        if matches!(classified, SyncError::Git(_))
            && error
                .message()
                .to_ascii_lowercase()
                .contains("couldn't find remote ref")
        {
            return Err(SyncError::RemoteBranchMissing(upstream.branch.clone()));
        }
        return Err(classified);
    }
    Ok(secrets)
}

/// 把本地分支快进到目标提交。
///
/// **实测顺序必须是「先检出目标树，再移动分支引用」**：`checkout_head()` 在
/// 分支引用已经被移动之后调用会**静默变成空操作**——git2 0.21 / libgit2 1.9 上
/// 它返回 `Ok(())`，但工作区与索引都不更新（本 ticket 用探针实测过：文件内容
/// 仍是旧提交的）。`Repository::checkout_head` 的文档注释也警告过不要把
/// 「改 HEAD」与它混用。因此这里检出目标提交的树（不依赖 HEAD），随后再移动
/// 分支引用，最后确认 HEAD 指向该分支。
///
/// 检出只用 `safe()`：允许新建文件，但绝不覆盖已有修改（树已确认干净，
/// 这一步是纵深防御）。失败时按字节还原 `.git/index`，并尽量把工作区退回原提交。
fn advance_branch(
    repo: &git2::Repository,
    branch: &str,
    target: git2::Oid,
    secrets: &[String],
) -> Result<(), SyncError> {
    let refname = format!("refs/heads/{branch}");
    let index_path = repo.path().join("index");
    let index_backup = std::fs::read(&index_path).ok();
    let previous = repo
        .find_reference(&refname)
        .ok()
        .and_then(|reference| reference.target());

    // 1) 检出目标提交的树（HEAD 此刻仍指向旧提交，工作区与索引一起更新）。
    let object = repo
        .find_object(target, None)
        .map_err(|error| classify(&error, secrets))?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe().update_index(true);
    if let Err(error) = repo.checkout_tree(&object, Some(&mut checkout)) {
        restore_index(&index_path, index_backup.as_deref());
        return Err(classify(&error, secrets));
    }

    // 2) 移动分支引用；失败时把工作区与索引退回原来的提交。
    let reflog = format!("flashcast: 快进拉取到 {}", &target.to_string()[..7]);
    let moved = match repo.find_reference(&refname) {
        Ok(mut reference) => reference.set_target(target, &reflog).map(|_| ()),
        Err(_) => repo.reference(&refname, target, true, &reflog).map(|_| ()),
    };
    if let Err(error) = moved {
        if let Some(oid) = previous {
            if let Ok(object) = repo.find_object(oid, None) {
                let mut restore = git2::build::CheckoutBuilder::new();
                restore.safe().update_index(true);
                let _ = repo.checkout_tree(&object, Some(&mut restore));
            }
        }
        restore_index(&index_path, index_backup.as_deref());
        return Err(classify(&error, secrets));
    }

    let _ = repo.set_head(&refname);
    Ok(())
}

/// 按字节还原索引文件。
fn restore_index(path: &std::path::Path, backup: Option<&[u8]>) {
    match backup {
        Some(bytes) => {
            let _ = write_atomic(path, bytes);
        }
        None => {
            let _ = std::fs::remove_file(path);
        }
    }
}

// ---------------------------------------------------------------------------
// 推送（显式触发，永不 force）
// ---------------------------------------------------------------------------

/// 核心层一次成功推送的结果（还不含宿主侧重建的状态）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushReport {
    /// 本地分支名。
    pub branch: String,
    /// 远端名。
    pub remote: String,
    /// 实际更新的远端引用（本地 → 远端）。
    pub updated: Vec<PushUpdateView>,
    /// 面向用户的中文结果。
    pub message: String,
}

/// 宿主层一次成功推送的结果：核心报告 + 推送后重建的状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushOutcome {
    /// 本地分支名。
    pub branch: String,
    /// 远端名。
    pub remote: String,
    /// 实际更新的远端引用（本地 → 远端）。
    pub updated: Vec<PushUpdateView>,
    /// 推送后按真实 Git 状态重建的状态。
    pub status: SyncStatus,
    /// 面向用户的中文结果。
    pub message: String,
}

/// 一个被推送的引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushUpdateView {
    pub local: String,
    pub remote: String,
    /// 本地一侧的提交（本地引用名不可解析时为 oid 文本）。
    pub local_oid: String,
    /// 远端一侧的提交；远端还没有这个引用时是全零 oid。
    pub remote_oid: String,
}

impl PushUpdateView {
    /// 这个引用是否真的会被更新（两边 oid 相同就是「已经一致」）。
    pub fn moves(&self) -> bool {
        self.local_oid != self.remote_oid
    }
}

/// 显式推送当前分支到它的上游。refspec 不带 `+`，因此远端拒绝非快进时不会覆盖。
pub fn push(
    workspace: &Workspace,
    provider: &CredentialProvider,
    control: &SyncControl,
) -> Result<PushReport, SyncError> {
    let repo = open_repo(workspace)?;
    let inspected = inspect(&repo)?;
    if let Some(blocker) = push_blocker(&inspected) {
        control.finish(SyncPhase::Failed, Some(blocker.to_string()));
        return Err(blocker);
    }
    if control.is_cancelled() {
        control.finish(SyncPhase::Cancelled, Some("同步已取消".to_string()));
        return Err(SyncError::Cancelled);
    }
    let branch = inspected.branch.clone().ok_or(SyncError::DetachedHead)?;
    let upstream = inspected
        .upstream
        .clone()
        .expect("push_blocker 已确认上游存在");

    control.reset(SyncPhase::Pushing);
    let secrets: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let updates: Arc<Mutex<Vec<PushUpdateView>>> = Arc::new(Mutex::new(Vec::new()));
    let rejections: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut remote = repo
        .find_remote(&upstream.remote)
        .map_err(|_| SyncError::NoRemote)?;

    let mut callbacks = git2::RemoteCallbacks::new();
    callbacks.credentials({
        let provider = provider.clone();
        let secrets = Arc::clone(&secrets);
        move |url, username, allowed| provider.credential(url, username, allowed, &secrets)
    });
    callbacks.push_negotiation({
        let control = control.clone();
        let updates = Arc::clone(&updates);
        move |negotiated| {
            if control.is_cancelled() {
                return Err(git2::Error::from_str("推送已取消"));
            }
            let mut collected = lock(&updates);
            collected.clear();
            for update in negotiated {
                collected.push(PushUpdateView {
                    local: update
                        .src_refname()
                        .map(str::to_string)
                        .unwrap_or_else(|_| update.src().to_string()),
                    remote: update
                        .dst_refname()
                        .map(str::to_string)
                        .unwrap_or_else(|_| update.dst().to_string()),
                    local_oid: update.src().to_string(),
                    remote_oid: update.dst().to_string(),
                });
            }
            Ok(())
        }
    });
    callbacks.push_update_reference({
        let rejections = Arc::clone(&rejections);
        move |refname, status| {
            if let Some(message) = status {
                lock(&rejections).push(format!("{refname}：{message}"));
            }
            Ok(())
        }
    });
    callbacks.push_transfer_progress({
        let control = control.clone();
        move |current, total, bytes| {
            control.update(|progress| {
                progress.updates += 1;
                progress.received_objects = current;
                progress.total_objects = total;
                progress.received_bytes = bytes;
            });
        }
    });

    let mut options = git2::PushOptions::new();
    options
        .remote_callbacks(callbacks)
        .follow_redirects(git2::RemoteRedirect::All);

    let refspec = format!("refs/heads/{branch}:refs/heads/{}", upstream.branch);
    let result = remote.push(&[refspec.as_str()], Some(&mut options));
    let secrets = lock(&secrets).clone();
    let updates = lock(&updates).clone();
    let rejections = lock(&rejections).clone();

    if control.is_cancelled() {
        control.finish(SyncPhase::Cancelled, Some("同步已取消".to_string()));
        return Err(SyncError::Cancelled);
    }
    if let Err(error) = result {
        let classified = classify(&error, &secrets);
        control.finish(SyncPhase::Failed, Some(classified.to_string()));
        return Err(classified);
    }
    if !rejections.is_empty() {
        let error =
            SyncError::RemoteRejected(clone::redact_secrets(&rejections.join("；"), &secrets));
        control.finish(SyncPhase::Failed, Some(error.to_string()));
        return Err(error);
    }
    // 「没有需要推送的提交」有两种表现：协商结果为空，或协商出的引用两边 oid 相同
    // （实测 libgit2 1.9 仍然会为已一致的引用回调一次，因此必须比较 oid）。
    let moved: Vec<PushUpdateView> = updates.into_iter().filter(PushUpdateView::moves).collect();
    if moved.is_empty() {
        let error = SyncError::NothingToPush;
        control.finish(SyncPhase::Failed, Some(error.to_string()));
        return Err(error);
    }
    // 推送成功后更新本地远端跟踪引用（与 git 一致），状态里的领先 / 落后才是最新的。
    if let Some(oid) = repo
        .find_reference(&format!("refs/heads/{branch}"))
        .ok()
        .and_then(|reference| reference.target())
    {
        let _ = repo.reference(
            &upstream.tracking,
            oid,
            true,
            "flashcast: 推送后更新远端跟踪引用",
        );
    }

    control.finish(SyncPhase::Done, Some("推送完成".to_string()));
    Ok(PushReport {
        branch,
        remote: upstream.remote.clone(),
        message: format!(
            "已推送到 {}/{}（{} 个引用）",
            upstream.remote,
            upstream.branch,
            moved.len()
        ),
        updated: moved,
    })
}

fn open_repo(workspace: &Workspace) -> Result<git2::Repository, SyncError> {
    git::open(workspace).map_err(|error| match error {
        git::GitError::NotARepository(path) => SyncError::NotARepository(path),
        other => SyncError::Git(other.to_string()),
    })
}
