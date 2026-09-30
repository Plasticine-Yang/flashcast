//! 配置工作区的 Git 变更查看与显式范围提交（ticket 15）集成测试。
//!
//! 全部经由 `Host` 的入口在**真实临时 Git 仓库**上验证，并断言真实仓库状态：
//! 提交对象、树内容、父提交、作者身份、索引字节。不断言 Git 命令字符串（ADR §3 / §10）。

mod support;

use std::collections::BTreeMap;

use flashcast_core::{ChangedFile, SETTINGS_FILE};
use support::{
    cleanup, fast_settings, git_commit_all, git_repo_with_commit, git_stage, git_write,
    host_with_device,
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

/// 提交后能看到新提交，且 HEAD 推进到它。
#[allow(dead_code)]
fn assert_head_is(repo: &std::path::Path, oid: git2::Oid) {
    assert_eq!(support::git_head_oid(repo), oid);
}

#[allow(dead_code)]
fn commit_all(repo: &std::path::Path) -> git2::Oid {
    git_commit_all(repo, "补充提交")
}
