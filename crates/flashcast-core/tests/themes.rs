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
    ThemeSelection, MANIFEST_FILE, THEMES_DIR, THEME_DARK, THEME_FILE, THEME_LIGHT, THEME_SYSTEM,
};
use support::{
    cleanup, fast_settings, host_restarted, host_with_device, real_git_repo, unique_dir,
};

/// 一个完整的自定义主题文档（深色外观，可安装）。
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

/// 每个内置主题的 token 集合都完整，并且完整映射为 CSS 自定义属性。
///
/// 「UI 以 CSS 自定义属性消费」的前提是映射没有缺口：任何语义 token 没有对应的
/// 变量，都会让某个界面元素退回默认值，主题切换就只生效一半。
#[test]
fn every_builtin_theme_exposes_a_complete_token_set() {
    use std::collections::HashSet;

    let required = [
        "--fc-page-bg",
        "--fc-surface",
        "--fc-hover-bg",
        "--fc-border",
        "--fc-border-strong",
        "--fc-text",
        "--fc-text-muted",
        "--fc-text-disabled",
        "--fc-accent",
        "--fc-info-bg",
        "--fc-warning-bg",
        "--fc-warning-border",
        "--fc-warning-text",
        "--fc-icon-fallback-bg",
        "--fc-selection-bg",
        "--fc-selection-border",
        "--fc-focus-ring",
        "--fc-error-bg",
        "--fc-error-border",
        "--fc-error-text",
        "--fc-disabled-opacity",
        "--fc-font-family",
        "--fc-font-body",
        "--fc-font-input",
        "--fc-font-aux",
        "--fc-space-window-padding",
        "--fc-space-row-padding",
        "--fc-space-row-gap",
        "--fc-space-section-gap",
        "--fc-row-height",
        "--fc-radius-window",
        "--fc-radius-item",
        "--fc-radius-control",
        "--fc-shadow",
        "--fc-shadow-overlay",
    ];

    for document in flashcast_core::builtin_themes() {
        for system in [Appearance::Light, Appearance::Dark] {
            let tokens = document
                .resolve(system)
                .unwrap_or_else(|error| panic!("{} 在 {system:?} 下必须可解析：{error}", document.id));
            tokens
                .validate()
                .unwrap_or_else(|error| panic!("{} 的 token 必须通过校验：{error}", document.id));
            assert_eq!(
                tokens.css_vars().len(),
                required.len(),
                "{} 的 CSS 变量数量与语义 token 不匹配",
                document.id
            );
        }

        let tokens = document.resolve(Appearance::Light).expect("可解析");
        let vars = tokens.css_vars();
        let names: HashSet<&str> = vars.iter().map(|var| var.name.as_str()).collect();
        assert_eq!(
            names.len(),
            vars.len(),
            "{} 的 CSS 变量名不得重复",
            document.id
        );
        for name in &names {
            assert!(name.starts_with("--fc-"), "变量名必须以 --fc- 开头：{name}");
        }
        for var in &vars {
            assert!(
                !var.value.trim().is_empty(),
                "{} 的 {var:?} 不能是空值",
                document.id
            );
        }
        for name in required {
            assert!(
                names.contains(name),
                "{} 缺少 CSS 变量 {name}",
                document.id
            );
        }
    }

    // token 的字段清单与语义集合一一对应：没有字段被映射遗漏。
    assert_eq!(
        flashcast_core::ThemeTokens::field_names().len(),
        36,
        "语义 token 字段数变化时必须同步更新本用例与 css_vars 映射"
    );
}

/// 主题数据里不允许出现动画 / 过渡语义：高频键盘操作在任何主题下都不得有动画。
#[test]
fn theme_tokens_carry_no_animation_semantics() {
    for document in flashcast_core::builtin_themes() {
        let json = serde_json::to_value(&document).expect("序列化主题文档");
        let text = json.to_string().to_ascii_lowercase();
        for forbidden in [
            "anim",
            "transition",
            "duration",
            "delay",
            "keyframe",
            "motion",
        ] {
            assert!(
                !text.contains(forbidden),
                "主题 {} 里出现动画相关字段：{forbidden}",
                document.id
            );
        }
    }

    // 未知字段会被拒绝：主题无法偷偷塞进动画字段。
    let mut json = serde_json::to_value(flashcast_core::light_tokens()).expect("序列化 token");
    json["state"]["selected"]["transition"] = serde_json::json!("all 300ms");
    let error =
        serde_json::from_value::<flashcast_core::ThemeTokens>(json).expect_err("未知字段必须被拒绝");
    assert!(error.to_string().contains("transition"), "{error}");
}

