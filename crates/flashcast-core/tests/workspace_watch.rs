//! 工作区文件监听的集成测试：真实临时 Git 仓库 + 真实文件系统事件。
//!
//! 覆盖 ADR §8 与 `notes/research/git-workspace.md` §6 要求的四层自写抑制：
//! 原子写入、内容哈希自写账本、每路径静默窗口、Git 操作忙标志，以及 gitdir 排除。
//! 全部经由宿主入口验证（ADR §10），不直接调用去抖器或过滤器。

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use flashcast_core::{
    Appearance, Host, PluginManifestFile, Settings, ThemeSelection, MANIFEST_FILE, SETTINGS_FILE,
    THEME_DARK, THEME_FILE, THEME_LIGHT,
};
use support::{cleanup, fast_settings, host_with_device, real_git_repo};

/// 等待可处理的外部变更的上限。去抖窗口 500ms，给足余量。
const WAIT: Duration = Duration::from_secs(6);
/// 断言「没有发生重载」的观察时长：必须明显长于去抖窗口。
const QUIET: Duration = Duration::from_millis(1600);

fn write_settings_file(repo: &Path, settings: &Settings) {
    fs::write(repo.join(SETTINGS_FILE), settings.to_toml().unwrap()).expect("写入设置文件");
}

/// 用与当前内容**完全相同**的字节再写一次设置文件。
///
/// 模拟的是「同一次逻辑写入的第二条事件记录」：Linux 的 inotify 一次 `rename` 基本只上报
/// 一条，而 macOS 的 FSEvents 与 Windows 的 `ReadDirectoryChangesW` 常上报多条。这里用
/// 真写一次来补齐那条记录，内容不变，因此它必须被内容哈希账本吞掉。
fn rewrite_settings_unchanged(repo: &Path) -> PathBuf {
    let path = repo
        .canonicalize()
        .expect("解析工作区路径")
        .join(SETTINGS_FILE);
    let bytes = fs::read(&path).expect("读取设置文件");
    fs::write(&path, bytes).expect("用相同内容重写设置文件");
    path
}

/// 把最近一次事件轨迹描述成断言消息里的一行。
///
/// 「多了一次重载」本身没有信息量：需要知道究竟是哪条事件（路径、事件类型）从哪个
/// 分支漏出来的。这里同时给出最后一次被接受的事件与最后一次过滤决策。
fn watch_trace(host: &Host) -> String {
    let describe = |label: &str, trace: Option<flashcast_core::WatchEventTrace>| match trace {
        Some(trace) => format!("{label}：{}", trace.describe()),
        None => format!("{label}：无"),
    };
    format!(
        "{}；{}",
        describe("最后接受的事件", host.last_watch_event()),
        describe("最后的事件决策", host.last_watch_decision())
    )
}

