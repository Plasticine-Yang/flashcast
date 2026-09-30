//! ticket 16：配置工作区的远端同步（状态 / 仅快进拉取 / 显式推送 / 阻塞与保护）。
//!
//! 全部经由宿主入口（`sync_status` / `pull_workspace` / `push_workspace` /
//! `redetect_sync_state` / `commit_workspace` / `update_settings`），在**真实临时
//! 仓库**与**真实本地 bare 远端**上断言真实仓库状态。
//!
//! 本地 bare 远端证明的是「走通了 git2 的 fetch / push 代码路径」；它不经过网络，
//! 因此鉴权与离线两类失败另外用「只回 401 的本地 HTTP 桩」与「不可达端口」在外部
//! 边界上构造（spec 允许：只有鉴权与网络失败才在相应外部边界控制）。

mod support;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use flashcast_core::{
    Host, PullResult, Settings, SyncControl, SyncError, SyncPhase, SETTINGS_FILE,
};
use support::{
    bare_remote, bare_remote_commit, bare_remote_file, bare_remote_oid, cleanup, fast_settings,
    git_index_bytes, git_put_marker, git_remote_add, git_remove_dir, git_remove_marker,
    git_repo_with_commit, host_with_device, remote_url, unique_dir, workspace_files,
};

/// 一段完整合法的 `settings.toml`（`deny_unknown_fields` 要求所有键都在）。
fn settings_toml(hotkey: &str) -> String {
    Settings {
        hotkey: hotkey.to_string(),
        ..Settings::default()
    }
    .to_toml()
    .expect("序列化设置")
}

const HOTKEY_INITIAL: &str = "Ctrl+Alt+Space";
const HOTKEY_REMOTE: &str = "Alt+Space";
const HOTKEY_LOCAL: &str = "Ctrl+Shift+Space";

fn files() -> Vec<(&'static str, String)> {
    workspace_files(HOTKEY_INITIAL)
}

/// 已从 bare 远端克隆好的工作区。
struct Fixture {
    host: Host,
    remote: PathBuf,
    workspace: PathBuf,
    device: PathBuf,
}

impl Fixture {
    fn new(prefix: &str) -> Self {
        let remote = bare_remote(prefix, &files());
        let workspace = unique_dir(&format!("{prefix}-ws"));
        let (host, _launcher, device) = host_with_device(Vec::new(), fast_settings());
        host.clone_workspace(&remote_url(&remote), &workspace)
            .unwrap_or_else(|error| panic!("克隆本地 bare 远端应成功：{error}"));
        Self {
            host,
            remote,
            workspace,
            device,
        }
    }

    /// 克隆出来的仓库没有提交身份（工作区的 `.git/config` 不含 user.*），
    /// 而 CI 上全局配置被清空，因此需要提交的测试必须自己写仓库本地身份。
    fn with_identity(&self) {
        support::set_identity(
            &self.workspace,
            support::TEST_AUTHOR_NAME,
            support::TEST_AUTHOR_EMAIL,
        );
    }

    fn read(&self, rel: &str) -> Vec<u8> {
        std::fs::read(self.workspace.join(rel)).expect("读取工作区文件")
    }

    fn head_oid(&self) -> git2::Oid {
        support::git_head_oid(&self.workspace)
    }

    fn index_bytes(&self) -> Vec<u8> {
        git_index_bytes(&self.workspace)
    }

    fn remote_oid(&self) -> Option<git2::Oid> {
        bare_remote_oid(&self.remote, "main")
    }

    fn cleanup(&self) {
        cleanup(&self.remote);
        cleanup(&self.workspace);
        cleanup(&self.device);
    }
}

// ---------------------------------------------------------------------------
// 状态
// ---------------------------------------------------------------------------

