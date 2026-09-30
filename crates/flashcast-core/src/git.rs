//! 配置工作区的 Git 变更查看与显式范围提交（ADR §9）。
//!
//! 设计要点：
//!
//! - **差异基准透明**：[`WorkspaceChanges::diff_base`] 说明差异是相对哪一次提交算的，
//!   每个文件的补丁都是 `HEAD → 工作区`（含已暂存改动的最终结果），
//!   与「勾选某个路径后实际提交的内容」一致。
//! - **提交范围显式**：[`commit`] 只提交调用方显式给出的路径，绝不默认纳入其它改动。
//! - **不吞用户已有的暂存状态**：提交树由一个**临时索引**写出（`HEAD` 树 + 被选中路径的
//!   工作区内容），仓库真实索引里其它路径的暂存改动不会进入提交；只有被选中的路径会
//!   像 `git commit -- <path>` 那样把工作区内容记入索引，且失败时按字节还原索引文件。
//! - **只读探测不加锁**：状态与差异读取使用 `no_refresh`，不写索引；真正要改索引之前
//!   先检查 `index.lock`，被占用时给出明确反馈。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::workspace::Workspace;

/// 单文件差异的最大字符数。超出部分截断并标记，UI 不做无界渲染。
pub const MAX_DIFF_CHARS: usize = 20_000;

/// 变更列表中一个文件的状态与差异。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    /// 仓库相对路径，始终使用 `/` 分隔。
    pub path: String,
    /// `git status --short` 风格的两列代码（索引列 + 工作区列）。
    pub code: String,
    /// 面向用户的中文状态说明。
    pub status_label: String,
    /// 索引里已有改动（已暂存）。不会随本次提交自动纳入，除非该路径同时被选中。
    pub staged: bool,
    /// 工作区里有改动（未暂存）。
    pub unstaged: bool,
    /// 未跟踪的新文件。
    pub untracked: bool,
    /// 存在合并冲突。
    pub conflicted: bool,
    /// 真实补丁文本（`HEAD → 工作区`）。
    pub diff: String,
    /// 差异是否因为过大被截断。
    pub diff_truncated: bool,
}

/// 当前工作区的 Git 变更快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceChanges {
    /// 当前工作区是不是可读取的 Git 仓库。
    pub repository: bool,
    /// 当前分支名（short）。分离 HEAD 时为 `None`。
    pub branch: Option<String>,
    /// HEAD 是否为分离状态。
    pub detached: bool,
    /// 是否存在可提交的内容（含未跟踪文件）。
    pub has_changes: bool,
    /// 差异基准的中文说明，例如 `HEAD (1a2b3c4) → 工作区（含已暂存改动）`。
    pub diff_base: String,
    /// 变更文件列表（按路径排序）。
    pub files: Vec<ChangedFile>,
    /// 工作区处于进行中的操作（合并 / 变基 / 拣选……）时的中文说明。
    pub state: Option<String>,
    /// 读取变更失败时的中文原因。
    pub error: Option<String>,
}

impl WorkspaceChanges {
    /// 尚未关联工作区。
    pub fn unlinked() -> Self {
        Self {
            repository: false,
            branch: None,
            detached: false,
            has_changes: false,
            diff_base: String::new(),
            files: Vec::new(),
            state: None,
            error: None,
        }
    }

    /// 无法读取 Git 状态。
    pub fn failed(error: String) -> Self {
        Self {
            error: Some(error),
            ..Self::unlinked()
        }
    }

    /// 选中路径中可提交的那些（UI 与提交实现共用同一份判断）。
    pub fn paths(&self) -> Vec<String> {
        self.files.iter().map(|file| file.path.clone()).collect()
    }
}