/// 外部编辑有效的设置文件后，宿主自动重新加载并生效。
#[test]
fn external_edit_of_settings_is_reloaded() {
    let repo = real_git_repo("watch-external");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let initial = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(initial.clone()).expect("保存初始设置");
    // 自身写入必须被吞掉，先把基线确认清楚。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "自身写入不得触发重载（{}）",
        watch_trace(&host)
    );

    // 外部（编辑器 / 其他进程）改成另一份合法配置。
    let edited = Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        quick_access_limit: 9,
        ..initial
    };
    write_settings_file(&repo, &edited);

    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("外部修改必须触发重载");
    assert!(reload.applied, "有效的外部修改必须生效：{reload:?}");
    assert!(reload.error.is_none(), "有效配置不应报错：{reload:?}");
    assert_eq!(
        reload.path,
        repo.canonicalize().unwrap().join(SETTINGS_FILE)
    );
    assert_eq!(host.settings(), edited, "重载后设置必须更新");
    assert_eq!(host.workspace_reloads(), 1);
    assert!(host.workspace_status().error.is_none());
    // 一次外部修改只应产生一次重载：重载本身要读文件，而读文件不得再次触发事件
    // （inotify 的 Access 事件曾让这里变成永不收敛的事件流）。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "一次外部修改不得产生持续的事件流（{}；已计重载 {}）",
        watch_trace(&host),
        host.workspace_reloads()
    );
    assert_eq!(
        host.workspace_reloads(),
        1,
        "不得出现重复重载（{}）",
        watch_trace(&host)
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 外部编辑成无效配置：保留上一次有效设置，并给出中文原因。
#[test]
fn invalid_external_edit_keeps_the_last_valid_settings() {
    let repo = real_git_repo("watch-invalid");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    let valid = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(valid.clone()).expect("保存有效设置");
    assert!(host.wait_for_workspace_change(QUIET).is_none());

    fs::write(repo.join(SETTINGS_FILE), "hotkey = 42\n").expect("写入无效配置");

    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("无效修改也必须被处理");
    assert!(!reload.applied, "无效配置不得生效：{reload:?}");
    let message = reload.error.expect("必须给出原因");
    assert!(
        message.contains("配置无效") && message.contains(SETTINGS_FILE),
        "原因必须可读且指向设置文件：{message}"
    );
    assert_eq!(host.settings(), valid, "必须保留上一次有效设置");
    assert_eq!(host.workspace_reloads(), 0, "无效配置不算一次生效的重载");

    let status = host.workspace_status();
    assert_eq!(status.error.as_deref(), Some(message.as_str()));
    assert!(!status.valid, "配置无效时状态必须如实标记");
    assert_eq!(
        status.path.as_deref(),
        Some(repo.canonicalize().unwrap().as_path()),
        "仍然保持关联（只是配置无效）"
    );

    // 修好后可以恢复有效状态。
    let fixed = Settings {
        hotkey: "Ctrl+Shift+J".to_string(),
        ..valid
    };
    write_settings_file(&repo, &fixed);
    let reload = host.wait_for_workspace_change(WAIT).expect("修复后应重载");
    assert!(reload.applied);
    assert_eq!(host.settings().hotkey, "Ctrl+Shift+J");
    assert!(host.workspace_status().error.is_none());

    cleanup(&repo);
    cleanup(&device);
}

/// 应用自身的写入不得触发重载循环。
#[test]
fn the_applications_own_write_does_not_form_a_reload_loop() {
    let repo = real_git_repo("watch-self-write");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    for hotkey in ["Super+Space", "Ctrl+Alt+K", "Ctrl+Shift+F2"] {
        host.update_settings(Settings {
            hotkey: hotkey.to_string(),
            ..fast_settings()
        })
        .expect("写入设置");
        assert!(
            host.wait_for_workspace_change(QUIET).is_none(),
            "自身写入 {hotkey} 触发了重载（{}）",
            watch_trace(&host)
        );
    }

    assert_eq!(host.workspace_reloads(), 0, "自身写入不得计入重载");
    assert_eq!(host.settings().hotkey, "Ctrl+Shift+F2");
    assert!(
        !repo
            .join(format!(".{SETTINGS_FILE}.flashcast.tmp"))
            .exists(),
        "原子写入不得留下临时文件"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 回归（跨平台缺陷 1）：自身写入后的**第二条**事件记录，内容未变，不得产生重载。
///
/// 一次逻辑写入在 macOS（FSEvents）与 Windows（ReadDirectoryChangesW）上常产生多条事件
/// 记录，相邻记录可能相隔数百毫秒。若第一条命中内容哈希后就把账本条目删掉，第二条只能
/// 落到每路径静默窗口上；晚于 `QUIET_WINDOW` 时它就被当成外部修改，产生一次内容其实没变
/// 的多余重载。Linux 只上报一条记录，所以这里手工补齐第二条。
#[test]
fn a_duplicate_event_with_unchanged_content_is_suppressed() {
    let repo = real_git_repo("watch-duplicate-event");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let settings = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(settings.clone()).expect("保存设置");
    // 等到第一条事件记录被处理（去抖窗口 500ms）**且**静默窗口（600ms）已经过期，
    // 第二条记录才会暴露「账本条目被第一条吃掉」这个缺陷。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "自身写入不得触发重载"
    );

    rewrite_settings_unchanged(&repo);

    // 重复记录若被放行，去抖窗口 500ms 后就会到达，`QUIET` 足够看到它。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "内容未变化的重复事件记录不得产生重载（{}）",
        watch_trace(&host)
    );
    assert_eq!(host.workspace_reloads(), 0, "内容未变化不算重载");
    assert_eq!(host.settings(), settings, "设置必须原样保留");

    cleanup(&repo);
    cleanup(&device);
}

/// 回归（跨平台缺陷 2）：重载自己要读设置文件，这次读不得再变成一次重载。
///
/// macOS 的 FSEvents 经常不给更细的事件类型，读文件会被上报成 `EventKind::Any`，按事件
/// 类型丢不掉，于是「重载 → 读 → 事件 → 重载」会自我维持。宿主把重载读到的字节也记进
/// 账本后，内容相同的后续事件必须被吞掉。这里在重载完成后用相同字节再写一次来触发它。
#[test]
fn a_reload_does_not_repeat_itself_from_its_own_read() {
    let repo = real_git_repo("watch-reload-read");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");
    assert!(host.wait_for_workspace_change(QUIET).is_none());

    let edited = Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        ..fast_settings()
    };
    write_settings_file(&repo, &edited);
    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("外部修改必须触发重载");
    assert!(reload.applied, "外部修改必须生效：{reload:?}");

    // 重载自己的那次读（以及同一逻辑写入的重复上报）内容与新设置一致，必须被吞掉。
    rewrite_settings_unchanged(&repo);
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "重载读到的内容不得再触发一次重载（{}；已计重载 {}）",
        watch_trace(&host),
        host.workspace_reloads()
    );
    assert_eq!(
        host.workspace_reloads(),
        1,
        "一次外部修改只应重载一次（{}）",
        watch_trace(&host)
    );
    assert_eq!(host.settings(), edited);

    cleanup(&repo);
    cleanup(&device);
}