#[test]
fn status_reports_branch_remote_counts_and_sync_possible() {
    let fixture = Fixture::new("sync-status");
    let status = fixture.host.sync_status();

    assert!(status.repository, "克隆出来的工作区应当是 Git 仓库");
    assert_eq!(status.branch.as_deref(), Some("main"));
    assert!(!status.detached);
    assert_eq!(status.remote_name.as_deref(), Some("origin"));
    assert_eq!(
        status.remote_url.as_deref(),
        Some(remote_url(&fixture.remote).as_str())
    );
    assert_eq!(status.upstream.as_deref(), Some("origin/main"));
    assert_eq!(
        (status.ahead, status.behind),
        (0, 0),
        "刚克隆应不领先不落后"
    );
    assert!(status.tracking, "克隆会写好远端跟踪引用");
    assert!(!status.dirty, "克隆出来的工作区是干净的");
    assert!(status.state.is_none(), "没有进行中的 Git 操作");
    assert!(status.can_pull && status.can_push);
    assert!(status.nothing_to_push, "没有本地提交时无需推送");
    assert!(status.blocking.is_none() && status.push_blocking.is_none());
    assert!(status.sync_possible());
    assert!(!status.busy);
    assert!(status.error.is_none());

    // 状态查询是只读的：索引字节不变。
    let before = fixture.index_bytes();
    let _ = fixture.host.sync_status();
    assert_eq!(before, fixture.index_bytes());

    fixture.cleanup();
}

#[test]
fn status_without_workspace_is_a_named_block() {
    let (host, _launcher, device) = host_with_device(Vec::new(), fast_settings());
    let status = host.sync_status();
    assert!(!status.repository);
    assert!(!status.sync_possible());
    assert_eq!(
        status.blocking.as_ref().map(|block| block.code.as_str()),
        Some("noWorkspace")
    );
    assert!(status
        .blocking
        .as_ref()
        .map(|block| block.hint.contains("工作区"))
        .unwrap_or(false));
    cleanup(&device);
}

// ---------------------------------------------------------------------------
// 推送
// ---------------------------------------------------------------------------

#[test]
fn push_moves_the_bare_remote_branch() {
    let fixture = Fixture::new("sync-push");
    fixture.with_identity();

    let updated = Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        ..fast_settings()
    };
    fixture
        .host
        .update_settings(updated.clone())
        .expect("写入设置");
    let changes = fixture.host.workspace_changes();
    assert!(changes.has_changes, "改设置后应有未提交变更");
    let outcome = fixture
        .host
        .commit_workspace("同步测试：改快捷键", &[SETTINGS_FILE.to_string()])
        .expect("提交应成功");
    let local_head = outcome.oid.clone();

    let pushed = fixture.host.push_workspace().expect("推送应成功");
    assert_eq!(pushed.branch, "main");
    assert_eq!(pushed.remote, "origin");
    assert_eq!(pushed.updated.len(), 1, "应当只推送一个引用");
    assert_eq!(pushed.updated[0].local, "refs/heads/main");
    assert_eq!(pushed.updated[0].remote, "refs/heads/main");
    assert!(pushed.message.contains("origin/main"), "{}", pushed.message);

    // 真远端的分支确实移动了，且内容就是本地提交的内容。
    assert_eq!(
        fixture.remote_oid().map(|oid| oid.to_string()).as_deref(),
        Some(local_head.as_str()),
        "bare 远端的 main 必须指向本地新提交"
    );
    let remote_settings =
        bare_remote_file(&fixture.remote, "main", SETTINGS_FILE).expect("远端应有设置文件");
    assert!(remote_settings.contains("Ctrl+Alt+K"), "{remote_settings}");

    // 推送后状态重建：不再领先。
    assert_eq!(pushed.status.ahead, 0);
    assert_eq!(pushed.status.behind, 0);
    assert!(pushed.status.nothing_to_push);

    fixture.cleanup();
}