/// Git 操作的失败原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GitError {
    #[error("尚未关联配置工作区，无法执行 Git 操作")]
    NoWorkspace,
    #[error("当前工作区不是 Git 仓库，无法提交：{}", .0.display())]
    NotARepository(PathBuf),
    #[error("提交说明不能为空")]
    EmptyMessage,
    #[error("没有可提交的变更")]
    NothingToCommit,
    #[error("没有选择要提交的文件：提交范围只包含你显式勾选的路径")]
    EmptySelection,
    #[error("所选路径不在当前变更列表中，已拒绝提交：{0}")]
    UnknownPath(String),
    #[error("Git 用户身份未配置，请先设置 user.name 与 user.email")]
    IdentityMissing,
    #[error("{0}")]
    AbnormalState(String),
    #[error("Git 索引被占用（{}），可能有其它 Git 操作正在进行，请稍后重试", .0.display())]
    IndexLocked(PathBuf),
    #[error("Git 操作失败：{0}")]
    Git(String),
}

/// 一次成功提交的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    /// 新提交的完整 oid。
    pub oid: String,
    /// 缩写 oid。
    pub short: String,
    /// 提交说明。
    pub message: String,
    /// 作者名。
    pub author_name: String,
    /// 作者邮箱。
    pub author_email: String,
    /// 本次提交的路径（仓库相对）。
    pub paths: Vec<String>,
    /// 提交后重新读取的变更快照。
    pub changes: WorkspaceChanges,
}

impl CommitOutcome {
    /// 面向用户的中文结果。
    pub fn summary(&self) -> String {
        format!(
            "已创建提交 {}，包含 {} 个文件",
            self.short,
            self.paths.len()
        )
    }
}

/// 读取工作区的 Git 变更（状态 + 逐文件真实差异）。
///
/// 只读，不修改仓库：状态使用 `no_refresh`，不会与 `index.lock` 冲突。
pub fn changes(workspace: &Workspace) -> WorkspaceChanges {
    match open(workspace) {
        Ok(repo) => match collect(&repo) {
            Ok(changes) => changes,
            Err(error) => WorkspaceChanges::failed(error.to_string()),
        },
        Err(error) => WorkspaceChanges::failed(error.to_string()),
    }
}

/// 打开工作区的 Git 仓库。工作区不是仓库时给出明确原因。
pub(crate) fn open(workspace: &Workspace) -> Result<git2::Repository, GitError> {
    match git2::Repository::open(workspace.root()) {
        Ok(repo) => Ok(repo),
        Err(error) if error.code() == git2::ErrorCode::NotFound => {
            Err(GitError::NotARepository(workspace.root().to_path_buf()))
        }
        Err(error) => Err(GitError::Git(error.message().to_string())),
    }
}

/// 读取仓库的变更快照。
fn collect(repo: &git2::Repository) -> Result<WorkspaceChanges, GitError> {
    let detached = repo.head_detached().unwrap_or(false);
    let branch = branch_name(repo, detached);
    let state = in_progress_state(repo);
    let head = repo.head().ok();
    let head_tree = head.as_ref().and_then(|head| head.peel_to_tree().ok());

    let files = statuses(repo, head_tree.as_ref())?;
    let has_changes = !files.is_empty();

    let diff_base = match &head_tree {
        Some(tree) => format!(
            "HEAD ({}) → 工作区（含已暂存改动）",
            &tree.id().to_string()[..7]
        ),
        None => "空仓库（尚无提交）→ 工作区".to_string(),
    };

    Ok(WorkspaceChanges {
        repository: true,
        branch,
        detached,
        has_changes,
        diff_base,
        files,
        state,
        error: None,
    })
}

/// 当前分支名。分离 HEAD 时为 `None`；尚未有提交时从 `HEAD` 的符号引用取名。
fn branch_name(repo: &git2::Repository, detached: bool) -> Option<String> {
    if detached {
        return None;
    }
    match repo.head() {
        Ok(head) => head.shorthand().ok().map(str::to_string),
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => repo
            .find_reference("HEAD")
            .ok()
            .and_then(|reference| {
                reference
                    .symbolic_target()
                    .ok()
                    .flatten()
                    .map(str::to_string)
            })
            .map(|target| {
                target
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&target)
                    .to_string()
            }),
        Err(_) => None,
    }
}