/// gitdir（`.git/`）内部的变动不触发重载。
///
/// `StatusOptions::update_index(true)` 会在每次 status 查询时改写 `.git/index`，
/// 这是最典型的自伤事件风暴来源。ticket 15/16 引入 status 查询前，这一层就生效。
#[test]
fn git_directory_churn_is_ignored() {
    let repo = real_git_repo("watch-gitdir");
    let git_dir = repo.join(".git");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");
    assert!(host.wait_for_workspace_change(QUIET).is_none());

    // 模拟 status 查询改写索引与其它 git 元数据。
    fs::write(git_dir.join("index"), b"fake-index").expect("改写 .git/index");
    fs::write(git_dir.join("ORIG_HEAD"), b"deadbeef").expect("写入 .git/ORIG_HEAD");
    fs::create_dir_all(git_dir.join("refs/heads")).expect("准备 refs 目录");
    fs::write(git_dir.join("refs/heads/main"), b"deadbeef").expect("写入 ref");

    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "gitdir 内部的变动不得触发重载"
    );
    assert_eq!(host.workspace_reloads(), 0);
    assert!(host.workspace_status().error.is_none());

    // 对照：工作区里的设置文件仍然可以被重载，说明监听本身是活的。
    let edited = Settings {
        hotkey: "Ctrl+Alt+J".to_string(),
        ..host.settings()
    };
    write_settings_file(&repo, &edited);
    assert!(
        host.wait_for_workspace_change(WAIT).is_some(),
        "对照：设置文件变更必须仍然被处理"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// Git 操作忙标志置位期间丢弃变更事件，操作结束后显式重建状态。
#[test]
fn git_busy_flag_discards_changes_during_git_operations() {
    let repo = real_git_repo("watch-git-busy");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    let before = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(before.clone()).expect("保存设置");
    assert!(host.wait_for_workspace_change(QUIET).is_none());

    // Git 操作进行中：外部修改被丢弃，不产生重载。
    host.set_git_busy(true);
    write_settings_file(
        &repo,
        &Settings {
            hotkey: "Ctrl+Alt+B".to_string(),
            ..before.clone()
        },
    );
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "Git 操作期间的变更必须被丢弃"
    );
    assert_eq!(host.settings(), before, "忙标志期间不得应用外部修改");

    // 操作结束（ticket 15/16 会在此显式重读 Git 状态），后续变更恢复正常处理。
    host.set_git_busy(false);
    let after = Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        ..before.clone()
    };
    write_settings_file(&repo, &after);
    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("恢复后应处理变更");
    assert!(reload.applied, "{reload:?}");
    assert_eq!(host.settings(), after);

    cleanup(&repo);
    cleanup(&device);
}