/// 「跟随系统」在运行时跟着系统外观切换：不需要重启，也不改变主题选择。
#[test]
fn follow_system_theme_reacts_to_system_appearance_changes() {
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_theme(THEME_SYSTEM).expect("选择跟随系统");

    let light = host.theme_state();
    assert_eq!(light.preference, ThemeAppearance::System);
    assert_eq!(light.appearance, Appearance::Light);
    assert_eq!(light.system_appearance, Appearance::Light);

    // 系统切到深色：同一个主题立即给出深色 token。
    let dark = host.set_system_appearance(Appearance::Dark);
    assert_eq!(dark.selected, THEME_SYSTEM, "系统外观变化不得改变主题选择");
    assert_eq!(dark.appearance, Appearance::Dark);
    assert_eq!(dark.system_appearance, Appearance::Dark);
    assert_eq!(dark.tokens.color.surface, "#202226");
    assert_ne!(dark.css_vars, light.css_vars, "CSS 变量必须真的换了");

    // 系统切回浅色：回到最初的 token 集合。
    let back = host.set_system_appearance(Appearance::Light);
    assert_eq!(back.appearance, Appearance::Light);
    assert_eq!(back.tokens, light.tokens);

    // 固定外观的主题不受系统外观影响。
    host.select_theme(THEME_DARK).expect("选择深色");
    let fixed = host.set_system_appearance(Appearance::Light);
    assert_eq!(fixed.appearance, Appearance::Dark);
    assert_eq!(fixed.tokens.color.surface, "#202226");

    cleanup(&device);
}

/// 无效 / 读不到的主题保留上一次可用外观，并给出可读的中文原因。
#[test]
fn an_unreadable_theme_keeps_the_last_usable_appearance() {
    let repo = real_git_repo("theme-invalid");
    let packages = unique_dir("theme-packages-invalid");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    let package = write_theme_package(&packages, "example.solarized", "Solarized 深色");
    host.install_theme_package(&package).expect("安装主题包");
    host.select_theme("example.solarized")
        .expect("选择已安装主题");
    let good = host.theme_state();
    assert_eq!(good.tokens.color.surface, "#101418");

    // 外部把主题包文件写坏：保留可用外观，说明原因，主题在列表里标记为不可用。
    let installed = repo
        .join(THEMES_DIR)
        .join("example.solarized")
        .join(THEME_FILE);
    fs::write(&installed, "{ 坏掉的 JSON").expect("写坏主题包");
    let reload = host.reload_workspace();
    // 主题列表确实变了（该主题被标记为不可用），但生效外观不得改变。
    assert!(
        reload.error.is_some(),
        "读不到的主题必须在重载结果里给出原因：{reload:?}"
    );
    let state = host.theme_state();
    assert_eq!(state.selected, "example.solarized", "主题选择不得被回退");
    assert_eq!(state.tokens, good.tokens, "必须保留上一次可用外观");
    assert_eq!(
        state.css_vars, good.css_vars,
        "下发给 UI 的 CSS 属性也必须是上一次可用的"
    );
    let message = state.error.expect("必须给出原因");
    assert!(
        message.contains("主题无效") || message.contains("JSON"),
        "原因必须可读：{message}"
    );
    let entry = state
        .themes
        .iter()
        .find(|theme| theme.id == "example.solarized")
        .expect("列表里仍有它");
    assert!(!entry.usable, "损坏的主题必须标记为不可用");
    assert!(entry.error.is_some());

    // 修好文件后恢复，错误消失。
    fs::write(
        &installed,
        custom_theme_json("example.solarized", "Solarized 深色"),
    )
    .expect("修好主题包");
    let reload = host.reload_workspace();
    assert!(reload.applied, "修好后必须重新生效：{reload:?}");
    assert_eq!(host.theme_state().error, None);
    assert_eq!(host.theme_state().tokens.color.surface, "#101418");

    // 主题配置指向一个不存在的主题：保留当前外观并说明原因。
    fs::write(
        repo.join(THEME_FILE),
        ThemeSelection::new("ghost.theme").to_json().unwrap(),
    )
    .expect("写出无效的主题配置");
    let reload = host.reload_workspace();
    assert!(!reload.applied);
    assert_eq!(host.theme_state().selected, "example.solarized");
    assert!(
        reload
            .error
            .as_deref()
            .is_some_and(|message| message.contains("不可用")),
        "原因必须可读：{:?}",
        reload.error
    );

    cleanup(&repo);
    cleanup(&packages);
    cleanup(&device);
}