/// 工作区进行中的 Git 操作。用 `Repository::state()` 与 `.git` 标记文件双重复核。
fn in_progress_state(repo: &git2::Repository) -> Option<String> {
    let git_dir = repo.path();
    let marker = |name: &str| git_dir.join(name).exists();

    let operation = if marker("rebase-merge") || marker("rebase-apply") || marker("REBASE_HEAD") {
        Some("变基（rebase）")
    } else if marker("CHERRY_PICK_HEAD") || marker("sequencer") {
        Some("拣选（cherry-pick）/ 序列操作")
    } else if marker("REVERT_HEAD") {
        Some("回退（revert）")
    } else if marker("MERGE_HEAD") {
        Some("合并（merge）")
    } else if marker("BISECT_LOG") {
        Some("二分查找（bisect）")
    } else {
        match repo.state() {
            git2::RepositoryState::Clean => None,
            git2::RepositoryState::Merge => Some("合并（merge）"),
            git2::RepositoryState::Revert | git2::RepositoryState::RevertSequence => {
                Some("回退（revert）")
            }
            git2::RepositoryState::CherryPick | git2::RepositoryState::CherryPickSequence => {
                Some("拣选（cherry-pick）")
            }
            git2::RepositoryState::Rebase
            | git2::RepositoryState::RebaseInteractive
            | git2::RepositoryState::RebaseMerge => Some("变基（rebase）"),
            git2::RepositoryState::Bisect => Some("二分查找（bisect）"),
            git2::RepositoryState::ApplyMailbox
            | git2::RepositoryState::ApplyMailboxOrRebase => Some("应用补丁"),
        }
    }?;

    Some(format!(
        "工作区正在进行{operation}，请先在外部完成或中止它，再创建提交"
    ))
}

