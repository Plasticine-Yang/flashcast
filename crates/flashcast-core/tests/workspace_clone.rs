//! 从远端克隆配置工作区的集成测试（ticket 14）。
//!
//! 全部经由 `Host` 的克隆入口验证（ADR §3 / §10）。远端是真实的临时**裸仓库**
//! （`git2::Repository::init_bare`），走真实的 `git2` 克隆实现；本机无法访问真实
//! 网络远端，因此 https / ssh 的远端行为在 ticket 14 的 Comments 里记为未覆盖。

mod support;

use std::fs;
use std::path::Path;
use std::sync::Arc;

use flashcast_core::{
    CloneControl, ClonePhase, PluginRegistry, Settings, SETTINGS_FILE,
};
use support::{
    bare_remote, cleanup, fast_settings, files_under, host_restarted, host_with_device,
    host_with_plugins_device, real_git_repo, unique_dir, workspace_files, StaticPlugin,
};

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

/// 目标目录已有用户文件时拒绝克隆：不覆盖、不建仓库、不改动当前工作区与设置。
#[test]
fn cloning_refuses_a_non_empty_target_without_touching_it() {
    let files = workspace_files("Super+Space");
    let remote = bare_remote("clone-refuse-remote", &files);
    let parent = unique_dir("clone-refuse");
    let target = parent.join("occupied");
    fs::create_dir_all(&target).expect("准备目标目录");
    let keep = target.join("keep.txt");
    fs::write(&keep, "用户自己写的内容").expect("准备已有文件");

    let existing = real_git_repo("clone-refuse-existing");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());
    host.select_workspace(&existing).expect("关联已有仓库");
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+K".to_string(),
        ..Settings::default()
    })
    .expect("写入设置");

    let error = host
        .clone_workspace(&remote.to_string_lossy(), &target)
        .expect_err("非空目标必须拒绝克隆");
    let message = error.to_string();
    assert!(
        message.contains("非空") && message.contains("覆盖"),
        "必须是清晰的中文原因：{message}"
    );

    assert_eq!(
        fs::read_to_string(&keep).expect("已有文件必须保留"),
        "用户自己写的内容"
    );
    assert!(!target.join(".git").exists(), "拒绝时不得建立 Git 仓库");
    assert_eq!(
        host.settings().hotkey,
        "Ctrl+Shift+K",
        "当前设置必须原样保留"
    );
    assert_eq!(
        host.workspace_status().path.as_deref(),
        Some(existing.canonicalize().expect("规范化").as_path()),
        "当前工作区必须保持不变"
    );
    assert_eq!(host.clone_progress().phase, ClonePhase::Failed);

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&existing);
    cleanup(&device);
}

/// 克隆出来的配置无效时：回滚整个目录、保留当前工作区与设置，之后重试可成功。
#[test]
fn a_failed_clone_leaves_no_half_directory_and_can_be_retried() {
    let bad = bare_remote(
        "clone-bad-remote",
        &[
            ("settings.toml", "hotkey = \"不是快捷键\"\n"),
            ("memos/hello.md", "无效配置的远端\n"),
        ],
    );
    let good_files = workspace_files("Ctrl+Alt+G");
    let good = bare_remote("clone-good-remote", &good_files);

    let existing = real_git_repo("clone-failed-existing");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());
    host.select_workspace(&existing).expect("关联已有仓库");
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+J".to_string(),
        ..Settings::default()
    })
    .expect("写入设置");

    let parent = unique_dir("clone-failed");
    let target = parent.join("flashcast-config");
    let error = host
        .clone_workspace(&bad.to_string_lossy(), &target)
        .expect_err("配置无效的远端必须让克隆整体失败");
    assert!(
        error.to_string().contains("配置无效") || error.to_string().contains("快捷键"),
        "必须是清晰的中文原因：{error}"
    );
    assert!(
        !target.exists(),
        "失败后不得留下半成品目录：{}",
        target.display()
    );
    assert_eq!(
        host.settings().hotkey,
        "Ctrl+Shift+J",
        "失败后必须保留当前设置"
    );
    assert_eq!(
        host.workspace_status().path.as_deref(),
        Some(existing.canonicalize().expect("规范化").as_path())
    );
    assert!(host.workspace_status().remote.is_none(), "未克隆成功不得记录远端");

    // 重试：目标目录未被半成品占用，正常远端可以克隆成功。
    let outcome = host
        .clone_workspace(&good.to_string_lossy(), &target)
        .expect("失败后重试必须成功");
    assert!(outcome.workspace.valid);
    assert_eq!(host.settings().hotkey, "Ctrl+Alt+G");

    cleanup(&bad);
    cleanup(&good);
    cleanup(&parent);
    cleanup(&existing);
    cleanup(&device);
}

