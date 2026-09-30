//! 配置工作区的集成测试：真实临时目录、真实 `git2` 仓库、真实文件读写。
//!
//! 全部经由 `Host` 的工作区与设置入口验证，不直接调用内部辅助函数（ADR §3 / §10）。

mod support;

use std::fs;
use std::path::Path;

use flashcast_core::{Settings, KEY_WORKSPACE_PATH, SETTINGS_FILE};
use support::{
    cleanup, fast_settings, files_under, host_restarted, host_with_device, real_git_repo,
    unique_dir,
};

/// 选择现有本地仓库后它成为当前工作区；设置写进可读的 TOML 文件并跨重启恢复。
#[test]
fn selecting_a_local_repository_persists_settings_across_restart() {
    let repo = real_git_repo("workspace-select");
    let canonical = repo.canonicalize().expect("规范化仓库路径");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let status = host.select_workspace(&repo).expect("选择现有仓库必须成功");
    assert_eq!(status.path.as_deref(), Some(canonical.as_path()));
    assert!(status.valid, "刚选择的工作区必须有效：{status:?}");
    assert!(status.git_dir.is_some(), "必须识别出 Git 仓库：{status:?}");
    assert!(status.persisted, "已关联工作区后设置必须落盘：{status:?}");
    assert_eq!(host.settings(), fast_settings(), "没有设置文件时沿用当前设置");

    let updated = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.update_settings(updated.clone())
        .expect("有效设置必须写入工作区");

    let text = fs::read_to_string(canonical.join(SETTINGS_FILE)).expect("必须写出设置文件");
    assert!(
        text.contains("Super+Space"),
        "设置文件必须是人类可读的 TOML：{text}"
    );

    // 重启：同一个设备目录上新建宿主，设置从工作区文件恢复。
    let restarted = host_restarted(&device, Settings::default());
    assert_eq!(
        restarted.settings().hotkey,
        "Super+Space",
        "重启后设置必须从工作区恢复"
    );
    assert_eq!(
        restarted.workspace_status().path.as_deref(),
        Some(canonical.as_path()),
        "重启后必须恢复上次的工作区"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 为新目录初始化工作区：目录、Git 仓库与设置文件一起建好，并成为当前工作区。
#[test]
fn initialising_a_new_directory_creates_the_workspace_and_its_git_repo() {
    let parent = unique_dir("workspace-init");
    let target = parent.join("flashcast-config");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let status = host.init_workspace(&target).expect("初始化新工作区必须成功");
    let canonical = target.canonicalize().expect("规范化工作区路径");

    assert_eq!(status.path.as_deref(), Some(canonical.as_path()));
    assert!(status.valid, "新初始化的 workspace 必须有效：{status:?}");
    assert!(status.persisted, "设置必须写在新建的工作区文件里");
    assert!(
        canonical.join(".git").exists(),
        "初始化必须建立真实 Git 仓库：{}",
        canonical.display()
    );
    assert!(
        git2::Repository::open(&canonical).is_ok(),
        "新建目录必须是一个可打开的 Git 仓库"
    );
    assert!(
        canonical.join(SETTINGS_FILE).exists(),
        "新工作区必须有默认的设置文件"
    );

    // 新工作区从默认设置开始，并且改动立即写进文件。
    host.update_settings(Settings {
        hotkey: "Ctrl+Shift+F1".to_string(),
        ..fast_settings()
    })
    .expect("写入设置");
    let text = fs::read_to_string(canonical.join(SETTINGS_FILE)).expect("读取设置文件");
    assert!(text.contains("Ctrl+Shift+F1"), "设置文件内容：{text}");

    cleanup(&parent);
    cleanup(&device);
}

/// 非空目录拒绝初始化：不建立 Git 仓库、不写设置文件、不改动已有文件。
#[test]
fn initialising_a_non_empty_directory_is_refused_without_clobbering() {
    let dir = unique_dir("workspace-nonempty");
    let keep = dir.join("keep.txt");
    fs::write(&keep, "用户自己写的内容").expect("准备已有文件");

    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    let before = host.settings();

    let error = host
        .init_workspace(&dir)
        .expect_err("非空目录必须拒绝初始化");

    let message = error.to_string();
    assert!(
        message.contains("非空") && message.contains("覆盖"),
        "失败原因必须是可读的中文说明：{message}"
    );
    assert_eq!(
        fs::read_to_string(&keep).expect("已有文件必须还在"),
        "用户自己写的内容",
        "已有用户文件不得被覆盖"
    );
    assert!(!dir.join(".git").exists(), "拒绝后不得建立 Git 仓库");
    assert!(
        !dir.join(SETTINGS_FILE).exists(),
        "拒绝后不得写入设置文件"
    );
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "目录里只能有用户原有文件");
    assert!(host.workspace_status().path.is_none(), "拒绝后不能关联工作区");
    assert_eq!(host.settings(), before, "拒绝后设置保持不变");

    cleanup(&dir);
    cleanup(&device);
}

/// 目标工作区配置无效时拒绝切换，保留当前工作区与最后一次有效设置。
#[test]
fn invalid_settings_in_the_target_workspace_keep_the_current_one() {
    let current = real_git_repo("workspace-valid");
    let broken = real_git_repo("workspace-broken");
    fs::write(
        broken.join(SETTINGS_FILE),
        "hotkey = \"Ctrl+Alt+Space\"\nquickAccessLimit = 0\n",
    )
    .expect("写入无效设置");

    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    let good = Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    };
    host.select_workspace(&current).expect("关联有效工作区");
    host.update_settings(good.clone()).expect("保存有效设置");

    let error = host
        .select_workspace(&broken)
        .expect_err("无效配置的工作区必须拒绝");
    let message = error.to_string();
    assert!(
        message.contains("配置无效") && message.contains(SETTINGS_FILE),
        "失败原因必须指出是哪个文件、哪里不合法：{message}"
    );

    let status = host.workspace_status();
    assert_eq!(
        status.path.as_deref(),
        Some(current.canonicalize().unwrap().as_path()),
        "失败后必须保留原工作区"
    );
    assert_eq!(host.settings(), good, "失败后必须保留最后一次有效设置");
    assert!(
        !broken.join(SETTINGS_FILE).exists() || fs::read_to_string(broken.join(SETTINGS_FILE)).unwrap().contains("quickAccessLimit = 0"),
        "拒绝切换不得改写目标工作区文件"
    );

    // 键名写错（拼写错误）也必须被发现，而不是被静默忽略成默认值。
    fs::write(
        broken.join(SETTINGS_FILE),
        "hotkey = \"Ctrl+Alt+Space\"\nquick_access_limit = 3\n",
    )
    .expect("写入拼错的键名");
    let error = host
        .select_workspace(&broken)
        .expect_err("拼错的键名必须报错而不是静默忽略");
    assert!(
        error.to_string().contains("配置无效"),
        "失败原因：{error}"
    );

    // 外部编辑成合法内容后即可切换。
    fs::write(
        broken.join(SETTINGS_FILE),
        Settings {
            hotkey: "Ctrl+Shift+Space".to_string(),
            ..fast_settings()
        }
        .to_toml()
        .unwrap(),
    )
    .expect("修正设置文件");
    let status = host.select_workspace(&broken).expect("修正后应可切换");
    assert_eq!(status.path.as_deref(), Some(broken.canonicalize().unwrap().as_path()));
    assert_eq!(host.settings().hotkey, "Ctrl+Shift+Space");

    cleanup(&current);
    cleanup(&broken);
    cleanup(&device);
}

