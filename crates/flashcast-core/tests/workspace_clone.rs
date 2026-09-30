//! 从远端克隆配置工作区的集成测试（ticket 14）。
//!
//! 全部经由 `Host` 的克隆入口验证（ADR §3 / §10）。远端是真实的临时**裸仓库**
//! （`git2::Repository::init_bare`），走真实的 `git2` 克隆实现；本机无法访问真实
//! 网络远端，因此 https / ssh 的远端行为在 ticket 14 的 Comments 里记为未覆盖。

mod support;

use flashcast_core::{ClonePhase, Settings, SETTINGS_FILE};
use support::{bare_remote, cleanup, fast_settings, host_restarted, host_with_device, unique_dir, workspace_files};

/// 从本地裸远端克隆：校验通过、成为当前工作区、已有功能读取其中内容，
/// 远端关系（远端名、默认分支、上游）被记录，进度来自真实回调。
#[test]
fn cloning_from_a_local_bare_remote_links_the_workspace_and_reads_its_content() {
    let files = workspace_files("Super+Space");
    let remote = bare_remote("clone-remote", &files);
    let parent = unique_dir("clone-target");
    let target = parent.join("flashcast-config");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());

    let outcome = host
        .clone_workspace(&remote.to_string_lossy(), &target)
        .expect("从本地裸远端克隆必须成功");

    let canonical = target.canonicalize().expect("规范化目标目录");
    assert_eq!(outcome.workspace.path.as_deref(), Some(canonical.as_path()));
    assert!(outcome.workspace.valid, "克隆出的工作区必须有效：{outcome:?}");
    assert!(outcome.workspace.git_dir.is_some(), "必须有 Git 仓库");

    // 已有功能读取克隆内容：设置来自工作区的 settings.toml，文件也在磁盘上。
    assert_eq!(host.settings().hotkey, "Super+Space", "设置必须来自克隆的工作区");
    assert!(canonical.join(SETTINGS_FILE).exists());
    assert!(
        canonical.join("memos/hello.md").exists(),
        "远端内容必须被检出"
    );

    // 远端关系：ticket 16 的同步据此工作。
    assert_eq!(outcome.remote.name, "origin");
    assert_eq!(outcome.remote.branch, "main");
    assert_eq!(outcome.remote.upstream.as_deref(), Some("origin/main"));

    // 进度来自 git2 的真实回调，而不是凭空拼出来的状态。
    let progress = host.clone_progress();
    assert_eq!(progress.phase, ClonePhase::Done, "完成后阶段为 Done：{progress:?}");
    assert!(progress.updates > 0, "必须有真实的进度回调：{progress:?}");
    // 本地传输不走对象传输，但检出进度来自真实回调（CheckoutBuilder::progress）。
    // 对象传输回调（transfer_progress / sideband_progress）只在真实网络远端触发，
    // 见 ticket 14 Comments 的未覆盖说明。
    assert_eq!(
        progress.checkout_total, 4,
        "检出进度必须报告远端里的文件数：{progress:?}"
    );
    assert_eq!(progress.checkout_completed, 4, "所有文件都必须检出：{progress:?}");

    // 关系持久化：重启同一设备目录后仍然可用。
    let restarted = host_restarted(&device, Settings::default());
    let remote_after_restart = restarted
        .workspace_status()
        .remote
        .expect("重启后必须仍记录远端关系");
    assert_eq!(remote_after_restart.branch, "main");
    assert_eq!(remote_after_restart.name, "origin");

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&device);
    let _ = fast_settings();
}
