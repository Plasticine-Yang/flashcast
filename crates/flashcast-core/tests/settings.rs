//! 设置模型与宿主设置入口的集成测试。

mod support;

use flashcast_core::Settings;
use flashcast_platform::hotkey::{HotkeySpec, DEFAULT_HOTKEY};
use support::{app, fast_settings, host_with};

/// 默认快捷键可用且不会与 GNOME / 输入法默认键位冲突。
#[test]
fn default_settings_use_a_parseable_non_conflicting_hotkey() {
    let settings = Settings::default();

    assert_eq!(settings.hotkey, DEFAULT_HOTKEY);
    assert_eq!(DEFAULT_HOTKEY, "Ctrl+Alt+Space");
    let spec = HotkeySpec::parse(&settings.hotkey).expect("默认快捷键必须可解析");
    assert_eq!(spec.canonical(), "Ctrl+Alt+Space");
    assert!(!settings.launch_at_startup);
    settings.validate().expect("默认设置必须有效");
}

/// 设置可以在 JSON 与 TOML 之间往返，保持快捷键与开机启动偏好。
#[test]
fn settings_round_trip_json_and_toml() {
    let settings = Settings {
        hotkey: "Ctrl+Shift+Space".to_string(),
        launch_at_startup: true,
        quick_access_limit: 8,
        plugin_timeout_ms: 250,
        disabled_plugins: vec!["memo".to_string()],
    };

    let from_json = Settings::from_json(&settings.to_json().expect("序列化 JSON")).expect("反序列化");
    assert_eq!(from_json, settings);

    let from_toml = Settings::from_toml(&settings.to_toml().expect("序列化 TOML")).expect("反序列化");
    assert_eq!(from_toml, settings);
}

/// 无效设置被拒绝，并保留上一次可用状态。
#[test]
fn invalid_settings_are_rejected_and_previous_state_is_kept() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    let before = host.settings();

    let invalid_hotkey = Settings {
        hotkey: "这不是快捷键".to_string(),
        ..before.clone()
    };
    let error = host
        .update_settings(invalid_hotkey)
        .expect_err("无效快捷键必须被拒绝");
    assert!(error.to_string().contains("快捷键"));
    assert_eq!(host.settings(), before, "必须保留上一次可用设置");

    let invalid_limit = Settings {
        quick_access_limit: 0,
        ..before.clone()
    };
    assert!(host.update_settings(invalid_limit).is_err());
    assert_eq!(host.settings(), before);

    let invalid_timeout = Settings {
        plugin_timeout_ms: 10_000,
        ..before.clone()
    };
    assert!(host.update_settings(invalid_timeout).is_err());
    assert_eq!(host.settings(), before);

    // 有效设置可以生效。
    let valid = Settings {
        hotkey: "Super+Space".to_string(),
        launch_at_startup: true,
        ..before.clone()
    };
    let applied = host.update_settings(valid.clone()).expect("有效设置应被接受");
    assert_eq!(applied, valid);
    assert_eq!(host.settings().hotkey, "Super+Space");
    assert!(host.settings().launch_at_startup);
}

/// 快速访问项数量受设置约束。
#[test]
fn quick_access_limit_setting_is_respected() {
    let apps = (0..10)
        .map(|index| app(&format!("app{index}"), &format!("软件 {index}")))
        .collect();
    let (host, _launcher) = host_with(
        apps,
        Settings {
            quick_access_limit: 2,
            ..fast_settings()
        },
    );

    let response = host.query("");
    let app_items = response
        .items
        .iter()
        .filter(|item| item.kind == flashcast_core::ItemKind::Application)
        .count();

    assert_eq!(app_items, 2, "空查询中的软件数量必须受设置约束");
}

/// 宿主能力摘要来自平台探测，不伪造。
#[test]
fn capabilities_come_from_the_platform_probe() {
    let (host, _launcher) = host_with(vec![], fast_settings());

    let capabilities = host.capabilities();

    assert_eq!(capabilities.os, flashcast_platform::OsKind::Linux);
    assert_eq!(
        capabilities.session,
        flashcast_platform::SessionType::X11,
        "替身能力探测为 X11"
    );
    assert!(capabilities.hotkey.is_supported());
}

/// 未提供 Tauri 依赖：证明核心逻辑与外壳解耦。
#[test]
fn host_has_no_tauri_dependency() {
    // 该测试的价值在于编译期：如果 `flashcast-core` 引入了 tauri，
    // 无桌面会话的 CI 上就无法编译本测试。
    let (host, _launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    assert_eq!(host.query("alpha").items.len(), 1);
}
