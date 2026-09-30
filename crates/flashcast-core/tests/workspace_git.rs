//! 配置工作区的 Git 变更查看与显式范围提交（ticket 15）集成测试。
//!
//! 全部经由 `Host` 的入口在**真实临时 Git 仓库**上验证，并断言真实仓库状态：
//! 提交对象、树内容、父提交、作者身份、索引字节。不断言 Git 命令字符串（ADR §3 / §10）。

mod support;

use std::collections::BTreeMap;

use flashcast_core::{ChangedFile, SETTINGS_FILE};
use support::{
    cleanup, fast_settings, git_repo_with_commit, git_stage, git_write, host_with_device,
};

/// 公共夹具：一个带初始提交的真实仓库。
///
/// ```text
/// settings.toml          HEAD: v1          → 工作区改成 v2（未暂存）
/// theme.json             HEAD: v1 → 暂存 v2 → 工作区再改成 v3（部分暂存 + 未暂存）
/// memos/2026-10-01.md    未跟踪的新文件
/// other.txt              HEAD 内容不变，作为「无关文件」对照
/// ```
fn changes_fixture(prefix: &str) -> std::path::PathBuf {
    let repo = git_repo_with_commit(
        prefix,
        &[
            (SETTINGS_FILE, "hotkey = \"Ctrl+Alt+Space\"\n"),
            ("theme.json", "{\"theme\":\"dark\"}\n"),
            ("other.txt", "无关文件\n"),
        ],
    );
    git_write(&repo, SETTINGS_FILE, "hotkey = \"Super+Space\"\n");
    git_write(&repo, "theme.json", "{\"theme\":\"dark\",\"contrast\":true}\n");
    git_stage(&repo, "theme.json");
    git_write(&repo, "theme.json", "{\"theme\":\"dark\",\"font\":\"serif\"}\n");
    git_write(&repo, "memos/2026-10-01.md", "备忘录内容：今天做的事\n");
    repo
}

fn by_path(changes: &flashcast_core::WorkspaceChanges) -> BTreeMap<String, ChangedFile> {
    changes
        .files
        .iter()
        .map(|file| (file.path.clone(), file.clone()))
        .collect()
}

/// 设置页需要看到：分支、是否有可提交内容、每个文件的状态分类与真实差异内容。
#[test]
fn workspace_changes_expose_branch_status_and_real_diffs() {
    let repo = changes_fixture("git-changes");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区必须成功");

    let changes = host.workspace_changes();
    assert!(changes.repository, "必须识别出 Git 仓库：{changes:?}");
    assert_eq!(changes.branch.as_deref(), Some("main"), "必须显示当前分支");
    assert!(!changes.detached, "普通仓库不是分离 HEAD");
    assert!(changes.has_changes, "有未提交改动：{changes:?}");
    assert!(changes.error.is_none(), "读取失败：{:?}", changes.error);
    assert!(changes.state.is_none(), "干净仓库不应有异常状态");
    assert!(
        changes.diff_base.contains("HEAD"),
        "差异基准必须透明可见：{}",
        changes.diff_base
    );

    let files = by_path(&changes);
    assert!(
        !files.contains_key("other.txt"),
        "没有改动的文件不应出现在变更列表中：{:?}",
        files.keys().collect::<Vec<_>>()
    );

    // 未暂存的修改。
    let settings = &files[SETTINGS_FILE];
    assert!(!settings.staged && settings.unstaged && !settings.untracked);
    assert_eq!(settings.code, " M", "git status --short 风格代码");
    assert!(
        settings.status_label.contains("未暂存"),
        "状态标签必须点明未暂存：{}",
        settings.status_label
    );

    // 已有暂存改动：必须被单独标记出来（不会随本次提交自动纳入）。
    let theme = &files["theme.json"];
    assert!(theme.staged, "已暂存改动必须被识别：{theme:?}");
    assert!(theme.unstaged, "暂存之后工作区又改了：{theme:?}");
    assert_eq!(theme.code, "MM");

    // 未跟踪的新文件。
    let memo = &files["memos/2026-10-01.md"];
    assert!(memo.untracked && !memo.staged && !memo.unstaged);
    assert_eq!(memo.code, "??");
    assert!(
        memo.status_label.contains("未跟踪"),
        "状态标签：{}",
        memo.status_label
    );

    // 真实差异内容（不是命令字符串）：HEAD → 工作区。
    assert!(
        settings.diff.contains("--- a/settings.toml") && settings.diff.contains("+++ b/settings.toml"),
        "差异必须是真实的补丁：\n{}",
        settings.diff
    );
    assert!(
        settings.diff.contains("+hotkey = \"Super+Space\""),
        "差异必须包含新内容：\n{}",
        settings.diff
    );
    assert!(
        settings.diff.contains("-hotkey = \"Ctrl+Alt+Space\""),
        "差异必须包含旧内容：\n{}",
        settings.diff
    );
    assert!(
        memo.diff.contains("+备忘录内容：今天做的事"),
        "未跟踪文件的差异必须展示全部新增内容：\n{}",
        memo.diff
    );
    assert!(
        theme.diff.contains("+{\"theme\":\"dark\",\"font\":\"serif\"}")
            && !theme.diff.contains("contrast"),
        "差异必须是 HEAD → 工作区，而不是 HEAD → 索引（暂存的中间内容不得出现）：\n{}",
        theme.diff
    );
    assert!(!settings.diff_truncated);

    cleanup(&repo);
    cleanup(&device);
}