#[test]
fn push_without_new_commits_reports_nothing_to_push() {
    let fixture = Fixture::new("sync-push-nothing");
    let error = fixture.host.push_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "nothingToPush");
    assert!(matches!(error, SyncError::NothingToPush));
    assert!(error.hint().contains("无需推送"));
    // 远端没有被改动。
    assert!(fixture.remote_oid().is_some());
    fixture.cleanup();
}

#[test]
fn push_without_upstream_is_a_distinct_state() {
    let remote = bare_remote("sync-no-upstream", &files());
    let repo = git_repo_with_commit(
        "sync-no-upstream-repo",
        &[(SETTINGS_FILE, settings_toml(HOTKEY_INITIAL).as_str())],
    );
    git_remote_add(&repo, "origin", &remote_url(&remote));
    // 故意不写 branch.main.remote / branch.main.merge：没有上游关系。

    let (host, _launcher, device) = host_with_device(Vec::new(), fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let status = host.sync_status();
    assert_eq!(status.remote_name.as_deref(), Some("origin"));
    assert!(status.upstream.is_none(), "没有上游关系时 upstream 为空");
    assert!(!status.can_push && !status.can_pull);
    assert_eq!(
        status
            .push_blocking
            .as_ref()
            .map(|block| block.code.as_str()),
        Some("noUpstream")
    );
    assert!(status
        .push_blocking
        .as_ref()
        .map(|block| block.hint.contains("push -u"))
        .unwrap_or(false));

    let error = host.push_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "noUpstream");
    assert!(matches!(error, SyncError::NoUpstream { .. }));
    assert!(error.hint().contains("上游"), "{}", error.hint());

    // 远端没有被改动：仍然只有一条初始提交。
    assert!(bare_remote_oid(&remote, "main").is_some());
    assert_eq!(
        bare_remote_file(&remote, "main", SETTINGS_FILE).as_deref(),
        Some(settings_toml(HOTKEY_INITIAL).as_str())
    );

    cleanup(&remote);
    cleanup(&repo);
    cleanup(&device);
}

// ---------------------------------------------------------------------------
// 快进拉取
// ---------------------------------------------------------------------------

#[test]
fn fast_forward_pull_applies_commit_and_reloads_settings_theme_and_memos() {
    let fixture = Fixture::new("sync-pull-ff");
    let before = fixture.head_oid();

    let remote_settings = settings_toml(HOTKEY_REMOTE);
    let target = bare_remote_commit(
        &fixture.remote,
        &[
            (SETTINGS_FILE, remote_settings.as_str()),
            ("memos/remote-note.md", "# 远端笔记\n\n来自另一台设备。\n"),
        ],
        "远端更新设置与备忘录",
    );

    let outcome = fixture.host.pull_workspace().expect("快进拉取应成功");
    assert_eq!(outcome.result.commits(), 1);
    assert_eq!(outcome.result.to(), Some(target.to_string().as_str()));
    assert_eq!(outcome.result.from(), Some(before.to_string().as_str()));
    assert!(outcome.message.contains("快进"), "{}", outcome.message);

    // 工作区内容真的更新了。
    assert_eq!(
        std::fs::read_to_string(fixture.workspace.join(SETTINGS_FILE)).expect("读设置"),
        remote_settings
    );
    assert!(fixture.workspace.join("memos/remote-note.md").exists());
    assert_eq!(fixture.head_oid(), target, "本地分支应指向远端提交");

    // 生效设置重新加载：宿主看到的是拉取进来的快捷键。
    assert_eq!(fixture.host.settings().hotkey, HOTKEY_REMOTE);
    assert!(outcome.reload.applied, "设置内容变了，重载应当生效");
    assert!(outcome.reload.error.is_none());

    // 主题与备忘录重新读取。
    assert_eq!(outcome.theme.as_deref(), Some("dark"));
    assert_eq!(
        outcome.memos,
        vec![
            "memos/hello.md".to_string(),
            "memos/remote-note.md".to_string()
        ]
    );

    // 拉取后状态：不领先不落后、干净。
    assert_eq!((outcome.status.ahead, outcome.status.behind), (0, 0));
    assert!(!outcome.status.dirty);
    assert!(outcome.status.nothing_to_push);
    assert!(!outcome.status.busy);

    fixture.cleanup();
}

