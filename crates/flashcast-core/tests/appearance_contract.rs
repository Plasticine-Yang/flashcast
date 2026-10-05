//! 插件兼容与外观偏好的宿主入口验证；真实文件和工作区，不测 UI。
mod support;
use flashcast_core::{Appearance, SurfaceRenderer, SurfaceStyle, ThemeAppearance, THEME_ARC};
use std::fs;
use support::{
    cleanup, fast_settings, host_restarted, host_with_device, real_git_repo, unique_dir,
};

fn paper() -> serde_json::Value {
    let mut document = flashcast_core::builtin_themes().remove(0);
    document.id = "example.paper".into();
    document.name = "纸面".into();
    document.styles = vec![SurfaceStyle {
        id: "paper".into(),
        name: "纸面".into(),
        ..SurfaceStyle::solid()
    }];
    document.default_style = Some("paper".into());
    serde_json::to_value(document).unwrap()
}

#[test]
fn theme_mode_style_and_reduction_are_independent_and_survive_restart() {
    let repo = real_git_repo("appearance-contract");
    let package = unique_dir("appearance-package").join("paper.json");
    fs::write(&package, paper().to_string()).unwrap();
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    host.set_appearance_preferences(ThemeAppearance::Dark, "liquid".into(), false)
        .unwrap();
    host.install_theme_package(&package).unwrap();
    let state = host.select_theme("example.paper").unwrap();
    assert_eq!(state.appearance, Appearance::Dark);
    assert_eq!(state.style, "paper");
    assert_eq!(state.renderer, SurfaceRenderer::Solid);
    assert_eq!(state.styles.len(), 1);
    let state = host.select_theme(THEME_ARC).unwrap();
    assert_eq!(state.style, "liquid");
    let state = host
        .set_appearance_preferences(ThemeAppearance::Light, "liquid".into(), true)
        .unwrap();
    assert_eq!(state.appearance, Appearance::Light);
    assert_eq!(state.style, "liquid");
    assert_eq!(state.renderer, SurfaceRenderer::Solid);
    assert_eq!(state.surface.fill_opacity, 1.0);
    let restarted = host_restarted(&device, fast_settings());
    let state = restarted.theme_state();
    assert_eq!(state.preference, ThemeAppearance::Light);
    assert!(state.reduce_transparency);
    assert_eq!(state.style, "liquid");
    let state = restarted
        .set_appearance_preferences(ThemeAppearance::System, "liquid".into(), false)
        .unwrap();
    assert_eq!(state.renderer, SurfaceRenderer::Liquid);
    assert_eq!(
        restarted.set_system_appearance(Appearance::Dark).appearance,
        Appearance::Dark
    );
    cleanup(&repo);
    cleanup(package.parent().unwrap());
    cleanup(&device);
}

#[test]
fn preferences_without_a_workspace_are_saved_on_this_device() {
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.set_appearance_preferences(ThemeAppearance::Dark, "liquid".into(), true)
        .unwrap();
    let state = host_restarted(&device, fast_settings()).theme_state();
    assert_eq!(state.appearance, Appearance::Dark);
    assert_eq!(state.style, "liquid");
    assert!(state.reduce_transparency);
    cleanup(&device);
}