/// 撤销工作区关联（宿主重启后工作区不可用）不影响宿主可用性。
#[test]
fn a_vanished_workspace_is_reported_instead_of_breaking_the_host() {
    let repo = real_git_repo("watch-vanished");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    host.update_settings(Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");

    // 工作区在应用未察觉时被删除（外部 `rm -rf`）。
    fs::remove_dir_all(&repo).expect("删除工作区");

    let restarted = support::host_restarted(&device, Settings::default());
    let status = restarted.workspace_status();
    assert!(status.path.is_none(), "不可用的工作区不得成为当前工作区");
    let message = status.error.expect("必须给出原因");
    assert!(
        message.contains("不可用") && message.contains("目录不存在"),
        "原因必须可读：{message}"
    );
    // 宿主仍然可用：查询照常返回。
    assert!(restarted.query("").notice.is_none());

    // 有工作区时，文件被删除也会如实报告并保留上次有效设置。
    let repo2 = real_git_repo("watch-deleted-file");
    let (host2, _launcher2, device2) = host_with_device(vec![], fast_settings());
    host2.select_workspace(&repo2).expect("关联工作区");
    host2
        .update_settings(Settings {
            hotkey: "Ctrl+Alt+L".to_string(),
            ..fast_settings()
        })
        .expect("保存设置");
    assert!(host2.wait_for_workspace_change(QUIET).is_none());
    fs::remove_file(repo2.join(SETTINGS_FILE)).expect("删除设置文件");
    let reload = host2
        .wait_for_workspace_change(WAIT)
        .expect("删除必须被处理");
    assert!(!reload.applied);
    assert!(
        reload.error.expect("必须给出原因").contains("已不存在"),
        "删除设置文件必须如实说明"
    );
    assert_eq!(host2.settings().hotkey, "Ctrl+Alt+L", "保留上次有效设置");

    cleanup(&device);
    cleanup(&device2);
    cleanup(&repo2);
}

/// 监听器在切换工作区后只跟随新的工作区。
#[test]
fn switching_workspaces_moves_the_watcher() {
    let first = real_git_repo("watch-first");
    let second = real_git_repo("watch-second");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    host.select_workspace(&first).expect("关联第一个工作区");
    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");
    assert!(host.wait_for_workspace_change(QUIET).is_none());

    host.select_workspace(&second).expect("切换到第二个工作区");

    // 旧工作区的修改不再影响当前状态。
    write_settings_file(
        &first,
        &Settings {
            hotkey: "Ctrl+Alt+O".to_string(),
            ..host.settings()
        },
    );
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "旧工作区的修改不得生效"
    );

    // 新工作区的修改必须被处理。
    let edited = Settings {
        hotkey: "Ctrl+Alt+N".to_string(),
        ..host.settings()
    };
    write_settings_file(&second, &edited);
    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("新工作区应生效");
    assert!(reload.applied, "{reload:?}");
    assert_eq!(host.settings().hotkey, "Ctrl+Alt+N");

    cleanup(&first);
    cleanup(&second);
    cleanup(&device);
}

/// 跨平台缺陷 2 之后的新判据，**完全不依赖文件监听**：已应用内容相等就不是重载。
///
/// 这是「一次外部修改只重载一次」的主机制：宿主重载时先读原始字节，与「当前已应用内容」
/// 的哈希比较，相同就不计数、不生效、不对外发事件。这里绕过监听，直接调用
/// `reload_workspace`（UI 的「重新检测」入口）来证明判据本身成立，与事件路径、事件类型、
/// FSEvents 的延迟、重复记录和静默窗口都无关。
#[test]
fn reload_is_a_noop_while_the_disk_content_equals_the_applied_content() {
    let repo = real_git_repo("watch-applied-content");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let initial = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(initial.clone()).expect("保存初始设置");

    // 内容与已应用的一致：不算一次重载，设置也不变。
    let reload = host.reload_workspace();
    assert!(!reload.applied, "内容未变不得生效：{reload:?}");
    assert!(reload.error.is_none(), "内容未变不该报错：{reload:?}");
    assert_eq!(host.workspace_reloads(), 0, "内容未变不得计入重载");
    assert_eq!(host.settings(), initial);

    // 内容真的变了：恰好一次重载。
    let edited = Settings {
        hotkey: "Ctrl+Alt+K".to_string(),
        quick_access_limit: 9,
        ..initial
    };
    write_settings_file(&repo, &edited);
    let reload = host.reload_workspace();
    assert!(reload.applied, "真实改动必须生效：{reload:?}");
    assert_eq!(reload.settings, edited);
    assert_eq!(host.settings(), edited);
    assert_eq!(host.workspace_reloads(), 1, "真实改动恰好算一次重载");

    // 同一内容再来一次：仍然不算重载（否则事件流会重复）。
    let reload = host.reload_workspace();
    assert!(!reload.applied, "同一内容不得重复生效：{reload:?}");
    assert_eq!(host.workspace_reloads(), 1, "同一内容只应算一次重载");

    cleanup(&repo);
    cleanup(&device);
}