/// 选中的主题跟随配置工作区：切换工作区后按目标工作区恢复，切回来也恢复。
#[test]
fn the_selected_theme_follows_the_config_workspace() {
    let first = real_git_repo("theme-ws-first");
    let second = real_git_repo("theme-ws-second");
    let packages = unique_dir("theme-packages-ws");
    let package = write_theme_package(&packages, "example.solarized", "Solarized 深色");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    host.select_workspace(&first).expect("关联第一个工作区");
    host.select_theme(THEME_DARK).expect("第一个工作区用深色");
    assert_eq!(host.theme_state().appearance, Appearance::Dark);

    // 第二个工作区：安装并选择一个本地主题包。
    host.select_workspace(&second).expect("切换到第二个工作区");
    host.install_theme_package(&package)
        .expect("在第二个工作区安装主题包");
    host.select_theme("example.solarized")
        .expect("在第二个工作区选择已安装主题");
    assert_eq!(host.theme_state().tokens.color.surface, "#101418");

    // 切回第一个：恢复深色，并且看不到第二个工作区安装的主题。
    host.select_workspace(&first).expect("切回第一个工作区");
    let state = host.theme_state();
    assert_eq!(state.selected, THEME_DARK, "必须恢复该工作区选中的主题");
    assert_eq!(state.tokens.color.surface, "#202226");
    assert!(
        state
            .themes
            .iter()
            .all(|theme| theme.id != "example.solarized"),
        "不得残留另一个工作区安装的主题：{:?}",
        state.themes.iter().map(|theme| &theme.id).collect::<Vec<_>>()
    );

    // 再切到第二个：恢复本地主题包（清单与主题包都来自那个工作区）。
    host.select_workspace(&second).expect("再切到第二个工作区");
    let state = host.theme_state();
    assert_eq!(state.selected, "example.solarized");
    assert_eq!(state.tokens.color.surface, "#101418");
    assert!(
        state
            .themes
            .iter()
            .any(|theme| theme.id == "example.solarized" && theme.usable),
        "第二个工作区的主题包必须重新可用"
    );

    // 重启后仍然停在第二个工作区的选择。
    let restarted = host_restarted(&device, fast_settings());
    assert_eq!(restarted.theme_state().selected, "example.solarized");

    cleanup(&first);
    cleanup(&second);
    cleanup(&packages);
    cleanup(&device);
}

