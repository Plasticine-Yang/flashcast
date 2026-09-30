//! 配置工作区的集成测试：真实临时目录、真实 `git2` 仓库、真实文件读写。
//!
//! 全部经由 `Host` 的工作区与设置入口验证，不直接调用内部辅助函数（ADR §3 / §10）。

mod support;

use std::fs;

use flashcast_core::{Settings, SETTINGS_FILE};
use support::{cleanup, fast_settings, host_restarted, host_with_device, real_git_repo};

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
