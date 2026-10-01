//! 命令入口的集成测试：全部经由 `Host::execute`（ADR §10）。

mod support;

use std::sync::Arc;

use flashcast_core::{
    ActionStatus, DefaultAction, Host, HostDeps, ItemKind, PluginRegistry, Preview, Score,
    SearchItem, COMMAND_CAPABILITIES, COMMAND_RESCAN,
};
use flashcast_platform::catalog::{AppEntry, AppSource};
use flashcast_platform::fake::{
    FakeAppCatalog, FakeCapabilityProbe, FakeChrome, FakeClipboard, FakeClipboardWatcher,
    FakeFocusTracker, FakeLauncher, FakePaster,
};
use flashcast_platform::launch::LaunchError;
use support::{app, fast_settings, host_with};

/// 回车启动软件：argv 按词传递，程序与参数分离。
#[test]
fn execute_launches_application_with_tokenized_argv() {
    let entry = AppEntry {
        id: "code.desktop".to_string(),
        name: "Visual Studio Code".to_string(),
        comment: None,
        icon: None,
        exec: vec![
            "/usr/bin/code".to_string(),
            "--new-window".to_string(),
            "/home/user/项目 目录".to_string(),
        ],
        desktop_file: None,
        working_dir: None,
        wm_class: None,
        terminal: false,
        keywords: Vec::new(),
        source: AppSource::Desktop,
    };
    let (host, launcher) = host_with(vec![entry], fast_settings());
    let response = host.query("visual");
    let item = response.selected().expect("应有选中结果").clone();

    let outcome = host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Done);
    let requests = launcher.requests();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].program, "/usr/bin/code");
    assert_eq!(
        requests[0].args,
        vec![
            "--new-window".to_string(),
            "/home/user/项目 目录".to_string()
        ]
    );
}

/// 含 shell 元字符的参数必须原样传递，证明没有经过 shell。
#[test]
fn execute_passes_arguments_without_shell_interpretation() {
    let entry = AppEntry {
        id: "risky.desktop".to_string(),
        name: "风险参数示例".to_string(),
        exec: vec![
            "/usr/bin/printf".to_string(),
            "a; rm -rf / #".to_string(),
            "$(whoami)".to_string(),
            "管道|与&符号".to_string(),
        ],
        ..app("risky", "风险参数示例")
    };
    let (host, launcher) = host_with(vec![entry], fast_settings());
    let item = host.query("风险").selected().expect("应有结果").clone();

    let outcome = host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Done);
    let request = launcher
        .requests()
        .into_iter()
        .next()
        .expect("应有启动请求");
    assert_eq!(request.program, "/usr/bin/printf");
    assert_eq!(
        request.args,
        vec![
            "a; rm -rf / #".to_string(),
            "$(whoami)".to_string(),
            "管道|与&符号".to_string(),
        ],
        "参数必须逐字传递，不得被 shell 解释或转义"
    );
    assert!(flashcast_platform::launch::contains_shell_metacharacters(
        &request.args[0]
    ));
}

/// 启动失败必须给出可理解的中文反馈。
#[test]
fn launch_failure_produces_chinese_feedback() {
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(vec![app(
            "missing",
            "不存在的软件",
        )])),
        launcher: Arc::new(FakeLauncher::always_fails(LaunchError::ProgramNotFound {
            program: "/usr/bin/missing".to_string(),
        })),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        clipboard: Arc::new(FakeClipboard::new()),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: Arc::new(FakeChrome::not_installed("测试环境未配置 Chrome")),
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: support::unique_dir("device"),
    };
    let host = Host::new(deps, fast_settings());
    let item = host.query("不存在").selected().expect("应有结果").clone();

    let outcome = host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("失败必须带中文反馈");
    assert!(
        message.contains("无法启动"),
        "反馈应说明启动失败：{message}"
    );
    assert!(
        message.contains("不存在的软件"),
        "反馈应包含软件名：{message}"
    );
    assert!(
        message.contains("找不到可执行文件"),
        "反馈应包含底层原因：{message}"
    );
}

/// 软件条目已被移除时给出可操作的反馈。
#[test]
fn execute_stale_application_item_reports_failure() {
    let (host, launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    let stale = SearchItem {
        id: "app:gone.desktop".to_string(),
        title: "已卸载的软件".to_string(),
        subtitle: None,
        icon: None,
        source: flashcast_core::HOST_SOURCE.to_string(),
        kind: ItemKind::Application,
        default_action: DefaultAction::Open,
        preview: Preview::None,
        score: Score::unordered(),
    };

    let outcome = host.execute(&stale);

    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(launcher.requests().is_empty(), "不应尝试启动");
    let message = outcome.message.expect("失败必须带反馈");
    assert!(message.contains("重新扫描"), "应给出可操作建议：{message}");
}

/// 来源插件不在清单里时，执行必须给出明确的中文反馈，而不是静默失败。
///
/// ticket 07 起备忘录可以执行、ticket 09 起剪贴板历史也可以执行，因此这里用「剪贴板
/// 条目但没有安装对应插件」这个真实场景：断言的是「未授权的来源一律拒绝并说明原因」
/// 这条行为，而不是某一个具体种类。
#[test]
fn execute_clipboard_entry_without_installed_plugin_reports_failure() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    let clipboard = SearchItem {
        id: "clipboard:1".to_string(),
        title: "剪贴板条目".to_string(),
        subtitle: None,
        icon: None,
        source: "clipboard".to_string(),
        kind: ItemKind::ClipboardEntry,
        default_action: DefaultAction::Paste,
        preview: Preview::None,
        score: Score::unordered(),
    };

    let outcome = host.execute(&clipboard);

    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(
        outcome
            .message
            .expect("应有反馈")
            .contains("不在插件清单里"),
        "必须说明来源插件不存在，而不是静默失败"
    );
}

/// 空查询中的「重新扫描软件」快速访问项可以执行，并给出反馈。
#[test]
fn rescan_command_from_quick_access_executes() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    let response = host.query("");
    let command = response
        .items
        .iter()
        .find(|item| item.id == COMMAND_RESCAN)
        .expect("空查询应包含重新扫描命令")
        .clone();

    let outcome = host.execute(&command);

    assert_eq!(outcome.status, ActionStatus::Done);
    assert!(outcome.message.expect("应给出反馈").contains("重新扫描"));
}

/// 「查看平台能力」快速访问项给出真实的能力摘要（替身环境为 X11）。
#[test]
fn capabilities_command_reports_probe_result() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha")], fast_settings());
    let command = host
        .query("")
        .items
        .into_iter()
        .find(|item| item.id == COMMAND_CAPABILITIES)
        .expect("空查询应包含能力命令");

    let outcome = host.execute(&command);

    assert_eq!(outcome.status, ActionStatus::Done);
    let message = outcome.message.expect("应给出能力摘要");
    assert!(message.contains("会话"), "摘要应包含会话类型：{message}");
    assert!(message.contains("X11"), "替身环境为 X11：{message}");
}