#[test]
fn invalid_preferences_and_failed_writes_preserve_the_complete_appearance() {
    let repo = real_git_repo("appearance-invalid");
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    let before = host
        .set_appearance_preferences(ThemeAppearance::Dark, "liquid".into(), false)
        .unwrap();
    assert!(host
        .set_appearance_preferences(ThemeAppearance::Light, "unknown".into(), true)
        .is_err());
    assert_eq!(host.theme_state(), before);
    let path = repo.join("theme.json");
    fs::write(&path, r#"{"selected":"flashcast.theme.arc","appearance":"light","styles":{"flashcast.theme.arc":"missing"},"reduceTransparency":true}"#).unwrap();
    assert!(host.reload_workspace().error.is_some());
    let state = host.theme_state();
    assert_eq!(state.tokens, before.tokens);
    assert_eq!(state.surface, before.surface);
    assert_eq!(state.style, before.style);
    // 将目标变成目录，模拟无法原子替换；不能先改内存。
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(host
        .set_appearance_preferences(ThemeAppearance::Light, "solid".into(), true)
        .is_err());
    assert_eq!(host.theme_state().tokens, before.tokens);
    cleanup(&repo);
    cleanup(&device);
}

#[test]
fn new_packages_must_supply_both_modes_and_a_supported_contract() {
    let repo = real_git_repo("appearance-package-validation");
    let folder = unique_dir("appearance-bad-packages");
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    let before = host.theme_state();
    for variant in [
        "missing-dark",
        "unknown-renderer",
        "incompatible-host",
        "api",
        "no-contract",
        "legacy",
    ] {
        let mut value = paper();
        match variant {
            "missing-dark" => {
                value["palettes"].as_object_mut().unwrap().remove("dark");
            }
            "unknown-renderer" => value["styles"][0]["renderer"] = "unimplemented".into(),
            "incompatible-host" => value["contract"]["hostVersion"] = ">=9.0.0".into(),
            "api" => value["contract"]["apiVersion"] = 99.into(),
            "no-contract" => {
                value.as_object_mut().unwrap().remove("contract");
            }
            "legacy" => {
                value = serde_json::json!({"schemaVersion":1,"id":"example.legacy","name":"旧主题","version":"1.0.0","appearance":"dark","tokens":flashcast_core::dark_tokens()});
            }
            _ => unreachable!(),
        }
        let path = folder.join(format!("{variant}.json"));
        fs::write(&path, value.to_string()).unwrap();
        assert!(
            host.install_theme_package(&path).is_err(),
            "应拒绝 {variant}"
        );
        assert_eq!(host.theme_state(), before, "失败不能改变外观：{variant}");
        assert!(!repo.join("themes/example.paper/theme.json").exists());
    }
    cleanup(&repo);
    cleanup(&folder);
    cleanup(&device);
}

#[test]
fn external_preferences_and_same_version_package_edits_are_reloaded() {
    let repo = real_git_repo("appearance-external");
    let folder = unique_dir("appearance-external-package");
    let path = folder.join("paper.json");
    fs::write(&path, paper().to_string()).unwrap();
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    host.install_theme_package(&path).unwrap();
    host.select_theme("example.paper").unwrap();
    let mut value = paper();
    value["palettes"]["light"]["color"]["surface"] = "#e1e7ef".into();
    fs::write(
        repo.join("themes/example.paper/theme.json"),
        value.to_string(),
    )
    .unwrap();
    assert!(host.reload_workspace().applied);
    assert_eq!(host.theme_state().tokens.color.surface, "#e1e7ef");
    fs::write(
        repo.join("theme.json"),
        r#"{"selected":"example.paper","appearance":"dark","styles":{"example.paper":"paper"}}"#,
    )
    .unwrap();
    assert!(host.reload_workspace().applied);
    assert_eq!(host.theme_state().appearance, Appearance::Dark);
    cleanup(&repo);
    cleanup(&folder);
    cleanup(&device);
}

#[test]
fn incompatible_feature_plugins_never_contribute_to_host_search() {
    use flashcast_core::PluginRegistry;
    use std::sync::Arc;
    use support::{host_with_plugins_device, item, StaticPlugin};
    let registry = Arc::new(PluginRegistry::new());
    let mut rejected = StaticPlugin::new(
        "example.rejected",
        vec![item("bad", "Bad", "example.rejected", 100)],
    );
    rejected.manifest.contract.api_version = 999;
    assert!(registry.try_register(Arc::new(rejected)).is_err());
    let accepted = StaticPlugin::new(
        "example.accepted",
        vec![item("good", "Good", "example.accepted", 100)],
    );
    registry.try_register(Arc::new(accepted)).unwrap();
    let (host, _, device) = host_with_plugins_device(vec![], fast_settings(), registry);
    let response = host.query("good");
    assert!(response.items.iter().any(|i| i.id == "good"));
    assert!(!response.items.iter().any(|i| i.id == "bad"));
    cleanup(&device);
}

#[test]
fn failed_theme_upgrade_restores_original_package_and_appearance() {
    let repo = real_git_repo("appearance-upgrade");
    let package = unique_dir("appearance-upgrade-package").join("paper.json");
    fs::write(&package, paper().to_string()).unwrap();
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    host.install_theme_package(&package).unwrap();
    let before = host.select_theme("example.paper").unwrap();
    let installed = repo.join("themes/example.paper/theme.json");
    let bytes = fs::read(&installed).unwrap();
    let manifest = repo.join("manifest.json");
    fs::remove_file(&manifest).unwrap();
    fs::create_dir(&manifest).unwrap();
    let mut upgraded = paper();
    upgraded["version"] = "1.1.0".into();
    fs::write(&package, upgraded.to_string()).unwrap();
    assert!(host.install_theme_package(&package).is_err());
    assert_eq!(fs::read(&installed).unwrap(), bytes);
    assert_eq!(host.theme_state().tokens, before.tokens);
    assert!(host.set_plugin_enabled(THEME_ARC, false).is_err());
    cleanup(&repo);
    cleanup(package.parent().unwrap());
    cleanup(&device);
}

#[test]
fn transparent_readability_and_saved_style_ids_are_validated() {
    let mut arc = serde_json::to_value(flashcast_core::builtin_themes().remove(0)).unwrap();
    arc["styles"][0]["dark"]["fillOpacity"] = 0.65.into();
    assert!(flashcast_core::ThemeDocument::from_json(&arc.to_string()).is_err());
    let repo = real_git_repo("appearance-style-upgrade");
    let package = unique_dir("appearance-style-package").join("paper.json");
    fs::write(&package, paper().to_string()).unwrap();
    let (host, _, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).unwrap();
    host.install_theme_package(&package).unwrap();
    host.select_theme("example.paper").unwrap();
    let before = host
        .set_appearance_preferences(ThemeAppearance::Dark, "paper".into(), false)
        .unwrap();
    let mut update = paper();
    update["styles"][0]["id"] = "renamed".into();
    update["defaultStyle"] = "renamed".into();
    fs::write(&package, update.to_string()).unwrap();
    assert!(host.install_theme_package(&package).is_err());
    assert_eq!(host.theme_state(), before);
    cleanup(&repo);
    cleanup(package.parent().unwrap());
    cleanup(&device);
}
