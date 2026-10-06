//! 键盘选择、鼠标移动与过期查询的集成测试。

mod support;

use std::sync::{Arc, Mutex};

use flashcast_core::{PluginRegistry, QueryScope};
use support::{app, fast_settings, host_with, host_with_plugins, item, KeywordPlugin};

/// 上下键移动选择，并在首尾处收敛而不是循环。
#[test]
fn arrow_keys_move_selection_and_clamp_at_edges() {
    let (host, _launcher) = host_with(
        vec![app("a", "Alpha"), app("b", "Beta"), app("c", "Gamma")],
        fast_settings(),
    );
    host.query("a");

    assert_eq!(host.move_selection(1).selection, 1);
    assert_eq!(host.move_selection(1).selection, 2);
    assert_eq!(
        host.move_selection(1).selection,
        2,
        "最后一项继续下移应停住"
    );
    assert_eq!(host.move_selection(-1).selection, 1);
    assert_eq!(host.move_selection(-1).selection, 0);
    assert_eq!(host.move_selection(-1).selection, 0, "第一项继续上移应停住");
}

/// 新输入重置键盘选择；同一输入的重复渲染保留选择。
#[test]
fn new_input_resets_selection_but_rerender_keeps_it() {
    let (host, _launcher) = host_with(
        vec![
            app("a", "Alpha"),
            app("b", "Alpha 二"),
            app("c", "Alpha 三"),
        ],
        fast_settings(),
    );
    host.query("alpha");
    assert_eq!(host.set_selection(2).selection, 2);

    let rerender = host.query("alpha");
    assert_eq!(rerender.selection, 2, "同一输入的重复渲染不应重置选择");

    let new_input = host.query("alpha ");
    assert_eq!(new_input.selection, 0, "新输入应重置选择");
}

/// 鼠标移动不改变键盘选择。
///
/// 宿主只在 `set_selection` / `select` / `move_selection` 中修改选择；UI 的
/// 鼠标悬停不调用这些入口，因此悬停不会抢走键盘选择。这里通过「重复渲染」与
/// 「快照」这两条悬停/重绘会触发的只读路径来断言宿主侧的不变量；
/// 真实鼠标事件的证明在浏览器交互检查中（见 tools/ui-check）。
#[test]
fn mouse_movement_does_not_change_selection() {
    let (host, _launcher) = host_with(
        vec![
            app("a", "Alpha"),
            app("b", "Beta"),
            app("c", "Gamma"),
            app("d", "Delta"),
            app("e", "Epsilon"),
        ],
        fast_settings(),
    );
    host.query("");
    assert_eq!(host.set_selection(3).selection, 3);

    // 悬停/重绘只会触发只读路径：重新渲染与快照。
    assert_eq!(host.query("").selection, 3, "重绘不得改变选择");
    assert_eq!(host.snapshot().selection, 3, "快照不得改变选择");
    assert_eq!(host.snapshot().selection, 3, "多次快照不得改变选择");
}

/// seq 单调递增，且过期响应可以被调用方安全丢弃。
#[test]
fn stale_query_responses_are_identified_by_seq() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha"), app("b", "Beta")], fast_settings());

    let first = host.query("a");
    let second = host.query("al");

    assert!(second.seq > first.seq, "seq 必须单调递增");
    assert!(first.is_stale(second.seq), "旧响应必须能被识别为过期并丢弃");
    assert!(!second.is_stale(first.seq), "新响应不应被判定为过期");
}

/// 并发查询下 seq 不重复：UI 才能可靠地丢弃旧响应。
#[test]
fn seq_is_unique_under_concurrent_queries() {
    let (host, _launcher) = host_with(vec![app("a", "Alpha"), app("b", "Beta")], fast_settings());
    let host = Arc::new(host);
    let seqs = Arc::new(Mutex::new(Vec::new()));

    let mut handles = Vec::new();
    for thread in 0..8 {
        let host = Arc::clone(&host);
        let seqs = Arc::clone(&seqs);
        handles.push(std::thread::spawn(move || {
            for round in 0..25 {
                let input = if (thread + round) % 2 == 0 { "a" } else { "b" };
                let response = host.query(input);
                seqs.lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .push(response.seq);
            }
        }));
    }
    for handle in handles {
        handle.join().expect("查询线程不应 panic");
    }

    let mut collected = seqs
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    collected.sort_unstable();
    let total = collected.len();
    collected.dedup();
    assert_eq!(total, 200, "每个线程的每次查询都应返回响应");
    assert_eq!(collected.len(), 200, "seq 在并发下必须唯一");
    assert_eq!(collected.last().copied(), Some(200));
}

/// `back()` 恢复进入插件范围之前的查询、范围与选择。
#[test]
fn back_restores_previous_input_scope_and_selection() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(KeywordPlugin::new(
        "memo",
        "备忘录",
        vec![item("memo:1", "常用回复", "memo", 80)],
    )));
    let (host, _launcher) = host_with_plugins(
        vec![app("a", "Alpha"), app("b", "Beta")],
        fast_settings(),
        plugins,
    );

    host.query("");
    assert_eq!(host.set_selection(1).selection, 1);

    // 输入完整匹配插件关键词 → 进入插件范围。
    let in_scope = support::plugin_query(&host, "备忘录");
    assert_eq!(
        in_scope.scope,
        QueryScope::Plugin {
            id: "memo".to_string(),
            keyword: "备忘录".to_string()
        }
    );
    assert_eq!(in_scope.items[0].title, "常用回复");
    assert_eq!(in_scope.selection, 0, "进入范围后选择归零");

    let back = host.back();

    assert!(back.restored, "应能返回上一查询范围");
    assert_eq!(back.response.scope, QueryScope::Home, "范围恢复为首屏");
    assert_eq!(back.response.input, "", "输入恢复");
    assert_eq!(back.response.selection, 1, "选择恢复");

    // 已在最外层时 back() 不再恢复，UI 据此关闭窗口。
    let outermost = host.back();
    assert!(!outermost.restored);
}