#[test]
fn pull_when_up_to_date_changes_nothing() {
    let fixture = Fixture::new("sync-pull-uptodate");
    let head = fixture.head_oid();
    let index = fixture.index_bytes();
    let settings = fixture.read(SETTINGS_FILE);

    let outcome = fixture.host.pull_workspace().expect("拉取应成功");
    assert!(matches!(outcome.result, PullResult::UpToDate));
    assert!(outcome.message.contains("最新"), "{}", outcome.message);
    assert_eq!(fixture.head_oid(), head);
    assert_eq!(fixture.index_bytes(), index, "索引必须逐字节不变");
    assert_eq!(fixture.read(SETTINGS_FILE), settings);
    assert!(!outcome.reload.applied, "没有变化就不应当重新应用设置");

    fixture.cleanup();
}

/// 上游指向一个远端还不存在的分支：拉取要单独分类，指引是「先推送一次」。
#[test]
fn pull_with_a_missing_remote_branch_is_distinct() {
    let fixture = Fixture::new("sync-pull-missing");
    support::git_set_upstream(&fixture.workspace, "main", "origin", "feature");

    let workspace = fixture.workspace.clone();
    let error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "remoteBranchMissing", "{error:?}");
    match &error {
        SyncError::RemoteBranchMissing(branch) => assert_eq!(branch, "feature"),
        other => panic!("应当是远端缺少分支：{other:?}"),
    }
    assert!(error.hint().contains("推送"), "{}", error.hint());
    assert_eq!(fixture.head_oid(), support::git_head_oid(&workspace));

    fixture.cleanup();
}

#[test]
fn diverged_pull_refuses_and_changes_nothing_on_either_side() {
    let fixture = Fixture::new("sync-pull-diverged");
    fixture.with_identity();

    // 本地提交。
    fixture
        .host
        .update_settings(Settings {
            hotkey: HOTKEY_LOCAL.to_string(),
            ..fast_settings()
        })
        .expect("写入设置");
    let local = fixture
        .host
        .commit_workspace("本地改动", &[SETTINGS_FILE.to_string()])
        .expect("本地提交");

    // 远端另一条提交。
    let remote_settings = settings_toml("Ctrl+Alt+J");
    let remote = bare_remote_commit(
        &fixture.remote,
        &[(SETTINGS_FILE, remote_settings.as_str())],
        "远端另一条提交",
    );

    let head = fixture.head_oid();
    let index = fixture.index_bytes();
    let worktree = fixture.read(SETTINGS_FILE);

    let error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "diverged");
    match &error {
        SyncError::Diverged { ahead, behind } => {
            assert_eq!((*ahead, *behind), (1, 1), "两边各有一条对方没有的提交");
        }
        other => panic!("应当是分叉：{other:?}"),
    }
    assert!(error.hint().contains("三方合并"), "{}", error.hint());
    assert!(error.hint().contains("不强推"), "{}", error.hint());

    // 两边都没有被改动。
    assert_eq!(fixture.head_oid(), head, "本地分支不得移动");
    assert_eq!(fixture.index_bytes(), index, "索引必须逐字节不变");
    assert_eq!(
        fixture.read(SETTINGS_FILE),
        worktree,
        "工作区内容必须原样保留"
    );
    assert_eq!(fixture.remote_oid(), Some(remote), "远端不得被改动");
    assert!(
        !std::fs::read_to_string(fixture.workspace.join(SETTINGS_FILE))
            .unwrap()
            .contains("Ctrl+Alt+J"),
        "不能把远端内容写进本地工作区"
    );
    let _ = local;

    // 状态里如实呈现分叉，并给出重新检测入口。
    let status = fixture.host.sync_status();
    assert!(!status.can_pull);
    assert_eq!(
        status.blocking.as_ref().map(|block| block.code.as_str()),
        Some("diverged")
    );
    assert_eq!((status.ahead, status.behind), (1, 1));

    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 内容与暂存状态保护