/// 状态分类 + 逐文件真实差异。
fn statuses(
    repo: &git2::Repository,
    head_tree: Option<&git2::Tree>,
) -> Result<Vec<ChangedFile>, GitError> {
    let mut options = git2::StatusOptions::new();
    options
        .show(git2::StatusShow::IndexAndWorkdir)
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false)
        .exclude_submodules(true)
        // 只读：不刷新索引的 stat 缓存，因此不写 `.git/index`，也不受 index.lock 影响。
        .no_refresh(true);

    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|error| GitError::Git(error.message().to_string()))?;

    let patches = patches(repo, head_tree)?;
    let mut files = Vec::new();
    for entry in statuses.iter() {
        let status = entry.status();
        if status.is_empty() || status == git2::Status::CURRENT {
            continue;
        }
        let Ok(path) = entry.path() else { continue };
        let path = path.replace('\\', "/");
        let (diff, diff_truncated) = match patches.get(&path) {
            Some(text) => truncate(text.clone()),
            None => (String::new(), false),
        };
        files.push(ChangedFile {
            code: status_code(status),
            status_label: status_label(status),
            staged: is_staged(status),
            unstaged: is_unstaged(status),
            untracked: is_untracked(status),
            conflicted: status.contains(git2::Status::CONFLICTED),
            diff,
            diff_truncated,
            path,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(files)
}

/// 每个文件的真实补丁：`HEAD → 工作区`（与 `git diff HEAD` 一致）。
fn patches(
    repo: &git2::Repository,
    head_tree: Option<&git2::Tree>,
) -> Result<std::collections::BTreeMap<String, String>, GitError> {
    let mut options = git2::DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_typechange(true)
        // 未跟踪文件默认只有 delta 没有内容；要展示真实差异必须显式打开。
        .show_untracked_content(true)
        .show_binary(false);

    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree, Some(&mut options))
        .map_err(|error| GitError::Git(error.message().to_string()))?;

    let mut patches = std::collections::BTreeMap::new();
    for index in 0..diff.deltas().len() {
        let Some(mut patch) = git2::Patch::from_diff(&diff, index)
            .map_err(|error| GitError::Git(error.message().to_string()))?
        else {
            continue;
        };
        let path = patch
            .delta()
            .new_file()
            .path()
            .or_else(|| patch.delta().old_file().path())
            .map(|path| path.to_string_lossy().replace('\\', "/"));
        let Some(path) = path else { continue };
        let text = patch
            .to_buf()
            .map_err(|error| GitError::Git(error.message().to_string()))?;
        let text = text.as_str().unwrap_or_default().to_string();
        let text = if text.is_empty() {
            "（二进制文件或无文本差异）\n".to_string()
        } else {
            text
        };
        patches.insert(path, text);
    }
    Ok(patches)
}

/// `git status --short` 风格的两列代码。
fn status_code(status: git2::Status) -> String {
    if status.contains(git2::Status::CONFLICTED) {
        return "UU".to_string();
    }
    if is_untracked(status) {
        return "??".to_string();
    }
    format!("{}{}", index_letter(status), workdir_letter(status))
}

fn index_letter(status: git2::Status) -> char {
    if status.contains(git2::Status::INDEX_NEW) {
        'A'
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        'M'
    } else if status.contains(git2::Status::INDEX_DELETED) {
        'D'
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        'R'
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        'T'
    } else {
        ' '
    }
}

fn workdir_letter(status: git2::Status) -> char {
    if status.contains(git2::Status::WT_NEW) {
        '?'
    } else if status.contains(git2::Status::WT_MODIFIED) {
        'M'
    } else if status.contains(git2::Status::WT_DELETED) {
        'D'
    } else if status.contains(git2::Status::WT_RENAMED) {
        'R'
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        'T'
    } else {
        ' '
    }
}

fn is_untracked(status: git2::Status) -> bool {
    status.contains(git2::Status::WT_NEW) && !is_staged(status)
}

fn is_staged(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::INDEX_NEW
            | git2::Status::INDEX_MODIFIED
            | git2::Status::INDEX_DELETED
            | git2::Status::INDEX_RENAMED
            | git2::Status::INDEX_TYPECHANGE,
    )
}

fn is_unstaged(status: git2::Status) -> bool {
    status.intersects(
        git2::Status::WT_MODIFIED
            | git2::Status::WT_DELETED
            | git2::Status::WT_RENAMED
            | git2::Status::WT_TYPECHANGE
            | git2::Status::CONFLICTED,
    )
}

/// 中文状态说明。已暂存与未暂存分别点明，避免用户误以为会一起提交。
fn status_label(status: git2::Status) -> String {
    if status.contains(git2::Status::CONFLICTED) {
        return "存在冲突（需在外部解决）".to_string();
    }
    let mut parts = Vec::new();
    if is_staged(status) {
        parts.push(match index_letter(status) {
            'A' => "已暂存新增",
            'M' => "已暂存修改",
            'D' => "已暂存删除",
            'R' => "已暂存重命名",
            'T' => "已暂存类型变更",
            _ => "已暂存",
        });
    }
    if is_untracked(status) {
        parts.push("未跟踪（新文件）");
    } else if is_unstaged(status) {
        parts.push(match workdir_letter(status) {
            'M' => "未暂存修改",
            'D' => "未暂存删除",
            'R' => "未暂存重命名",
            'T' => "未暂存类型变更",
            _ => "未暂存改动",
        });
    }
    if parts.is_empty() {
        parts.push("无改动");
    }
    parts.join(" + ")
}

/// 差异过长时按字符截断并标记。
fn truncate(text: String) -> (String, bool) {
    if text.chars().count() <= MAX_DIFF_CHARS {
        return (text, false);
    }
    let mut truncated: String = text.chars().take(MAX_DIFF_CHARS).collect();
    truncated.push_str("\n…（差异过大，已截断）\n");
    (truncated, true)
}