/// 本地主题包：校验失败给出中文原因，有效则可安装、选择，并可移除。
#[test]
fn a_local_theme_package_is_validated_installed_selected_and_removed() {
    let repo = real_git_repo("theme-install");
    let packages = unique_dir("theme-packages");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());
    host.select_workspace(&repo).expect("关联工作区");

    // 无效主题包：可读原因、不改动清单、不改动当前外观。
    let broken = packages.join("broken");
    fs::create_dir_all(&broken).expect("创建损坏主题包目录");
    fs::write(broken.join(THEME_FILE), "{ 这不是 JSON").expect("写出损坏主题包");
    let message = host
        .install_theme_package(&broken)
        .expect_err("无效主题包必须被拒绝")
        .to_string();
    assert!(
        message.contains("主题无效") && message.contains("JSON"),
        "原因必须可读：{message}"
    );
    assert_eq!(host.theme_state().selected, THEME_LIGHT);
    assert!(
        host.manifest_entries()
            .iter()
            .all(|entry| entry.id != "example.solarized"),
        "被拒绝的主题包不得写进清单"
    );

    let missing = host
        .install_theme_package(&packages.join("not-there"))
        .expect_err("不存在的路径必须被拒绝");
    assert!(
        missing.to_string().contains("主题包不存在"),
        "原因必须可读：{missing}"
    );

    // 有效主题包：安装进工作区的 themes/<id>/theme.json。
    let package = write_theme_package(&packages, "example.solarized", "Solarized 深色");
    let state = host.install_theme_package(&package).expect("安装主题包");
    let entry = state
        .themes
        .iter()
        .find(|theme| theme.id == "example.solarized")
        .expect("安装后必须出现在主题列表里");
    assert!(!entry.builtin, "安装的主题不是内置主题");
    assert!(entry.enabled && entry.usable);
    assert_eq!(entry.version, "2.1.0");
    let installed_file = repo
        .join(THEMES_DIR)
        .join("example.solarized")
        .join(THEME_FILE);
    assert!(
        installed_file.exists(),
        "主题包必须落在工作区里：{}",
        installed_file.display()
    );
    let file =
        PluginManifestFile::from_json(&fs::read_to_string(repo.join(MANIFEST_FILE)).unwrap())
            .expect("读取清单");
    let recorded = file.get("example.solarized").expect("清单里必须有它");
    assert_eq!(recorded.kind, PluginKind::Theme);
    assert_eq!(recorded.origin, PluginOrigin::Installed);

    // 选择它：外观按主题包的数据改变。
    let selected = host
        .select_theme("example.solarized")
        .expect("选择已安装主题");
    assert_eq!(selected.appearance, Appearance::Dark);
    assert_eq!(selected.tokens.color.surface, "#101418");

    // 重启后仍然选中，并且主题包还在。
    let restarted = host_restarted(&device, fast_settings());
    assert_eq!(restarted.theme_state().selected, "example.solarized");
    assert_eq!(restarted.theme_state().tokens.color.surface, "#101418");

    // 内置主题不能移除。
    let error = host
        .remove_theme(THEME_LIGHT)
        .expect_err("内置主题不得移除");
    assert!(
        error.to_string().contains("内置主题不能移除"),
        "原因必须可读：{error}"
    );

    // 移除已安装主题：文件与清单条目一起消失，选中回退并说明原因。
    let removed = host.remove_theme("example.solarized").expect("移除主题包");
    assert_eq!(removed.selected, THEME_LIGHT, "移除选中主题后必须回退");
    assert!(
        removed.error.is_some(),
        "回退必须给出中文原因：{:?}",
        removed.error
    );
    assert!(!repo.join(THEMES_DIR).join("example.solarized").exists());
    let file =
        PluginManifestFile::from_json(&fs::read_to_string(repo.join(MANIFEST_FILE)).unwrap())
            .expect("读取清单");
    assert!(file.get("example.solarized").is_none());

    let restarted = host_restarted(&device, fast_settings());
    assert!(
        restarted
            .theme_state()
            .themes
            .iter()
            .all(|theme| theme.id != "example.solarized"),
        "移除后重启不得再出现"
    );

    cleanup(&repo);
    cleanup(&packages);
    cleanup(&device);
}

/// 未关联配置工作区时不能安装主题包，并给出可读原因。
#[test]
fn installing_a_theme_package_requires_a_workspace() {
    let packages = unique_dir("theme-packages-noworkspace");
    let package = write_theme_package(&packages, "example.solarized", "Solarized 深色");
    let (host, _launcher, device) = host_with_device(vec![], fast_settings());

    let error = host
        .install_theme_package(&package)
        .expect_err("没有工作区时不得安装");
    assert!(
        error.to_string().contains("配置工作区"),
        "原因必须可读：{error}"
    );

    cleanup(&packages);
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