/// 每一条能改变设置的路径都必须刷新「已应用内容」，否则过期的哈希会静默吞掉真实改动。
///
/// 覆盖四条路径：首次关联 / 启动恢复（`activate_workspace`）、应用自身写入
/// （`update_settings`）、外部重载生效（`wait_for_workspace_change`）、切换工作区。
#[test]
fn every_path_that_changes_settings_refreshes_the_applied_content() {
    // 1）首次关联：工作区里已有的配置在关联时就已生效，不得再算一次重载。
    let repo = real_git_repo("watch-applied-paths");
    let file_settings = Settings {
        hotkey: "Ctrl+Alt+1".to_string(),
        ..fast_settings()
    };
    write_settings_file(&repo, &file_settings);
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    assert_eq!(host.settings(), file_settings, "关联时必须载入工作区配置");
    assert!(
        !host.reload_workspace().applied,
        "关联时的文件内容必须已经算作已应用"
    );
    assert_eq!(host.workspace_reloads(), 0, "关联本身不计重载");

    // 2）应用自身写入：写完即已应用，不得再算一次重载。
    let written = Settings {
        hotkey: "Ctrl+Alt+2".to_string(),
        ..file_settings.clone()
    };
    host.update_settings(written.clone()).expect("保存设置");
    assert!(
        !host.reload_workspace().applied,
        "应用写入后不得再算一次重载"
    );
    assert_eq!(host.workspace_reloads(), 0);
    // 顺带把自身写入的事件排干，让下面的外部修改不受静默窗口影响。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "自身写入不得触发重载（{}）",
        watch_trace(&host)
    );

    // 3）外部重载生效：生效后哈希必须刷新，同一内容不得重复重载。
    let external = Settings {
        hotkey: "Ctrl+Alt+3".to_string(),
        ..written.clone()
    };
    write_settings_file(&repo, &external);
    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("外部修改必须触发重载");
    assert!(reload.applied, "{reload:?}");
    assert_eq!(host.workspace_reloads(), 1);
    assert!(
        !host.reload_workspace().applied,
        "重载生效后不得再把同一内容算一次重载"
    );
    assert_eq!(host.workspace_reloads(), 1);

    // 4）切换工作区：新工作区的现有内容同样算作已应用。
    let second = real_git_repo("watch-applied-paths-2");
    let second_settings = Settings {
        hotkey: "Ctrl+Alt+4".to_string(),
        ..external.clone()
    };
    write_settings_file(&second, &second_settings);
    host.select_workspace(&second).expect("切换工作区");
    assert_eq!(host.settings(), second_settings);
    assert!(
        !host.reload_workspace().applied,
        "切换后新工作区的内容必须算作已应用"
    );
    assert_eq!(host.workspace_reloads(), 1, "切换工作区本身不计重载");

    // 5）反例：已应用哈希绝不能是过期的 —— 新工作区里的真实改动仍必须生效。
    let changed = Settings {
        hotkey: "Ctrl+Alt+5".to_string(),
        ..second_settings.clone()
    };
    write_settings_file(&second, &changed);
    let reload = host.reload_workspace();
    assert!(
        reload.applied,
        "真实改动不得被过期的已应用哈希吞掉：{reload:?}"
    );
    assert_eq!(host.settings(), changed);
    assert_eq!(host.workspace_reloads(), 2);

    cleanup(&repo);
    cleanup(&second);
    cleanup(&device);
}