/// 远端不可达（地址不存在）时同样回滚，不留目录。
#[test]
fn a_clone_that_cannot_reach_the_remote_rolls_back_the_directory() {
    let parent = unique_dir("clone-unreachable");
    let target = parent.join("flashcast-config");
    let missing = parent.join("no-such-remote");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());

    let error = host
        .clone_workspace(&missing.to_string_lossy(), &target)
        .expect_err("不存在的远端必须失败");
    assert!(!error.to_string().is_empty(), "必须有中文原因");
    assert!(
        !target.exists(),
        "失败后不得留下半成品目录：{}",
        target.display()
    );
    assert_eq!(host.clone_progress().phase, ClonePhase::Failed);
    assert!(!host.workspace_status().valid, "失败不得关联工作区");

    cleanup(&parent);
    cleanup(&device);
}

/// 已请求取消的克隆：不创建目录、不改动当前设置，之后可以重新克隆。
#[test]
fn a_cancelled_clone_keeps_the_current_configuration_and_can_be_retried() {
    let files = workspace_files("Ctrl+Alt+C");
    let remote = bare_remote("clone-cancel-remote", &files);
    let parent = unique_dir("clone-cancel");
    let target = parent.join("flashcast-config");

    let existing = real_git_repo("clone-cancel-existing");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());
    host.select_workspace(&existing).expect("关联已有仓库");
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+L".to_string(),
        ..Settings::default()
    })
    .expect("写入设置");

    let control = CloneControl::new();
    control.cancel();
    let error = host
        .clone_workspace_with_control(&remote.to_string_lossy(), &target, &control)
        .expect_err("已取消的克隆必须失败");
    assert!(
        error.to_string().contains("取消"),
        "必须说明是取消：{error}"
    );
    assert_eq!(host.clone_progress().phase, ClonePhase::Cancelled);
    assert!(
        !target.exists(),
        "取消后不得留下半成品目录：{}",
        target.display()
    );
    assert_eq!(host.settings().hotkey, "Ctrl+Shift+L");
    assert_eq!(
        host.workspace_status().path.as_deref(),
        Some(existing.canonicalize().expect("规范化").as_path())
    );

    // 重试（不取消）：同一路径可以正常克隆。
    let outcome = host
        .clone_workspace(&remote.to_string_lossy(), &target)
        .expect("取消后重试必须成功");
    assert!(outcome.workspace.valid);
    assert_eq!(host.settings().hotkey, "Ctrl+Alt+C");

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&existing);
    cleanup(&device);
}

