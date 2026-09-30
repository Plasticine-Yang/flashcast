//! 功能插件隔离的集成测试（ADR §6）：
//! 插件报错、超时或 panic 时，本轮返回空结果并记录插件错误，
//! **不影响宿主自身结果与其他插件**。
//!
//! 说明：首屏空查询只返回快速访问项，不运行插件搜索（不产生后台活动）；
//! 因此这些用例都用非空查询驱动插件贡献。

mod support;

use std::sync::Arc;
use std::time::{Duration, Instant};

use flashcast_core::{PluginFailureKind, PluginRegistry};
use support::{
    app, fast_settings, host_with_plugins, item, FailingPlugin, PanicPlugin, SlowPlugin,
    StaticPlugin,
};

/// 插件报错不影响宿主结果与其他插件。
#[test]
fn plugin_error_is_isolated_from_host_and_other_plugins() {
    let good = Arc::new(StaticPlugin::new(
        "good",
        vec![item("good:1", "好插件结果", "good", 90)],
    ));
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(good);
    plugins.register(Arc::new(FailingPlugin::new("bad")));

    let (host, _launcher) =
        host_with_plugins(vec![app("firefox", "Firefox")], fast_settings(), plugins);

    let response = host.query("x");

    assert!(
        response.items.iter().any(|i| i.title == "好插件结果"),
        "其他插件的正常结果必须保留"
    );
    assert_eq!(response.plugin_failures.len(), 1);
    let failure = &response.plugin_failures[0];
    assert_eq!(failure.plugin_id, "bad");
    assert_eq!(failure.kind, PluginFailureKind::Error);
    assert!(failure.reason.contains("插件内部错误"));

    // 宿主自身结果不受插件失败影响：换成能命中宿主的查询再验证一次。
    let host_response = host.query("fire");
    assert!(
        host_response.items.iter().any(|i| i.title == "Firefox"),
        "宿主自身结果必须保留"
    );
}

/// 插件超时被隔离，且不会拖慢整轮查询。
#[test]
fn plugin_timeout_is_isolated() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(SlowPlugin::new("slow", Duration::from_millis(1500))));
    plugins.register(Arc::new(StaticPlugin::new(
        "fast",
        vec![item("fast:1", "快插件结果", "fast", 90)],
    )));

    let (host, _launcher) =
        host_with_plugins(vec![app("firefox", "Firefox")], fast_settings(), plugins);

    let started = Instant::now();
    let response = host.query("x");
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(1000),
        "超时必须生效，实际耗时 {elapsed:?}"
    );
    assert_eq!(response.plugin_failures.len(), 1);
    assert_eq!(response.plugin_failures[0].plugin_id, "slow");
    assert_eq!(response.plugin_failures[0].kind, PluginFailureKind::Timeout);
    assert!(
        response.items.iter().any(|i| i.title == "快插件结果"),
        "未被超时影响的插件结果必须保留"
    );
}

/// 插件 panic 被隔离，宿主仍然可用。
#[test]
fn plugin_panic_is_isolated() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(PanicPlugin::new("panicky")));
    plugins.register(Arc::new(StaticPlugin::new(
        "good",
        vec![item("good:1", "好插件结果", "good", 90)],
    )));

    let (host, _launcher) =
        host_with_plugins(vec![app("firefox", "Firefox")], fast_settings(), plugins);

    let response = host.query("x");

    assert!(response.items.iter().any(|i| i.title == "好插件结果"));
    let failure = response
        .plugin_failures
        .iter()
        .find(|failure| failure.plugin_id == "panicky")
        .expect("必须记录 panic 的插件");
    assert_eq!(failure.kind, PluginFailureKind::Panic);
    assert!(failure.reason.contains("panic"));

    // panic 之后宿主仍然可以正常查询。
    assert!(host.query("fire").items.iter().any(|i| i.title == "Firefox"));
}

/// 停用插件后既不贡献结果，也不产生后台活动（不会被搜索）。
#[test]
fn disabled_plugin_does_not_contribute_or_get_searched() {
    let plugin = Arc::new(StaticPlugin::new(
        "memo",
        vec![item("memo:1", "备忘录结果", "memo", 90)],
    ));
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(plugin.clone());

    let (host, _launcher) = host_with_plugins(vec![app("a", "Alpha")], fast_settings(), plugins);

    assert!(host.query("x").items.iter().any(|i| i.source == "memo"));

    assert!(host.plugins().set_enabled("memo", false));
    let response = host.query("x");

    assert!(
        !response.items.iter().any(|i| i.source == "memo"),
        "停用后不得贡献结果"
    );
    assert_eq!(
        plugin.search_count(),
        1,
        "停用后不得再被搜索（只应有停用前的一次）"
    );
}

/// 空查询不运行插件搜索：首屏只给快速访问项，避免无谓的后台活动。
#[test]
fn empty_query_does_not_run_plugin_searches() {
    let plugin = Arc::new(StaticPlugin::new(
        "memo",
        vec![item("memo:1", "备忘录结果", "memo", 90)],
    ));
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(plugin.clone());
    let (host, _launcher) = host_with_plugins(vec![app("a", "Alpha")], fast_settings(), plugins);

    let response = host.query("");

    assert!(!response.items.iter().any(|i| i.source == "memo"));
    assert_eq!(plugin.search_count(), 0, "空查询不应触发插件搜索");
}

/// 插件清单反映启用状态与关键词。
#[test]
fn plugin_manifests_expose_keywords_and_state() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(support::KeywordPlugin::new(
        "memo",
        "备忘录",
        vec![],
    )));
    let (host, _launcher) = host_with_plugins(vec![], fast_settings(), plugins);

    let manifests = host.plugin_manifests();

    assert_eq!(manifests.len(), 1);
    assert_eq!(manifests[0].0.id, "memo");
    assert_eq!(manifests[0].0.keywords, vec!["备忘录".to_string()]);
    assert!(manifests[0].1, "默认应为启用状态");
}