// ---------------------------------------------------------------------------

#[test]
fn dirty_worktree_blocks_pull_and_preserves_content_and_index_byte_for_byte() {
    let fixture = Fixture::new("sync-pull-dirty");

    // 远端已经有一条新提交，拉取本该成功——但工作区脏，必须被拦下。
    bare_remote_commit(
        &fixture.remote,
        &[("memos/remote.md", "# 远端\n")],
        "远端新提交",
    );

    // 构造三种脏状态：已暂存改动、未暂存改动、未跟踪文件。
    support::git_write(
        &fixture.workspace,
        "memos/hello.md",
        "# 你好\n\n本地改过的内容。\n",
    );
    support::git_stage(&fixture.workspace, "memos/hello.md");
    support::git_write(
        &fixture.workspace,
        SETTINGS_FILE,
        &settings_toml("Ctrl+Alt+H"),
    );
    support::git_write(&fixture.workspace, "memos/local-draft.md", "# 草稿\n");

    let head = fixture.head_oid();
    let index = fixture.index_bytes();
    let staged_blob = support::git_index_blob(&fixture.workspace, "memos/hello.md");
    let settings = fixture.read(SETTINGS_FILE);
    let memo = fixture.read("memos/hello.md");
    let remote = fixture.remote_oid();

    let error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "dirtyWorktree");
    match &error {
        SyncError::DirtyWorktree(detail) => {
            assert!(
                detail.staged && detail.unstaged && detail.untracked,
                "{detail:?}"
            );
        }
        other => panic!("应当是未提交修改：{other:?}"),
    }
    assert!(error.hint().contains("提交"), "{}", error.hint());

    // 内容、索引与远端逐字节不变。
    assert_eq!(fixture.head_oid(), head);
    assert_eq!(fixture.index_bytes(), index, "已有暂存状态必须逐字节保留");
    assert_eq!(
        support::git_index_blob(&fixture.workspace, "memos/hello.md"),
        staged_blob
    );
    assert_eq!(fixture.read(SETTINGS_FILE), settings);
    assert_eq!(fixture.read("memos/hello.md"), memo);
    assert_eq!(fixture.remote_oid(), remote);
    assert!(
        !fixture.workspace.join("memos/remote.md").exists(),
        "被阻塞时不得写入远端文件"
    );

    // 状态如实呈现：拉取不可用，但推送不受未提交修改阻塞。
    let status = fixture.host.sync_status();
    assert!(!status.can_pull);
    assert!(
        status.can_push,
        "推送只搬运已提交对象，不应被未提交修改阻塞"
    );
    assert!(status.dirty && status.staged && status.unstaged && status.untracked);
    assert_eq!(
        status.blocking.as_ref().map(|block| block.code.as_str()),
        Some("dirtyWorktree")
    );
    assert!(status.push_blocking.is_none());

    fixture.cleanup();
}