/// 回归：只改主题文件（`settings.toml` 一字节未动）也必须真正生效。
///
/// 旧实现把「磁盘原始字节 == 已应用内容」当成**无条件**的主判据：事件来自 `theme.json`
/// 时宿主读到的 `settings.toml` 与已应用内容逐字节相同，于是提前 `return None`，在调用
/// `apply_workspace_config` **之前**就把这次外部修改吞掉。监听层本身是活的（事件被接受、
/// 路径也正确），但主题状态永远停在旧值——用户的外部编辑静默失效，既不报错也不重载。
///
/// 判据因此按文件名收窄：内容相等只在改的确实是设置文件时才短路；「什么都没变」由
/// `apply_workspace_config` 之后的 `applied` 标志与累计消息共同判定。本用例从宿主入口
/// 观察这次外部修改，要求它恰好产生一次重载、外观真的换掉，且随后不再有重载。
#[test]
fn an_external_edit_of_the_theme_file_is_reloaded() {
    let repo = real_git_repo("watch-theme-reload");

    // 工作区里本来就有一份完整配置：设置 + 主题选择（浅色）+ 默认插件清单。
    let settings = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    write_settings_file(&repo, &settings);
    fs::write(
        repo.join(THEME_FILE),
        ThemeSelection::new(THEME_LIGHT).to_json().unwrap(),
    )
    .expect("写出主题配置");
    fs::write(
        repo.join(MANIFEST_FILE),
        PluginManifestFile::defaults().to_json().unwrap(),
    )
    .expect("写出插件清单");

    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    // 关联时按工作区恢复：浅色的选择与外观已生效，且关联本身不算一次重载。
    let initial = host.theme_state();
    assert_eq!(initial.selected, THEME_LIGHT, "关联时必须载入工作区主题");
    assert_eq!(initial.appearance, Appearance::Light);
    assert_eq!(initial.error, None, "{:?}", initial.error);
    assert_eq!(host.settings(), settings, "关联时必须载入工作区设置");
    assert_eq!(host.workspace_reloads(), 0, "关联本身不计重载");
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "关联工作区不得触发重载（{}）",
        watch_trace(&host)
    );

    // 外部只改主题配置：换成深色，`settings.toml` 保持逐字节不变。
    let settings_path = repo
        .canonicalize()
        .expect("解析工作区路径")
        .join(SETTINGS_FILE);
    let settings_before = fs::read(&settings_path).expect("读取设置文件");
    fs::write(
        repo.join(THEME_FILE),
        ThemeSelection::new(THEME_DARK).to_json().unwrap(),
    )
    .expect("外部改写主题配置");
    assert_eq!(
        fs::read(&settings_path).expect("再次读取设置文件"),
        settings_before,
        "本用例只改主题文件，设置文件必须逐字节不变"
    );

    let reload = host
        .wait_for_workspace_change(WAIT)
        .expect("外部改写主题配置必须触发重载");
    assert!(reload.applied, "外部主题修改必须真正生效：{reload:?}");
    assert!(reload.error.is_none(), "有效主题配置不应报错：{reload:?}");
    assert_eq!(
        reload.path.file_name().and_then(|name| name.to_str()),
        Some(THEME_FILE),
        "这次重载必须归因于主题文件：{reload:?}"
    );
    let state = host.theme_state();
    assert_eq!(state.selected, THEME_DARK, "主题选择必须按文件更新");
    assert_eq!(state.appearance, Appearance::Dark, "外观必须真的换掉");
    assert_eq!(state.error, None, "{:?}", state.error);
    assert_eq!(host.workspace_reloads(), 1, "一次外部修改只应重载一次");
    assert_eq!(host.settings(), settings, "设置文件没变，设置也不该变");

    // 一次外部修改只应产生一次重载：重复事件记录必须被内容判据吞掉。
    assert!(
        host.wait_for_workspace_change(QUIET).is_none(),
        "一次外部主题修改不得产生后续重载（{}）",
        watch_trace(&host)
    );
    assert_eq!(
        host.workspace_reloads(),
        1,
        "不得出现重复重载（{}）",
        watch_trace(&host)
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 让编译器确认宿主可以跨线程共享（外壳在后台线程里调用等待入口）。
#[test]
fn host_is_shareable_across_threads() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Host>();
}
