//! 主题插件与插件清单的集成测试（ticket 06）。
//!
//! 全部经由宿主入口（`Host::theme_state` / `select_theme` / `install_theme_package`
//! / `remove_theme` / `set_plugin_enabled` / `reload_workspace` 等）与真实临时目录，
//! 不直接调用模块内部函数（ADR §10）。

mod support;

use std::fs;
use std::path::PathBuf;

use flashcast_core::{
    Appearance, ManifestEntry, PluginKind, PluginManifestFile, PluginOrigin, ThemeAppearance,
    ThemeSelection, MANIFEST_FILE, THEME_DARK, THEME_FILE, THEME_LIGHT, THEME_SYSTEM,
};
use support::{cleanup, fast_settings, host_restarted, host_with_device, real_git_repo};

/// 一个完整的自定义主题文档（深色外观，可安装）。
#[allow(dead_code)]
fn custom_theme_json(id: &str, name: &str) -> String {
    let mut tokens: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&flashcast_core::dark_tokens()).unwrap())
            .unwrap();
    // 换个颜色，便于断言「外观确实变了」。
    tokens["color"]["surface"] = serde_json::json!("#101418");
    tokens["color"]["pageBackground"] = serde_json::json!("rgba(12, 16, 20, 0.98)");
    serde_json::json!({
        "schemaVersion": 1,
        "id": id,
        "name": name,
        "version": "2.1.0",
        "appearance": "dark",
        "tokens": tokens,
    })
    .to_string()
}

/// 写出一个主题包目录，返回目录路径。
#[allow(dead_code)]
fn write_theme_package(parent: &PathBuf, id: &str, name: &str) -> PathBuf {
    let dir = parent.join(format!("{id}.package"));
    fs::create_dir_all(&dir).expect("创建主题包目录");
    fs::write(dir.join(THEME_FILE), custom_theme_json(id, name)).expect("写出主题包文档");
    dir
}

/// 默认清单里就有三个默认主题：浅色、深色与跟随系统。
#[test]
fn the_default_manifest_carries_the_three_default_themes() {
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let entries = host.manifest_entries();
    let themes: Vec<&ManifestEntry> = entries.iter().filter(|entry| entry.is_theme()).collect();
    assert_eq!(themes.len(), 3, "默认应有三个主题插件：{themes:?}");
    let ids: Vec<&str> = themes.iter().map(|entry| entry.id.as_str()).collect();
    assert_eq!(ids, vec![THEME_LIGHT, THEME_DARK, THEME_SYSTEM]);
    for entry in &themes {
        assert_eq!(entry.kind, PluginKind::Theme);
        assert_eq!(entry.version, "0.1.0");
        assert!(entry.enabled, "默认主题默认启用：{entry:?}");
        assert!(entry.origin.removable() == false, "内置主题不可移除");
    }

    let state = host.theme_state();
    assert_eq!(state.selected, THEME_LIGHT);
    assert_eq!(state.appearance, Appearance::Light);
    assert_eq!(state.themes.len(), 3);
    assert!(
        state.themes.iter().all(|theme| theme.usable && theme.error.is_none()),
        "默认主题都应可用：{:?}",
        state.themes
    );
    assert!(state.error.is_none());

    cleanup(&device);
}

/// 清单与主题配置写进工作区，重启后启用状态与选中主题都能恢复。
#[test]
fn manifest_and_theme_selection_round_trip_across_a_restart() {
    let repo = real_git_repo("theme-round-trip");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    // 关联工作区后就应写出人类可读的清单与主题配置。
    let text = fs::read_to_string(repo.join(MANIFEST_FILE)).expect("必须写出插件清单");
    let file = PluginManifestFile::from_json(&text).expect("清单必须是合法 JSON");
    assert_eq!(file.entries().len(), 3);
    let selection =
        ThemeSelection::from_json(&fs::read_to_string(repo.join(THEME_FILE)).expect("主题配置"))
            .expect("主题配置必须是合法 JSON");
    assert_eq!(selection.selected, THEME_LIGHT);

    host.select_theme(THEME_DARK).expect("选择深色主题");
    assert_eq!(host.theme_state().selected, THEME_DARK);
    assert_eq!(host.theme_state().appearance, Appearance::Dark);

    let restarted = host_restarted(&device, fast_settings());
    assert_eq!(
        restarted.theme_state().selected,
        THEME_DARK,
        "重启后必须恢复选中的主题"
    );
    assert_eq!(restarted.theme_state().appearance, Appearance::Dark);
    assert_eq!(restarted.theme_state().error, None);

    // 启停状态也随清单持久化。
    restarted
        .set_plugin_enabled(THEME_SYSTEM, false)
        .expect("停用跟随系统");
    let text = fs::read_to_string(repo.join(MANIFEST_FILE)).expect("读取清单");
    let file = PluginManifestFile::from_json(&text).expect("清单仍然合法");
    assert!(!file.get(THEME_SYSTEM).expect("跟随系统条目").enabled);

    let again = host_restarted(&device, fast_settings());
    assert!(
        !again
            .theme_state()
            .themes
            .iter()
            .find(|theme| theme.id == THEME_SYSTEM)
            .expect("跟随系统条目")
            .enabled,
        "停用状态必须在重启后保留"
    );

    cleanup(&repo);
    cleanup(&device);
}

