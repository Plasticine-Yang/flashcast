//! 查询入口的集成测试：全部经由 `Host::query`（ADR §10）。

mod support;

use flashcast_core::QueryScope;
use support::{app, app_rich, fast_settings, host_with};

/// 按软件名称可以找到软件。
#[test]
fn search_by_application_name_finds_the_application() {
    let (host, _launcher) = host_with(
        vec![
            app("firefox", "Firefox"),
            app("nautilus", "文件"),
            app("gimp", "GIMP 图像编辑器"),
        ],
        fast_settings(),
    );

    let response = host.query("fire");

    assert_eq!(response.items.len(), 1, "只应命中 Firefox");
    assert_eq!(response.items[0].title, "Firefox");
    assert_eq!(response.items[0].id, "app:firefox.desktop");
    assert_eq!(response.scope, QueryScope::Home);
    assert_eq!(response.input, "fire");
}

/// 中文名称同样可以搜索。
#[test]
fn search_by_chinese_application_name_finds_the_application() {
    let (host, _launcher) = host_with(
        vec![app("nautilus", "文件"), app("gimp", "GIMP 图像编辑器")],
        fast_settings(),
    );

    let response = host.query("文件");

    assert_eq!(response.items.len(), 1);
    assert_eq!(response.items[0].title, "文件");
}

/// 结果必须携带名称与图标，UI 才能辨识。
#[test]
fn search_result_carries_name_and_icon() {
    let (host, _launcher) = host_with(
        vec![app_rich("code", "Visual Studio Code", "代码编辑器", &["editor"])],
        fast_settings(),
    );

    let response = host.query("visual");
    let item = &response.items[0];

    assert_eq!(item.title, "Visual Studio Code");
    assert_eq!(item.subtitle.as_deref(), Some("代码编辑器"));
    let icon = item.icon.as_ref().expect("结果必须带图标引用");
    assert_eq!(icon.name, "code");
}

/// 空查询显示有限数量的快速访问项，并且是结构完整的响应。
#[test]
fn empty_query_shows_quick_access_items() {
    let apps = (0..12)
        .map(|index| app(&format!("app{index}"), &format!("软件 {index}")))
        .collect();
    let (host, _launcher) = host_with(apps, fast_settings());

    let response = host.query("");

    assert!(response.items.len() <= 8, "快速访问项必须有上限");
    assert!(
        response
            .items
            .iter()
            .any(|item| item.id == "flashcast.command.rescan"),
        "空查询应包含重新扫描命令"
    );
    assert!(
        response.items.iter().any(|item| item.title == "软件 0"),
        "空查询应包含软件"
    );
    assert_eq!(response.selection, 0);
    assert!(response.notice.is_none());
    assert!(response.plugin_failures.is_empty());
}

/// 未知查询返回空且结构完整的响应。
#[test]
fn unknown_query_returns_empty_well_formed_response() {
    let (host, _launcher) = host_with(vec![app("firefox", "Firefox")], fast_settings());

    let response = host.query("绝对不存在的软件名");

    assert!(response.items.is_empty());
    assert_eq!(response.selection, 0);
    assert_eq!(response.scope, QueryScope::Home);
    assert_eq!(response.input, "绝对不存在的软件名");
    assert!(response.seq > 0);
    assert!(response.selected().is_none());
}

/// 排序：标题前缀优于标题子串，标题子串优于元数据匹配。
#[test]
fn ranking_prefers_prefix_then_substring_then_metadata() {
    let (host, _launcher) = host_with(
        vec![
            // 元数据命中：标题不含 fire，说明里含 fire。
            app_rich("meta", "编辑器", "包含 fire 的说明", &[]),
            // 标题子串命中：fire 出现在标题中间。
            app("sub", "Bonfire 工具"),
            // 标题前缀命中。
            app("prefix", "firefox"),
        ],
        fast_settings(),
    );

    let response = host.query("fire");
    let titles: Vec<&str> = response.items.iter().map(|item| item.title.as_str()).collect();

    assert_eq!(
        titles,
        vec!["firefox", "Bonfire 工具", "编辑器"],
        "前缀 > 子串 > 元数据"
    );
}

/// 相同输入重复查询得到完全相同的顺序（稳定排序）。
#[test]
fn ranking_is_stable_for_the_same_input() {
    let apps = vec![
        app("a", "Alpha 工具"),
        app("b", "Alpha 助手"),
        app("c", "Alpha 中心"),
    ];
    let (host, _launcher) = host_with(apps, fast_settings());

    let first: Vec<String> = host
        .query("alpha")
        .items
        .into_iter()
        .map(|item| item.id)
        .collect();
    let second: Vec<String> = host
        .query("alpha")
        .items
        .into_iter()
        .map(|item| item.id)
        .collect();

    assert_eq!(first, second);
    assert_eq!(first.len(), 3);
}

/// 重新扫描后能反映新安装的软件。
#[test]
fn rescan_reflects_newly_installed_application() {
    use std::sync::Arc;

    use flashcast_core::{Host, HostDeps, PluginRegistry};
    use flashcast_platform::fake::{FakeAppCatalog, FakeCapabilityProbe, FakeLauncher};

    let catalog = Arc::new(FakeAppCatalog::with_scan_results(vec![
        Ok(vec![app("firefox", "Firefox")]),
        Ok(vec![app("firefox", "Firefox"), app("code", "Visual Studio Code")]),
    ]));
    let deps = HostDeps {
        catalog,
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        plugins: Arc::new(PluginRegistry::new()),
    };
    let host = Host::new(deps, fast_settings());

    assert_eq!(host.query("visual").items.len(), 0);

    let response = host.rescan();

    assert_eq!(host.query("visual").items.len(), 1, "重新扫描后应发现新软件");
    assert!(
        response.notice.is_some(),
        "重新扫描必须给出可见反馈"
    );
}