#[test]
fn in_progress_operation_blocks_pull_and_push() {
    let fixture = Fixture::new("sync-in-progress");
    let head = fixture.head_oid();
    let index = fixture.index_bytes();
    git_put_marker(&fixture.workspace, "MERGE_HEAD", &head.to_string());

    let status = fixture.host.sync_status();
    assert!(status.state.is_some());
    assert!(
        status.state.as_deref().unwrap_or_default().contains("合并"),
        "{:?}",
        status.state
    );
    assert!(!status.can_pull && !status.can_push);
    assert_eq!(
        status.blocking.as_ref().map(|block| block.code.as_str()),
        Some("operationInProgress")
    );
    assert_eq!(
        status
            .push_blocking
            .as_ref()
            .map(|block| block.code.as_str()),
        Some("operationInProgress")
    );

    let pull_error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(pull_error.kind().code(), "operationInProgress");
    let push_error = fixture.host.push_workspace().unwrap_err();
    assert_eq!(push_error.kind().code(), "operationInProgress");
    assert!(
        pull_error.hint().contains("merge --abort"),
        "{}",
        pull_error.hint()
    );

    assert_eq!(fixture.head_oid(), head);
    assert_eq!(fixture.index_bytes(), index);

    // 重新检测：外部去掉标记后立刻恢复可用。
    git_remove_marker(&fixture.workspace, "MERGE_HEAD");
    let redetected = fixture.host.redetect_sync_state();
    assert!(redetected.state.is_none());
    assert!(redetected.can_pull && redetected.can_push);

    fixture.cleanup();
}

#[test]
fn conflicts_in_the_index_are_a_distinct_block() {
    let fixture = Fixture::new("sync-conflicts");

    // 本地与远端改同一行 → 真实合并冲突。
    support::git_write(
        &fixture.workspace,
        "memos/hello.md",
        "# 你好\n\n本地版本。\n",
    );
    support::git_commit_all(&fixture.workspace, "本地改备忘录");
    let remote = bare_remote_commit(
        &fixture.remote,
        &[("memos/hello.md", "# 你好\n\n远端版本。\n")],
        "远端改备忘录",
    );

    let repo = git2::Repository::open(&fixture.workspace).expect("打开工作区");
    repo.find_remote("origin")
        .expect("远端")
        .fetch(&["+refs/heads/main:refs/remotes/origin/main"], None, None)
        .expect("fetch");
    support::git_merge_other(&fixture.workspace, remote);
    // 制造「索引里有冲突但没有进行中标记」：删掉 MERGE_HEAD。
    git_remove_marker(&fixture.workspace, "MERGE_HEAD");

    let index = fixture.index_bytes();
    let status = fixture.host.sync_status();
    assert!(status.conflicted, "索引里应当有冲突");
    assert!(status.state.is_none(), "标记已删除，不应当报告进行中操作");
    assert!(!status.can_pull && !status.can_push);
    assert_eq!(
        status.blocking.as_ref().map(|block| block.code.as_str()),
        Some("conflicts")
    );

    let error = fixture.host.pull_workspace().unwrap_err();
    assert!(matches!(error, SyncError::Conflicts(_)), "{error:?}");
    assert_eq!(error.kind().code(), "conflicts");
    assert!(error.hint().contains("git add"), "{}", error.hint());
    assert_eq!(fixture.index_bytes(), index, "冲突索引不得被改动");

    // 外部解决冲突后重新检测：冲突消失（剩下的分叉是真实的，另行处理）。
    support::git_resolve_all_conflicts(&fixture.workspace);
    let resolved = fixture.host.redetect_sync_state();
    assert!(!resolved.conflicted, "{:?}", resolved.blocking);
    assert!(!resolved.dirty, "{:?}", resolved.blocking);
    assert_ne!(
        resolved.blocking.as_ref().map(|block| block.code.as_str()),
        Some("conflicts")
    );
    assert_eq!(
        resolved.blocking.as_ref().map(|block| block.code.as_str()),
        Some("diverged"),
        "本地已提交的改动与远端提交确实分叉"
    );

    fixture.cleanup();
}