/// 克隆进行中按下取消：取消落在检出通知阶段，回滚干净，随后可以重新克隆。
#[test]
fn cancelling_during_a_clone_rolls_back_and_can_be_retried() {
    let file_count = 300;
    let mut files: Vec<(String, String)> = Vec::with_capacity(file_count + 1);
    files.push((
        "settings.toml".to_string(),
        "hotkey = \"Ctrl+Alt+M\"\n".to_string(),
    ));
    for index in 0..file_count {
        files.push((
            format!("memos/memo-{index}.md"),
            format!("# 备忘录 {index}\n"),
        ));
    }
    let remote = bare_remote("clone-mid-cancel-remote", &files);
    let parent = unique_dir("clone-mid-cancel");
    let target = parent.join("flashcast-config");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+N".to_string(),
        ..Settings::default()
    })
    .expect("未关联工作区时设置只在内存中生效");

    let control = CloneControl::new();
    control.start();
    let cancel_control = control.clone();
    let watcher = std::thread::spawn(move || {
        // libgit2 先跑完一轮检出通知（plan）再写文件，取消只在这一轮里生效；
        // 等它通知到第 5 个文件时取消，此时还有约 295 个文件。
        for _ in 0..200_000 {
            let notified = cancel_control.progress().checkout_notified;
            if notified >= 5 {
                cancel_control.cancel();
                return notified;
            }
            std::thread::sleep(std::time::Duration::from_micros(20));
        }
        0
    });

    let error = host
        .clone_workspace_with_control(&remote.to_string_lossy(), &target, &control)
        .expect_err("克隆过程中取消必须失败");
    let observed = watcher.join().expect("取消线程正常结束");
    assert!(observed > 0, "必须观察到真实的检出通知回调");

    assert!(
        error.to_string().contains("取消"),
        "必须说明是取消：{error}"
    );
    let progress = host.clone_progress();
    assert_eq!(progress.phase, ClonePhase::Cancelled, "{progress:?}");
    assert!(
        !target.exists(),
        "取消后不得留下半成品目录（发现 {} 个文件）",
        counts_files(&target)
    );
    assert!(!host.workspace_status().valid, "取消不得关联工作区");
    assert_eq!(
        host.settings().hotkey,
        "Ctrl+Shift+N",
        "取消不得损坏当前设置"
    );

    // 重试：不取消时必须能克隆成功。
    let retried = host
        .clone_workspace(&remote.to_string_lossy(), &target)
        .expect("取消后重试必须成功");
    assert!(retried.workspace.valid);
    assert_eq!(host.settings().hotkey, "Ctrl+Alt+M");
    assert!(counts_files(&target) >= file_count, "重试必须检出全部文件");

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&device);
}

/// 凭证绝不进入工作区、配置或错误文本：地址里的口令被拒绝且不会泄露。
#[test]
fn credentials_never_leak_into_errors_the_workspace_or_the_config() {
    let parent = unique_dir("clone-secret");
    let target = parent.join("flashcast-config");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());

    let error = host
        .clone_workspace("https://alice:sekrit-token-value@example.invalid/config.git", &target)
        .expect_err("地址带口令必须拒绝");
    let message = error.to_string();
    assert!(
        message.contains("密码") && message.contains("令牌"),
        "必须给出可操作的中文指引：{message}"
    );
    assert!(!message.contains("sekrit-token-value"), "错误文本不得含口令：{message}");
    assert!(!message.contains("alice:"), "错误文本不得含 userinfo：{message}");
    assert!(!target.exists());

    // 设备本地保存的令牌不会出现在工作区文件里，也不会出现在界面读到的远端地址里。
    let files = workspace_files("Super+Space");
    let remote = bare_remote("clone-secret-remote", &files);
    let outcome = host
        .clone_workspace(&remote.to_string_lossy(), &parent.join("ws"))
        .expect("正常克隆必须成功");
    let workspace = outcome.workspace.path.clone().expect("工作区路径");

    host.remember_git_token("https://github.com/me/config.git", "x-access-token", "ghp_0123456789abcdefghijklmnopqrstuvwx")
        .expect("保存令牌");
    let secret = "ghp_0123456789abcdefghijklmnopqrstuvwx";
    for file in files_under(&device) {
        // 令牌只允许出现在设备本地的凭证文件里。
        let text = fs::read_to_string(&file).unwrap_or_default();
        if file.ends_with("git-credentials.json") {
            assert!(text.contains(secret), "凭证文件应当保存令牌：{}", file.display());
        } else {
            assert!(
                !text.contains(secret),
                "令牌不得出现在其他设备文件里：{}",
                file.display()
            );
        }
    }
    for file in files_under(&workspace) {
        let text = fs::read_to_string(&file).unwrap_or_default();
        assert!(
            !text.contains(secret),
            "凭证绝不得进入工作区：{}",
            file.display()
        );
    }

    // 仓库里被人为写入带 userinfo 的地址时，读出来的远端地址必须已脱敏。
    {
        let repo = git2::Repository::open(&workspace).expect("打开克隆出的仓库");
        repo.remote_set_url("origin", "https://alice:sekrit-token-value@example.com/config.git")
            .expect("改写远端地址");
    }
    let relation = host.workspace_remote().expect("重新读取远端关系");
    assert!(
        !relation.url.contains("sekrit-token-value") && !relation.url.contains("alice"),
        "远端地址必须去掉 userinfo：{}",
        relation.url
    );
    let device_text = fs::read_to_string(device.join("device-local.json")).unwrap_or_default();
    assert!(
        !device_text.contains("sekrit-token-value"),
        "设备本地记录不得含口令"
    );

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&device);
}