/// 用户手动编辑设置文件后重启，应用读取的是文件内容而不是内存状态。
#[test]
fn hand_edited_settings_file_is_read_on_restart() {
    let repo = real_git_repo("workspace-hand-edit");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");

    // 模拟用户用编辑器手工修改（原子替换，和真实编辑器一致）。
    let mut settings = host.settings();
    settings.hotkey = "Ctrl+Alt+K".to_string();
    settings.quick_access_limit = 9;
    fs::write(repo.join(SETTINGS_FILE), settings.to_toml().unwrap()).expect("手工编辑设置文件");

    let restarted = host_restarted(&device, Settings::default());
    assert_eq!(restarted.settings().hotkey, "Ctrl+Alt+K");
    assert_eq!(restarted.settings().quick_access_limit, 9);

    cleanup(&repo);
    cleanup(&device);
}

/// 设备本地数据（剪贴板历史、设备路径等）绝不进入配置工作区目录树。
#[test]
fn device_local_data_never_appears_in_the_workspace_tree() {
    const SENTINEL: &str = "设备本地哨兵-c1b1b6a9-不应出现在工作区";

    let repo = real_git_repo("workspace-device-local");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");
    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("保存设置");

    // 写入两类设备本地数据：本机路径与一条剪贴板式历史记录。
    host.device().put("clipboard.lastEntry", SENTINEL).expect("写入设备本地数据");
    let recorded = host.device().workspace_path().expect("读取工作区指针");
    assert_eq!(recorded.as_deref(), Some(repo.canonicalize().unwrap().as_path()));
    assert!(
        host.device().file().exists(),
        "设备本地状态文件必须落在应用数据目录：{}",
        host.device().file().display()
    );

    // 设备本地状态文件本身不能在工作区里。
    let device_file = host.device().file();
    let workspace_root = repo.canonicalize().unwrap();
    assert!(
        !device_file.starts_with(&workspace_root),
        "设备本地状态文件不得位于工作区内：{}",
        device_file.display()
    );

    // 工作区目录树中不得出现设备本地内容。
    let files = files_under(&workspace_root);
    assert!(!files.is_empty(), "工作区应有 settings.toml 与 .git 目录");
    for file in files {
        let bytes = fs::read(&file).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(SENTINEL),
            "设备本地内容出现在工作区文件里：{}",
            file.display()
        );
        assert!(
            !text.contains(&device_file.to_string_lossy().to_string()),
            "设备本地状态文件路径出现在工作区文件里：{}",
            file.display()
        );
    }

    // 工作区里也没有任何指向设备目录的符号链接。
    assert!(
        !workspace_root
            .join(flashcast_core::DEVICE_STATE_FILE)
            .exists(),
        "工作区不得包含设备本地状态文件"
    );
    assert_eq!(
        host.device().get(KEY_WORKSPACE_PATH).unwrap(),
        Some(repo.canonicalize().unwrap().to_string_lossy().into_owned()),
        "工作区路径只记录在设备本地存储里"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 未关联工作区时设置只在内存生效，并如实报告未落盘。
#[test]
fn settings_are_memory_only_until_a_workspace_is_linked() {
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    let status = host.workspace_status();
    assert!(status.path.is_none());
    assert!(!status.persisted, "未关联工作区时不得声称设置已落盘");
    assert!(!status.valid);

    host.update_settings(Settings {
        hotkey: "Super+Space".to_string(),
        ..fast_settings()
    })
    .expect("未关联工作区时仍可修改内存设置");
    assert_eq!(host.settings().hotkey, "Super+Space");

    cleanup(&device);
}

/// 路径不存在或不是目录时给出可读的中文原因，且不改变当前状态。
#[test]
fn selecting_a_missing_path_reports_a_chinese_reason() {
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    let missing = unique_dir("workspace-missing").join("不存在");

    let error = host.select_workspace(&missing).expect_err("不存在的目录必须拒绝");
    assert!(
        error.to_string().contains("目录不存在"),
        "失败原因：{error}"
    );
    assert!(host.workspace_status().path.is_none());

    let file = missing.parent().unwrap().join("a-file");
    fs::write(&file, "x").unwrap();
    let error = host.select_workspace(&file).expect_err("文件不是目录");
    assert!(error.to_string().contains("不是目录"), "失败原因：{error}");

    cleanup(&device);
    cleanup(missing.parent().unwrap());
}

/// 工作区可以是普通目录（尚未 `git init`）：Git 由后续 ticket 关联。
#[test]
fn a_plain_directory_can_be_selected_without_a_git_repository() {
    let dir = unique_dir("workspace-plain");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let status = host.select_workspace(&dir).expect("普通目录可以作为工作区");
    assert!(status.valid);
    assert!(status.git_dir.is_none(), "普通目录没有 gitdir：{status:?}");
    assert_eq!(status.path.as_deref(), Some(dir.canonicalize().unwrap().as_path()));

    cleanup(&dir);
    cleanup(&device);
}

/// 工作区状态里的设置文件路径必须是工作区内的真实路径。
#[test]
fn workspace_status_points_at_a_real_settings_file() {
    let repo = real_git_repo("workspace-status");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    let status = host.select_workspace(&repo).expect("关联工作区");

    let settings_file = status.settings_file.expect("应给出设置文件路径");
    assert!(settings_file.ends_with(SETTINGS_FILE));
    assert!(settings_file.starts_with(status.path.as_deref().unwrap()));
    assert!(Path::new(&settings_file).parent().unwrap().is_dir());

    cleanup(&repo);
    cleanup(&device);
}