/// 停用的主题不能被选择：给出可读的中文原因，并且不改变当前外观。
#[test]
fn a_disabled_theme_cannot_be_selected_and_keeps_the_current_appearance() {
    let repo = real_git_repo("theme-disabled");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    host.set_plugin_enabled(THEME_DARK, false).expect("停用深色");
    let error = host
        .select_theme(THEME_DARK)
        .expect_err("停用的主题必须被拒绝");
    let message = error.to_string();
    assert!(
        message.contains("深色") && message.contains("停用"),
        "原因必须可读：{message}"
    );
    let state = host.theme_state();
    assert_eq!(state.selected, THEME_LIGHT, "被拒绝后不得切换主题");
    assert_eq!(state.appearance, Appearance::Light);

    // 重新启用后可以正常选择。
    host.set_plugin_enabled(THEME_DARK, true).expect("启用深色");
    host.select_theme(THEME_DARK).expect("启用后可以选择");
    assert_eq!(host.theme_state().appearance, Appearance::Dark);

    cleanup(&repo);
    cleanup(&device);
}

/// 主题由清单驱动：清单里指向一个没有文档的主题条目时，它不可用也不能被选择。
///
/// 这条用例是「默认主题经清单加载，而不是按 id 硬编码」的可观测证据：主题的可选性
/// 完全由清单条目决定。
#[test]
fn theme_availability_follows_the_manifest() {
    let repo = real_git_repo("theme-manifest-driven");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    // 手工往清单里加一个指向不存在主题包的条目（模拟用户/同步带来的配置）。
    let path = repo.join(MANIFEST_FILE);
    let mut file =
        PluginManifestFile::from_json(&fs::read_to_string(&path).unwrap()).expect("读取清单");
    file.upsert(ManifestEntry {
        id: "example.missing".to_string(),
        name: "缺失的主题包".to_string(),
        kind: PluginKind::Theme,
        version: "1.0.0".to_string(),
        enabled: true,
        keywords: Vec::new(),
        capabilities: Vec::new(),
        origin: PluginOrigin::Installed,
        appearance: Some(ThemeAppearance::Dark),
    });
    fs::write(&path, file.to_json().unwrap()).expect("写出清单");

    let restarted = host_restarted(&device, fast_settings());
    let state = restarted.theme_state();
    let missing = state
        .themes
        .iter()
        .find(|theme| theme.id == "example.missing")
        .expect("清单里的条目必须出现在主题列表里");
    assert!(!missing.usable, "没有文档的主题不可用");
    assert!(
        missing
            .error
            .as_deref()
            .is_some_and(|reason| reason.contains("主题包文件不存在")),
        "必须说明原因：{:?}",
        missing.error
    );

    let error = restarted
        .select_theme("example.missing")
        .expect_err("缺失的主题包必须被拒绝");
    assert!(
        error.to_string().contains("主题包文件不存在"),
        "原因必须可读：{error}"
    );
    assert_eq!(restarted.theme_state().selected, THEME_LIGHT);

    // 没有关联工作区时，未登记的主题同样不可选择。
    let (fresh, _launcher2, device2) = host_with_device(vec![], fast_settings());
    assert!(fresh.select_theme("example.missing").is_err());

    cleanup(&repo);
    cleanup(&device);
    cleanup(&device2);
}