/// 克隆成功后按工作区恢复插件选择，并如实报告本机没有的实现与记录的主题。
#[test]
fn cloning_restores_plugin_choices_and_reports_what_is_unavailable() {
    let files = vec![
        (
            "settings.toml".to_string(),
            "hotkey = \"Ctrl+Alt+P\"\nlaunchAtStartup = false\nquickAccessLimit = 6\npluginTimeoutMs = 400\ndisabledPlugins = [\"memo\", \"ghost-plugin\"]\n".to_string(),
        ),
        ("theme.json".to_string(), "{\"theme\": \"dark\"}\n".to_string()),
    ];
    let remote = bare_remote("clone-plugins-remote", &files);
    let parent = unique_dir("clone-plugins");
    let target = parent.join("flashcast-config");

    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(StaticPlugin::new("memo", Vec::new())));
    let (host, _launcher, device) =
        host_with_plugins_device(vec![], Settings::default(), plugins);

    let outcome = host
        .clone_workspace(&remote.to_string_lossy(), &target)
        .expect("克隆必须成功");

    // 插件选择随工作区恢复：memo 已注册，因此按记录停用；ghost-plugin 本机没有实现。
    assert!(!host.plugins().is_enabled("memo"), "记录为停用的插件必须停用");
    assert_eq!(outcome.unavailable_plugins, vec!["ghost-plugin".to_string()]);
    // 主题支持由 ticket 06 落地，这里如实报告工作区记录的主题名。
    assert_eq!(outcome.recorded_theme.as_deref(), Some("dark"));

    cleanup(&remote);
    cleanup(&parent);
    cleanup(&device);
}

/// 统计目录下的文件数（目标目录不存在时为 0）。
fn counts_files(root: &Path) -> usize {
    files_under(root).len()
}

/// 远端不可达（ssh 端口无服务）时给出可操作的中文指引，且不留目录、不改动配置。
#[test]
fn an_unreachable_ssh_remote_gives_actionable_chinese_guidance() {
    let parent = unique_dir("clone-ssh-unreachable");
    let target = parent.join("flashcast-config");
    let (host, _launcher, device) = host_with_device(vec![], Settings::default());
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+O".to_string(),
        ..Settings::default()
    })
    .expect("设置只在内存中生效");

    let error = host
        .clone_workspace("ssh://git@127.0.0.1:1/flashcast-config.git", &target)
        .expect_err("不可达的 ssh 远端必须失败");
    let message = error.to_string();
    assert!(
        message.contains("网络")
            || message.contains("ssh-agent")
            || message.contains("~/.ssh")
            || message.contains("鉴权"),
        "必须给出可操作的指引：{message}"
    );
    assert!(
        !target.exists(),
        "失败后不得留下半成品目录：{}",
        target.display()
    );
    assert_eq!(host.settings().hotkey, "Ctrl+Shift+O");
    assert!(!host.workspace_status().valid);

    cleanup(&parent);
    cleanup(&device);
}