/// 尚未关联工作区 / 不是 Git 仓库时，变更视图必须给出明确状态而不是 panic。
#[test]
fn workspace_changes_report_unlinked_and_non_repository_workspaces() {
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let unlinked = host.workspace_changes();
    assert!(!unlinked.repository, "未关联时不是仓库：{unlinked:?}");
    assert!(unlinked.files.is_empty());
    assert!(!unlinked.has_changes);

    // 普通目录（不是 Git 仓库）作为工作区。
    let plain = support::unique_dir("git-plain-dir");
    std::fs::write(plain.join(SETTINGS_FILE), "hotkey = \"Ctrl+Alt+Space\"\n")
        .expect("写入设置文件");
    host.select_workspace(&plain).expect("普通目录也可以作为工作区");

    let changes = host.workspace_changes();
    assert!(!changes.repository, "普通目录不是 Git 仓库：{changes:?}");
    assert!(
        changes.error.as_deref().unwrap_or_default().contains("Git"),
        "必须给出可读的中文原因：{:?}",
        changes.error
    );

    cleanup(&plain);
    cleanup(&device);
}

/// 单文件差异过大时截断并标记，避免 UI 被无界内容拖垮。
#[test]
fn oversized_file_diffs_are_truncated_and_flagged() {
    let repo = git_repo_with_commit("git-big-diff", &[(SETTINGS_FILE, "小内容\n")]);
    // 必须是合法的 settings.toml（工作区打开时会校验），所以每行都是注释。
    let big: String = (0..3000)
        .map(|index| format!("# 第 {index} 行内容\n"))
        .collect();
    git_write(&repo, SETTINGS_FILE, &big);
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let changes = host.workspace_changes();
    let settings = &by_path(&changes)[SETTINGS_FILE];
    assert!(settings.diff_truncated, "过大差异必须被标记为截断");
    assert!(
        settings.diff.chars().count() < 25_000,
        "截断后的差异长度必须有界，实际 {}",
        settings.diff.chars().count()
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 空仓库（还没有任何提交）也必须能查看变更。
#[test]
fn changes_in_a_repository_without_commits_are_visible() {
    let repo = support::real_git_repo("git-unborn");
    support::set_identity(&repo, support::TEST_AUTHOR_NAME, support::TEST_AUTHOR_EMAIL);
    git_write(&repo, SETTINGS_FILE, "hotkey = \"Ctrl+Alt+Space\"\n");

    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联空仓库");

    let changes = host.workspace_changes();
    assert!(changes.repository);
    assert!(changes.has_changes, "{changes:?}");
    assert_eq!(changes.branch.as_deref(), Some("main"), "未诞生的分支名也应可见");
    assert!(
        changes.diff_base.contains("尚无提交") || changes.diff_base.contains("空仓库"),
        "差异基准必须说明还没有提交：{}",
        changes.diff_base
    );
    let files = by_path(&changes);
    assert!(files[SETTINGS_FILE].untracked);
    assert!(files[SETTINGS_FILE].diff.contains("+hotkey"), "{}", files[SETTINGS_FILE].diff);

    cleanup(&repo);
    cleanup(&device);
}

/// 提交范围严格等于显式选择的路径：用户已有的暂存改动既不被吞进提交，也不被破坏。
#[test]
fn commit_includes_only_selected_paths_and_preserves_preexisting_staged_changes() {
    let repo = changes_fixture("git-commit-scope");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let head_before = support::git_head_oid(&repo);
    let settings_before =
        std::fs::read_to_string(repo.join(SETTINGS_FILE)).expect("读取提交前的设置文件");
    let staged_theme_before =
        support::git_index_oid(&repo, "theme.json").expect("theme.json 应已暂存");

    let outcome = host
        .commit_workspace(
            "提交设置与备忘录",
            &[SETTINGS_FILE.to_string(), "memos/2026-10-01.md".to_string()],
        )
        .expect("提交必须成功");

    // 提交对象真实存在，父提交、说明与作者身份都要核对（不只相信返回值）。
    let oid = git2::Oid::from_str(&outcome.oid).expect("oid 必须可解析");
    assert_eq!(support::git_head_oid(&repo), oid, "HEAD 必须推进到新提交");
    let (author, email, parents) = support::git_commit_signature(&repo, oid);
    assert_eq!(author, support::TEST_AUTHOR_NAME, "作者名");
    assert_eq!(email, support::TEST_AUTHOR_EMAIL, "作者邮箱");
    assert_eq!(parents, 1, "应有且仅有一个父提交");
    let repository = git2::Repository::open(&repo).expect("打开仓库");
    let commit = repository.find_commit(oid).expect("提交必须能再次找到");
    assert_eq!(commit.message().expect("提交说明"), "提交设置与备忘录");
    assert_eq!(commit.parent_id(0).expect("父提交"), head_before);

    // 提交树：选中的路径取工作区内容。
    assert_eq!(
        support::git_show(&repo, oid, SETTINGS_FILE).as_deref(),
        Some("hotkey = \"Super+Space\"\n"),
        "选中路径必须提交工作区内容"
    );
    assert_eq!(
        support::git_show(&repo, oid, "memos/2026-10-01.md").as_deref(),
        Some("备忘录内容：今天做的事\n"),
        "未跟踪的新文件也必须能提交"
    );
    assert_eq!(
        support::git_show(&repo, oid, "other.txt").as_deref(),
        Some("无关文件\n"),
        "无关文件必须保持 HEAD 内容"
    );

    // 关键：未选中的 theme.json 的**已有暂存改动**不得进入提交。
    assert_eq!(
        support::git_show(&repo, oid, "theme.json").as_deref(),
        Some("{\"theme\":\"dark\"}\n"),
        "未选中文件里用户已暂存的内容被吞进了提交"
    );
    assert_eq!(
        support::git_tree_paths(&repo, oid),
        vec![
            "memos/2026-10-01.md".to_string(),
            "other.txt".to_string(),
            SETTINGS_FILE.to_string(),
            "theme.json".to_string(),
        ],
        "提交树必须恰好是 HEAD 树 + 选中路径的工作区内容"
    );

    // 用户已有的暂存条目原样保留（`index` 里仍是那份中间内容）。
    assert_eq!(
        support::git_index_oid(&repo, "theme.json"),
        Some(staged_theme_before),
        "用户已有的暂存条目被破坏"
    );

    // 提交后的变更视图：选中的路径消失，未选中的暂存改动仍在。
    let files = by_path(&host.workspace_changes());
    assert!(
        !files.contains_key(SETTINGS_FILE),
        "已提交的路径必须从变更列表消失：{files:?}"
    );
    assert!(
        !files.contains_key("memos/2026-10-01.md"),
        "已提交的新文件必须从变更列表消失：{files:?}"
    );
    assert!(
        files["theme.json"].staged,
        "未选中文件的暂存改动必须仍在变更列表：{files:?}"
    );

    // 提交不触碰工作区文件：设置与备忘录内容逐字节不变，设置继续正常读取。
    assert_eq!(
        std::fs::read_to_string(repo.join(SETTINGS_FILE)).expect("读取提交后的设置文件"),
        settings_before,
        "提交不得改写工作区文件"
    );
    assert_eq!(
        host.settings().hotkey,
        "Super+Space",
        "提交后设置必须仍能正常读取"
    );
    assert!(host.workspace_status().valid, "提交后工作区必须仍然有效");
    assert!(
        outcome.summary().contains(&outcome.short) && outcome.summary().contains('2'),
        "结果说明必须包含缩写 oid 与文件数：{}",
        outcome.summary()
    );
    assert_eq!(
        outcome.paths,
        vec![SETTINGS_FILE.to_string(), "memos/2026-10-01.md".to_string()]
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 只有显式勾选才提交：空选择与不在变更列表中的路径都被拒绝，仓库丝毫不被改动。
#[test]
fn commit_refuses_empty_or_unknown_selection_without_touching_the_repository() {
    let repo = changes_fixture("git-commit-guard");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let head_before = support::git_head_oid(&repo);
    let index_before = support::git_index_bytes(&repo);

    let empty = host
        .commit_workspace("说明", &[])
        .expect_err("空选择必须被拒绝");
    assert!(
        matches!(empty, flashcast_core::GitError::EmptySelection),
        "空选择应给出 EmptySelection：{empty:?}"
    );
    assert!(empty.to_string().contains("没有选择"), "{empty}");

    let unchanged = host
        .commit_workspace("说明", &["other.txt".to_string()])
        .expect_err("没有改动的路径必须被拒绝");
    assert!(
        matches!(unchanged, flashcast_core::GitError::UnknownPath(_)),
        "未变更路径应给出 UnknownPath：{unchanged:?}"
    );

    let traversal = host
        .commit_workspace("说明", &["../outside.txt".to_string()])
        .expect_err("越界路径必须被拒绝");
    assert!(
        matches!(traversal, flashcast_core::GitError::UnknownPath(_)),
        "越界路径应给出 UnknownPath：{traversal:?}"
    );

    assert_eq!(support::git_head_oid(&repo), head_before, "不得产生提交");
    assert_eq!(
        support::git_index_bytes(&repo),
        index_before,
        "被拒绝的提交不得改动索引"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 空仓库（无任何提交）也能创建第一个提交，父提交为空。
#[test]
fn commit_creates_the_first_commit_in_an_empty_repository() {
    let repo = support::real_git_repo("git-first-commit");
    support::set_identity(&repo, support::TEST_AUTHOR_NAME, support::TEST_AUTHOR_EMAIL);
    std::fs::write(repo.join(SETTINGS_FILE), "hotkey = \"Super+Space\"\n")
        .expect("写入设置文件");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联空仓库");

    let outcome = host
        .commit_workspace("第一个提交", &[SETTINGS_FILE.to_string()])
        .expect("空仓库必须能创建第一个提交");
    let oid = git2::Oid::from_str(&outcome.oid).expect("oid");
    let (_, _, parents) = support::git_commit_signature(&repo, oid);
    assert_eq!(parents, 0, "第一个提交不应有父提交");
    assert_eq!(
        support::git_show(&repo, oid, SETTINGS_FILE).as_deref(),
        Some("hotkey = \"Super+Space\"\n")
    );
    let files = by_path(&host.workspace_changes());
    assert!(
        !files.contains_key(SETTINGS_FILE),
        "提交后该文件必须干净：{files:?}"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 提交失败不丢修改：提交对象无法写入时，索引按字节还原、工作区内容不变、不产生提交。
#[test]
fn failed_commit_restores_the_index_and_keeps_the_changes() {
    let repo = changes_fixture("git-commit-failure");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let head_before = support::git_head_oid(&repo);
    let index_before = support::git_index_bytes(&repo);
    let staged_theme_before =
        support::git_index_oid(&repo, "theme.json").expect("theme.json 应已暂存");
    let settings_before =
        std::fs::read_to_string(repo.join(SETTINGS_FILE)).expect("读取设置文件");

    // 占住分支引用的锁文件：更新引用必然失败，提交无法完成。
    let head_ref = support::git_head_ref(&repo);
    let lock = format!(
        "{}.lock",
        head_ref.strip_prefix("refs/heads/").expect("分支引用")
    );
    support::git_put_marker(&repo, &format!("refs/heads/{lock}"), "");

    let error = host
        .commit_workspace(
            "会失败的提交",
            &[SETTINGS_FILE.to_string(), "memos/2026-10-01.md".to_string()],
        )
        .expect_err("引用被锁定时提交必须失败");
    let message = error.to_string();
    assert!(
        message.contains("Git") || message.contains("索引") || message.contains("失败"),
        "失败原因必须是可读的中文说明：{message}"
    );

    // 提交失败后仓库状态必须原样：HEAD、索引字节、工作区文件都不变。
    assert_eq!(support::git_head_oid(&repo), head_before, "不得产生提交");
    assert_eq!(
        support::git_index_bytes(&repo),
        index_before,
        "提交失败后索引必须按字节还原"
    );
    assert_eq!(
        std::fs::read_to_string(repo.join(SETTINGS_FILE)).expect("读取设置文件"),
        settings_before,
        "提交失败不得丢失工作区修改"
    );
    let files = by_path(&host.workspace_changes());
    assert!(files.contains_key(SETTINGS_FILE), "失败的提交不得让改动消失");
    // 同时证明失败前确实动过索引、之后被还原：选中路径不得以工作区内容留在索引里。
    assert_eq!(
        support::git_index_blob(&repo, SETTINGS_FILE).as_deref(),
        Some("hotkey = \"Ctrl+Alt+Space\"\n"),
        "失败提交不得把工作区内容留在索引里"
    );
    assert_eq!(
        support::git_index_blob(&repo, "memos/2026-10-01.md"),
        None,
        "失败提交不得把未跟踪的新文件留在索引里"
    );
    assert_eq!(
        support::git_index_oid(&repo, "theme.json"),
        Some(staged_theme_before),
        "失败提交不得破坏用户已有的暂存条目"
    );
    assert_eq!(files[SETTINGS_FILE].code, " M", "设置文件必须仍是「未暂存修改」");

    // 拿掉锁之后，同一份选择必须能正常提交（证明失败不是别的原因）。
    support::git_remove_marker(&repo, &format!("refs/heads/{lock}"));
    let outcome = host
        .commit_workspace(
            "重试成功",
            &[SETTINGS_FILE.to_string(), "memos/2026-10-01.md".to_string()],
        )
        .expect("解除锁定后提交必须成功");
    assert_eq!(
        support::git_show(&repo, git2::Oid::from_str(&outcome.oid).expect("oid"), SETTINGS_FILE)
            .as_deref(),
        Some("hotkey = \"Super+Space\"\n")
    );

    cleanup(&repo);
    cleanup(&device);
}