#[test]
fn cancelled_pull_changes_nothing() {
    let fixture = Fixture::new("sync-cancel");
    bare_remote_commit(
        &fixture.remote,
        &[("memos/remote.md", "# 远端\n")],
        "远端新提交",
    );

    let head = fixture.head_oid();
    let index = fixture.index_bytes();
    let remote = fixture.remote_oid();

    let control = SyncControl::new();
    control.cancel();
    let error = fixture
        .host
        .pull_workspace_with_control(&control)
        .unwrap_err();
    assert!(matches!(error, SyncError::Cancelled), "{error:?}");
    assert_eq!(fixture.head_oid(), head);
    assert_eq!(fixture.index_bytes(), index);
    assert_eq!(fixture.remote_oid(), remote);
    assert!(!fixture.workspace.join("memos/remote.md").exists());
    assert_eq!(fixture.host.sync_progress().phase, SyncPhase::Cancelled);

    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 鉴权 / 离线：在外部边界上构造，且不影响本地可用性
// ---------------------------------------------------------------------------

/// 只回 401 的最小 HTTP 服务：在「外部边界」上构造真实的鉴权失败，
/// 不使用任何个人凭证。
struct Unauthorized {
    port: u16,
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Unauthorized {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地端口");
        let port = listener.local_addr().expect("本地地址").port();
        listener.set_nonblocking(true).expect("非阻塞监听");
        let stop = Arc::new(AtomicBool::new(false));
        let handle = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    match listener.accept() {
                        Ok((mut stream, _)) => {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                            let mut buffer = [0_u8; 4096];
                            let _ = stream.read(&mut buffer);
                            let body = "unauthorized";
                            let response = format!(
                                "HTTP/1.1 401 Unauthorized\r\n\
                                 WWW-Authenticate: Basic realm=\"flashcast-test\"\r\n\
                                 Content-Type: text/plain\r\n\
                                 Content-Length: {}\r\n\
                                 Connection: close\r\n\r\n{body}",
                                body.len()
                            );
                            let _ = stream.write_all(response.as_bytes());
                            let _ = stream.flush();
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(_) => break,
                    }
                }
            })
        };
        Self {
            port,
            stop,
            handle: Some(handle),
        }
    }

    fn url(&self) -> String {
        format!("http://127.0.0.1:{}/repo.git", self.port)
    }
}

impl Drop for Unauthorized {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

/// 一个已经关闭的本地端口：连接必然被拒，用来构造离线 / 网络失败。
fn closed_port_url() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("绑定本地端口");
    let port = listener.local_addr().expect("本地地址").port();
    drop(listener);
    format!("http://127.0.0.1:{port}/repo.git")
}

const TEST_TOKEN: &str = "ghp_flashcastTicket16SecretToken0123456789";

#[test]
fn auth_failure_is_distinct_and_local_features_keep_working() {
    let fixture = Fixture::new("sync-auth");
    fixture.with_identity();
    let server = Unauthorized::start();
    support::git_set_remote_url(&fixture.workspace, "origin", &server.url());
    fixture
        .host
        .remember_git_token(&server.url(), "x-access-token", TEST_TOKEN)
        .expect("保存设备本地令牌");

    let error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "authFailed", "{error:?}");
    assert!(error.hint().contains("令牌"), "{}", error.hint());

    // 令牌绝不出现在面向用户的错误文本里。
    assert!(!error.to_string().contains(TEST_TOKEN), "{error}");
    assert!(!error.hint().contains(TEST_TOKEN));
    assert!(!error.detail().contains(TEST_TOKEN));

    // 令牌只在设备本地目录里，工作区任何文件都不含它。
    let credentials = std::fs::read_to_string(fixture.device.join("git-credentials.json"))
        .expect("令牌文件应当存在");
    assert!(credentials.contains(TEST_TOKEN), "令牌应当保存在设备本地");
    for path in support::files_under(&fixture.workspace) {
        let bytes = std::fs::read(&path).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&bytes).contains(TEST_TOKEN),
            "工作区文件中不得出现令牌：{}",
            path.display()
        );
    }

    // 离线 / 鉴权失败不降级应用：本地搜索、设置与本地提交继续工作。
    assert_local_features_usable(&fixture);
    fixture.cleanup();
}

#[test]
fn offline_is_distinct_from_auth_failure() {
    let fixture = Fixture::new("sync-offline");
    support::git_set_remote_url(&fixture.workspace, "origin", &closed_port_url());

    let error = fixture.host.pull_workspace().unwrap_err();
    assert_eq!(error.kind().code(), "offline", "{error:?}");
    assert!(!error.to_string().contains(TEST_TOKEN));
    assert!(error.hint().contains("网络"), "{}", error.hint());
    assert_ne!(error.kind().code(), "authFailed");
    match error {
        SyncError::Offline(_) => {}
        other => panic!("应当分类为离线：{other:?}"),
    }

    // 状态查询本身不碰网络，因此仍然可读；推送同样是离线分类。
    let status = fixture.host.sync_status();
    assert!(status.repository);
    let push_error = fixture.host.push_workspace().unwrap_err();
    assert_eq!(push_error.kind().code(), "offline", "{push_error:?}");

    assert_local_features_usable(&fixture);
    fixture.cleanup();
}

/// 网络不可用时宿主核心功能仍然可用：搜索、设置、本地变更与本地提交。
fn assert_local_features_usable(fixture: &Fixture) {
    let response = fixture.host.query("");
    assert!(
        !response.items.is_empty(),
        "离线时首屏快速访问项仍应可用：{:?}",
        response.notice
    );
    fixture
        .host
        .update_settings(Settings {
            quick_access_limit: 7,
            ..fast_settings()
        })
        .expect("离线时仍应能写设置");
    assert_eq!(fixture.host.settings().quick_access_limit, 7);
    let changes = fixture.host.workspace_changes();
    assert!(changes.repository, "离线时仍应能读取本地变更");
    assert!(changes.has_changes, "改过的设置应当出现在变更列表里");
    assert!(changes.error.is_none(), "{:?}", changes.error);

    fixture.with_identity();
    let committed = fixture
        .host
        .commit_workspace("离线本地提交", &[SETTINGS_FILE.to_string()])
        .expect("离线时仍应能创建本地提交");
    assert!(!committed.oid.is_empty());
}

// ---------------------------------------------------------------------------
// 重新检测
// ---------------------------------------------------------------------------

#[test]
fn redetect_clears_state_after_the_user_fixes_it_outside() {
    let fixture = Fixture::new("sync-redetect");
    fixture.with_identity();

    // 1) 未提交修改阻塞拉取。
    support::git_write(&fixture.workspace, "memos/draft.md", "# 草稿\n");
    let blocked = fixture.host.sync_status();
    assert!(!blocked.can_pull);
    assert_eq!(
        blocked.blocking.as_ref().map(|block| block.code.as_str()),
        Some("dirtyWorktree")
    );

    // 外部解决：提交这个文件。
    support::git_stage(&fixture.workspace, "memos/draft.md");
    support::git_commit_all(&fixture.workspace, "外部提交草稿");
    let fixed = fixture.host.redetect_sync_state();
    assert!(fixed.can_pull, "{:?}", fixed.blocking);
    assert!(!fixed.dirty);

    // 2) 进行中的操作阻塞；外部中止后重新检测恢复。
    git_put_marker(
        &fixture.workspace,
        "rebase-merge/head-name",
        "refs/heads/main",
    );
    let blocked = fixture.host.redetect_sync_state();
    assert_eq!(
        blocked.blocking.as_ref().map(|block| block.code.as_str()),
        Some("operationInProgress")
    );
    let blocked_block = blocked.blocking.as_ref().expect("应当有阻塞原因");
    assert!(blocked_block.detail.contains("变基"), "{:?}", blocked_block);
    assert_eq!(blocked_block.label, "Git 操作进行中");
    assert!(
        blocked_block.hint.contains("rebase --abort"),
        "{:?}",
        blocked_block
    );
    git_remove_dir(&fixture.workspace, "rebase-merge");
    let fixed = fixture.host.redetect_sync_state();
    assert!(fixed.can_pull && fixed.can_push, "{:?}", fixed.blocking);

    fixture.cleanup();
}
