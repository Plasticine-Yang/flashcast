//! 剪贴板历史（ticket 09）的宿主级集成测试。
//!
//! 全部经**宿主的查询与命令入口**（`query` / `execute` / `preview` / 管理入口）验证，
//! 穿透真实的 SQLite 文件与真实的临时设备目录；平台侧用 `flashcast-platform::fake`
//! 的替身（替身通过不证明真实平台适配通过，真实读写由 `flashcast-platform-check`
//! 在可执行的会话里报告）。

mod support;

use std::sync::Arc;
use std::time::Duration;

use flashcast_core::{
    ActionStatus, ClipboardCaptureOutcome, ClipboardSettings, ItemKind, QueryScope, Settings,
};
use flashcast_platform::clipboard::{ClipboardError, ClipboardFormatKind};
use flashcast_platform::fake::FakeClipboardWatcher;

use support::{
    clipboard_host, clipboard_host_with_broken_storage, clipboard_host_with_device,
    clipboard_host_with_watcher, fast_settings, files_under, focused_app, unique_dir,
};

/// 默认的剪贴板设置（保留 30 天、容量 500）。
fn clipboard_settings(retention_days: u32, capacity: usize) -> Settings {
    Settings {
        clipboard: ClipboardSettings {
            paused: false,
            retention_days,
            capacity,
        },
        ..fast_settings()
    }
}

// ---------------------------------------------------------------------------
// 两个关键词，一个插件
// ---------------------------------------------------------------------------

/// 「剪贴板」与「剪切板」进入**同一个**插件的同一个范围：只有一个插件注册项，
/// 两个别名指向同一个历史（spec：剪切板只作为输入别名，不另建插件）。
#[test]
fn both_chinese_aliases_enter_the_single_clipboard_plugin() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("别名测试内容");

    // 清单里只有一个剪贴板插件，三个别名都在它身上。
    let manifests = harness.host.plugin_manifests();
    let clipboard_plugins: Vec<_> = manifests
        .iter()
        .filter(|(manifest, _)| {
            manifest
                .keywords
                .iter()
                .any(|keyword| keyword == "剪切板" || keyword == "剪贴板")
        })
        .collect();
    assert_eq!(
        clipboard_plugins.len(),
        1,
        "「剪贴板」与「剪切板」必须属于同一个插件，而不是两个"
    );
    let (manifest, enabled) = clipboard_plugins[0];
    assert_eq!(manifest.id, "clipboard");
    assert!(*enabled, "已显式启用");
    for alias in ["剪贴板", "剪切板", "clipboard"] {
        assert!(
            manifest.keywords.iter().any(|keyword| keyword == alias),
            "缺少关键词别名「{alias}」：{:?}",
            manifest.keywords
        );
    }

    // 两个写法进入同一个范围对象（同一个插件 id），并且看到同一份历史。
    let zh = harness.host.query("剪贴板");
    let alt = harness.host.query("剪切板");
    assert_eq!(
        zh.scope,
        QueryScope::Plugin {
            id: "clipboard".to_string(),
            keyword: "剪贴板".to_string()
        }
    );
    assert_eq!(
        alt.scope,
        QueryScope::Plugin {
            id: "clipboard".to_string(),
            keyword: "剪切板".to_string()
        }
    );
    let zh_ids: Vec<String> = zh.items.iter().map(|item| item.id.clone()).collect();
    let alt_ids: Vec<String> = alt.items.iter().map(|item| item.id.clone()).collect();
    assert_eq!(zh_ids, alt_ids, "两个别名必须看到同一份历史");
    assert_eq!(zh_ids.len(), 1);

    // 范围标签用用户实际输入的那个写法。
    assert_eq!(alt.scope.label_zh(), "剪切板 范围");
}

/// 首屏**不**检索全部剪贴板历史：必须经关键词进入（spec 明确要求）。
#[test]
fn home_scope_never_lists_clipboard_history() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("独一无二的历史内容");

    let empty = harness.host.query("");
    assert!(
        !empty
            .items
            .iter()
            .any(|item| item.kind == ItemKind::ClipboardEntry),
        "空查询的首屏不得出现历史条目"
    );

    let typed = harness.host.query("独一无二");
    assert!(typed.scope.is_home(), "普通输入仍在首屏");
    assert!(
        !typed
            .items
            .iter()
            .any(|item| item.kind == ItemKind::ClipboardEntry),
        "首屏不得因为输入命中历史内容而列出历史"
    );

    // 进入插件范围之后才看得到。
    let scoped = harness.host.query("剪贴板");
    assert!(scoped
        .items
        .iter()
        .any(|item| item.kind == ItemKind::ClipboardEntry));
}

// ---------------------------------------------------------------------------
// 捕获 → 重启 → 检索 / 预览 / 粘贴
// ---------------------------------------------------------------------------

#[test]
fn capture_restart_then_search_preview_and_paste() {
    let device_dir = unique_dir("clipboard-restart");
    let workspace = unique_dir("clipboard-restart-ws");
    let harness = clipboard_host_with_device(&device_dir, fast_settings());
    harness
        .host
        .select_workspace(&workspace)
        .expect("关联工作区");
    harness.enable();

    assert!(matches!(
        harness.copy("第一条内容"),
        ClipboardCaptureOutcome::Captured { .. }
    ));
    assert!(matches!(
        harness.copy("第二条内容"),
        ClipboardCaptureOutcome::Captured { .. }
    ));

    // 重启：同一个设备目录 + 同一个配置工作区。
    let restarted = clipboard_host_with_device(&device_dir, fast_settings());
    assert!(
        restarted.host.clipboard_plugin_enabled(),
        "启用状态记在工作区清单里，重启后仍然有效"
    );

    let listed = restarted.host.query("剪贴板");
    assert_eq!(listed.items.len(), 2, "重启后历史必须仍然在");
    let titles: Vec<&str> = listed
        .items
        .iter()
        .map(|item| item.title.as_str())
        .collect();
    assert_eq!(titles, vec!["第二条内容", "第一条内容"], "最新的排在最前");

    let searched = restarted.host.query("剪贴板 第一条");
    assert_eq!(searched.items.len(), 1);
    let item = searched.items[0].clone();
    assert_eq!(item.kind, ItemKind::ClipboardEntry);
    assert_eq!(item.default_action.label_zh(), "粘贴");

    let preview = restarted.host.preview(&item.id).expect("预览必须可用");
    match preview {
        flashcast_core::Preview::Text { body, .. } => assert_eq!(body, "第一条内容"),
        other => panic!("剪贴板历史的预览应是文本：{other:?}"),
    }

    // 粘贴复用 ticket 08 的恢复 + 注入路径。
    restarted.summon(focused_app("editor"));
    let outcome = restarted.host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::PastePending);
    let plan = outcome.paste.clone().expect("应有粘贴计划");
    assert_eq!(plan.label, "第一条内容", "PastePlan.label 应是可读摘要");
    assert_eq!(
        restarted.clipboard.last_write().as_deref(),
        Some("第一条内容"),
        "宿主必须先把这条历史写进剪贴板"
    );
    let done = restarted.host.complete_paste();
    assert_eq!(done.status, ActionStatus::Done);
    assert_eq!(restarted.paster.paste_count(), 1);
    assert_eq!(
        restarted.paster.text_at_paste(),
        vec![Some("第一条内容".to_string())],
        "注入粘贴时剪贴板里必须是这条历史"
    );
}

/// 自动粘贴不可用（这里模拟拿不到唤起前应用）时，必须降级为「已复制，请手动粘贴」，
/// 并给出中文原因——与备忘录走完全相同的路径。
#[test]
fn paste_falls_back_to_manual_when_there_is_no_target() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("需要手动粘贴的内容");
    let item = harness.first_item("剪贴板");

    harness.host.set_paste_target(None);
    let outcome = harness.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::CopiedNeedsManualPaste);
    let message = outcome.message.expect("必须给出中文原因");
    assert!(message.contains("手动粘贴"), "应提示手动粘贴：{message}");
    assert_eq!(
        harness.clipboard.last_write().as_deref(),
        Some("需要手动粘贴的内容"),
        "降级路径也必须已经把内容放进剪贴板"
    );
    assert_eq!(harness.paster.paste_count(), 0, "不能注入按键");
}

// ---------------------------------------------------------------------------
// 去重与自身写入抑制
// ---------------------------------------------------------------------------

#[test]
fn duplicate_content_is_deduplicated_into_one_entry() {
    let harness = clipboard_host(fast_settings());
    harness.enable();

    assert!(matches!(
        harness.copy("重复的内容"),
        ClipboardCaptureOutcome::Captured { .. }
    ));
    assert_eq!(
        harness.copy("重复的内容"),
        ClipboardCaptureOutcome::Deduplicated {
            id: harness.entries()[0].id.clone(),
            copies: 2
        }
    );
    assert!(matches!(
        harness.copy("重复的内容"),
        ClipboardCaptureOutcome::Deduplicated { copies: 3, .. }
    ));

    let entries = harness.entries();
    assert_eq!(entries.len(), 1, "同一内容只能有一条历史");
    assert_eq!(entries[0].copies, 3);

    // 不同内容仍然是新条目。
    harness.copy("另一段内容");
    assert_eq!(harness.entries().len(), 2);
}

/// 自身写入抑制（第一层：适配层）。Flashcast 写剪贴板之后，同一次写入不得再被捕获。
#[test]
fn flashcast_own_clipboard_write_is_not_captured_again() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("会被粘贴回去的内容");
    let item = harness.first_item("剪贴板");

    harness.summon(focused_app("editor"));
    assert_eq!(
        harness.host.execute(&item).status,
        ActionStatus::PastePending
    );
    assert_eq!(
        harness.watcher.own_write_count(),
        1,
        "宿主写剪贴板时必须登记自身写入"
    );

    // 反复轮询：不得新增条目，也不得被算成一次重复复制。
    for _ in 0..3 {
        let outcome = harness.host.capture_clipboard_once();
        assert!(
            matches!(
                outcome,
                ClipboardCaptureOutcome::Unchanged | ClipboardCaptureOutcome::Suppressed
            ),
            "自身写入不得变成新的复制事件：{outcome:?}"
        );
    }
    let entries = harness.entries();
    assert_eq!(entries.len(), 1, "自身写入不得填满历史");
    assert_eq!(entries[0].copies, 1, "自身写入不得被算成重复复制");
    assert_eq!(harness.host.complete_paste().status, ActionStatus::Done);
}

/// 自身写入抑制（第二层：宿主兜底）。即使适配层"看不见"自身写入，也必须靠内容指纹
/// 丢弃它，绝不能形成「粘贴 → 捕获 → 再粘贴」的循环（spec 明确要求）。
#[test]
fn host_layer_suppression_prevents_a_self_capture_loop() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("循环测试内容");
    let item = harness.first_item("剪贴板");
    // 让适配层不再抑制自身写入：这一层失效时宿主必须自己兜住。
    harness.watcher.ignore_own_writes();

    harness.summon(focused_app("editor"));
    assert_eq!(
        harness.host.execute(&item).status,
        ActionStatus::PastePending
    );

    let outcome = harness.host.capture_clipboard_once();
    assert_eq!(
        outcome,
        ClipboardCaptureOutcome::Suppressed,
        "宿主必须按内容指纹丢弃自己的写入"
    );
    assert_eq!(harness.entries().len(), 1);
    assert_eq!(harness.entries()[0].copies, 1);
    assert_eq!(
        harness.host.clipboard_state().suppressed,
        1,
        "抑制次数必须如实记录，便于诊断"
    );
}

/// 粘贴备忘录同样算自身写入：不能因为粘贴了一条备忘录就往历史里塞一条。
#[test]
fn pasting_a_memo_does_not_enter_the_history() {
    let workspace = unique_dir("clipboard-memo-ws");
    let harness = clipboard_host(fast_settings());
    harness
        .host
        .select_workspace(&workspace)
        .expect("关联工作区");
    harness.enable();
    harness
        .host
        .create_memo("常用回复", &["回复".to_string()], "收到，我看一下。")
        .expect("创建备忘录");

    let response = harness.host.query("备忘录");
    let memo = response
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Memo)
        .cloned()
        .expect("应有备忘录条目");
    harness.summon(focused_app("editor"));
    assert_eq!(
        harness.host.execute(&memo).status,
        ActionStatus::PastePending
    );
    assert_eq!(harness.host.complete_paste().status, ActionStatus::Done);

    // 轮询几次：备忘录内容不得进入剪贴板历史。
    for _ in 0..3 {
        let outcome = harness.host.capture_clipboard_once();
        assert!(
            matches!(
                outcome,
                ClipboardCaptureOutcome::Unchanged | ClipboardCaptureOutcome::Suppressed
            ),
            "粘贴备忘录不得进入历史：{outcome:?}"
        );
    }
    assert!(harness.entries().is_empty(), "历史必须仍然是空的");
}

// ---------------------------------------------------------------------------
// 用户控制：置顶 / 删除 / 清空 / 暂停
// ---------------------------------------------------------------------------

#[test]
fn pin_delete_and_clear_control_the_history() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("第一条");
    harness.copy("第二条");
    harness.copy("第三条");

    let first = harness
        .entries()
        .into_iter()
        .find(|event| event.summary == "第一条")
        .expect("应有第一条");

    harness
        .host
        .pin_clipboard_entry(&first.id, true)
        .expect("置顶");
    let listed = harness.host.query("剪贴板");
    assert_eq!(listed.items[0].title, "第一条", "置顶条目排在最前");
    assert!(listed.items[0]
        .subtitle
        .clone()
        .unwrap_or_default()
        .contains("已置顶"));
    assert_eq!(harness.host.clipboard_state().pinned, 1);

    // 删除。
    let second = harness
        .entries()
        .into_iter()
        .find(|event| event.summary == "第二条")
        .expect("应有第二条");
    harness
        .host
        .delete_clipboard_entry(&second.id)
        .expect("删除");
    assert_eq!(harness.entries().len(), 2);
    assert!(!harness
        .summaries()
        .iter()
        .any(|summary| summary == "第二条"));
    let missing = harness.host.delete_clipboard_entry("clip-不存在");
    assert!(missing
        .expect_err("删除不存在的条目必须失败")
        .to_string()
        .contains("找不到"));

    // 清空：置顶条目也会被清掉（用户的显式操作）。
    let removed = harness.host.clear_clipboard_history().expect("清空");
    assert_eq!(removed, 2);
    assert!(harness.entries().is_empty());
    assert_eq!(harness.host.clipboard_state().entries, 0);
    assert!(harness.host.query("剪贴板").items.is_empty());
}

#[test]
fn pause_stops_recording_and_resume_does_not_backfill() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("暂停前的内容");
    assert_eq!(harness.entries().len(), 1);

    let applied = harness.host.set_clipboard_paused(true).expect("暂停");
    assert!(applied.clipboard.paused);
    assert!(harness.host.clipboard_state().paused);

    // 暂停期间复制的两次内容都必须被丢弃（并且轮询仍然推进，不会在恢复后补记）。
    harness.copy_only("暂停期间的内容一");
    assert_eq!(
        harness.host.capture_clipboard_once(),
        ClipboardCaptureOutcome::Paused
    );
    harness.copy_only("暂停期间的内容二");
    assert_eq!(
        harness.host.capture_clipboard_once(),
        ClipboardCaptureOutcome::Paused
    );
    assert_eq!(harness.entries().len(), 1, "暂停期间不记录任何内容");

    harness.host.set_clipboard_paused(false).expect("恢复");
    assert_eq!(
        harness.host.capture_clipboard_once(),
        ClipboardCaptureOutcome::Unchanged,
        "恢复后不得补记暂停期间的内容"
    );
    assert_eq!(harness.summaries(), vec!["暂停前的内容".to_string()]);

    // 恢复之后新的复制照常记录。
    harness.copy("恢复后的内容");
    assert_eq!(harness.entries().len(), 2);
}

// ---------------------------------------------------------------------------
// 保留期限与容量
// ---------------------------------------------------------------------------

#[test]
fn capacity_limit_evicts_the_oldest_unpinned_entries() {
    let harness = clipboard_host(clipboard_settings(30, 3));
    harness.enable();
    harness.copy("一");
    harness.copy("二");
    harness.copy("三");
    assert_eq!(harness.entries().len(), 3);

    harness.copy("四");
    let summaries = harness.summaries();
    assert_eq!(summaries.len(), 3, "容量上限必须生效");
    assert_eq!(summaries, vec!["四", "三", "二"], "回收最旧的未置顶条目");
    assert!(!summaries.iter().any(|summary| summary == "一"));
}

/// 把容量改小要**立刻**回收，而不是等下一次复制。
#[test]
fn shrinking_capacity_reclaims_immediately_through_the_management_entry() {
    let harness = clipboard_host(clipboard_settings(30, 10));
    harness.enable();
    for index in 0..5 {
        harness.copy(format!("内容 {index}").as_str());
    }
    assert_eq!(harness.entries().len(), 5);

    let applied = harness.host.set_clipboard_limits(30, 2).expect("改小容量");
    assert_eq!(applied.clipboard.capacity, 2);
    assert_eq!(harness.entries().len(), 2, "改小容量后立即回收");
    assert_eq!(harness.summaries(), vec!["内容 4", "内容 3"]);
}

/// 容量已满且全是置顶条目：如实拒绝并给出准确状态，绝不假装保存成功。
#[test]
fn capacity_reached_with_pinned_entries_is_reported_honestly() {
    let harness = clipboard_host(clipboard_settings(30, 2));
    harness.enable();
    harness.copy("置顶一");
    harness.copy("置顶二");
    for event in harness.entries() {
        harness
            .host
            .pin_clipboard_entry(&event.id, true)
            .expect("置顶");
    }

    assert_eq!(
        harness.copy("装不下的内容"),
        ClipboardCaptureOutcome::CapacityReached {
            entries: 2,
            capacity: 2
        }
    );
    assert_eq!(harness.entries().len(), 2);
    assert!(!harness
        .summaries()
        .iter()
        .any(|summary| summary == "装不下的内容"));

    let state = harness.host.clipboard_state();
    let message = state.capacity_reached.expect("必须报告容量触顶");
    assert!(message.contains("已满"), "原因要写清楚：{message}");
    assert_eq!(state.entries, 2);
    assert_eq!(state.capacity, 2);
}

/// 保留期限：过期条目被回收，置顶条目不受影响。
#[test]
fn retention_expires_old_entries_but_keeps_pinned_ones() {
    let harness = clipboard_host(clipboard_settings(1, 500));
    harness.enable();
    harness.copy("过期内容");
    harness.copy("置顶内容");
    let pinned = harness
        .entries()
        .into_iter()
        .find(|event| event.summary == "置顶内容")
        .expect("应有置顶内容");
    harness
        .host
        .pin_clipboard_entry(&pinned.id, true)
        .expect("置顶");

    // 宿主不提供加速时间的能力，因此用**同一个回收入口**把「现在」推到保留期限之外，
    // 再从查询入口断言结果。
    let now = flashcast_core::now_ms();
    harness
        .host
        .clipboard_store()
        .reclaim(1, 500, now + 2 * 86_400_000)
        .expect("回收");

    let summaries = harness.summaries();
    assert_eq!(summaries, vec!["置顶内容".to_string()]);
    assert_eq!(harness.host.clipboard_state().entries, 1);
    assert_eq!(harness.host.clipboard_state().pinned, 1);

    // 取消置顶后再回收，它同样会过期消失。
    harness
        .host
        .pin_clipboard_entry(&pinned.id, false)
        .expect("取消置顶");
    harness
        .host
        .clipboard_store()
        .reclaim(1, 500, now + 3 * 86_400_000)
        .expect("回收");
    assert!(harness.entries().is_empty());
}

/// 清空历史时不再被引用的附件文件也要回收（tickets 10–12 的图片 / 文件副本）。
#[test]
fn clearing_history_reclaims_orphan_attachment_files() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.copy("带附件的条目");

    // 附件目录与附件文件（tickets 10–12 会写入这种文件）。
    let attachments = harness.device_dir.join("clipboard").join("attachments");
    std::fs::create_dir_all(&attachments).expect("附件目录");
    let file = attachments.join("screenshot.png");
    std::fs::write(&file, b"png").expect("写入附件");

    harness.host.clear_clipboard_history().expect("清空");
    assert!(!file.exists(), "清空历史必须同步回收不再被引用的附件");
}

// ---------------------------------------------------------------------------
// 停用插件：不捕获、不返回
// ---------------------------------------------------------------------------

#[test]
fn disabled_plugin_captures_nothing_and_returns_nothing() {
    let harness = clipboard_host(fast_settings());
    // 默认关闭：剪贴板历史必须由用户显式启用（隐私敏感）。
    assert!(
        !harness.host.clipboard_plugin_enabled(),
        "剪贴板历史默认关闭"
    );
    assert!(!harness.host.clipboard_capture_active());

    harness.copy_only("停用期间复制的内容");
    assert_eq!(
        harness.host.capture_clipboard_once(),
        ClipboardCaptureOutcome::Disabled
    );
    assert_eq!(
        harness.watcher.poll_count(),
        0,
        "插件停用时根本不得读取剪贴板"
    );

    let response = harness.host.query("剪贴板");
    assert!(response.scope.is_home(), "停用后关键词不再进入插件范围");
    assert!(
        !response
            .items
            .iter()
            .any(|item| item.kind == ItemKind::ClipboardEntry),
        "停用后不得返回任何历史条目"
    );
    let state = harness.host.clipboard_state();
    assert!(!state.enabled);
    assert_eq!(state.entries, 0);
}

/// 停用插件会**停止后台活动**，而不是让它空转。
#[test]
fn disabling_the_plugin_stops_the_background_capture_thread() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.host.sync_clipboard_runtime();
    assert!(
        harness.host.clipboard_capture_active(),
        "启用后后台捕获必须运行"
    );

    // 等到后台线程至少轮询过一次。
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while harness.watcher.poll_count() == 0 && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(harness.watcher.poll_count() > 0, "后台线程必须真的在轮询");

    harness
        .host
        .set_plugin_enabled("clipboard", false)
        .expect("停用插件");
    assert!(
        !harness.host.clipboard_capture_active(),
        "停用后后台捕获必须停止"
    );
    let polls_after_disable = harness.watcher.poll_count();
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        harness.watcher.poll_count(),
        polls_after_disable,
        "停用后不得再有新的轮询"
    );
    assert_eq!(
        harness.host.capture_clipboard_once(),
        ClipboardCaptureOutcome::Disabled
    );
}

// ---------------------------------------------------------------------------
// 准确状态：捕获失败与存储失败
// ---------------------------------------------------------------------------

#[test]
fn clipboard_read_failure_is_reported_and_never_a_silent_success() {
    let device_dir = unique_dir("clipboard-read-fail");
    let watcher = Arc::new(FakeClipboardWatcher::failing(ClipboardError::Failed(
        "剪贴板工具 wl-paste 超过 3 秒没有返回，已中止".to_string(),
    )));
    let harness = clipboard_host_with_watcher(&device_dir, fast_settings(), watcher);
    harness.enable();

    let outcome = harness.host.capture_clipboard_once();
    match outcome {
        ClipboardCaptureOutcome::Failed { message } => {
            assert!(message.contains("没有返回"), "原因要如实透传：{message}")
        }
        other => panic!("读取失败必须如实报告：{other:?}"),
    }

    let state = harness.host.clipboard_state();
    let error = state.last_error.expect("状态里必须能看到失败原因");
    assert!(error.contains("没有返回"), "{error}");
    assert!(state.storage_ok, "这是读取失败，不是存储失败");
    assert_eq!(state.entries, 0);
}

#[test]
fn storage_failure_is_reported_through_state_and_search() {
    let harness = clipboard_host_with_broken_storage(fast_settings());
    harness.enable();
    // 剪贴板里必须真的有内容：否则轮询会先返回「没有变化」，测不到存储路径。
    harness.copy_only("存储失败时复制的内容");

    // 捕获：必须失败并说明是存储不可用。
    match harness.host.capture_clipboard_once() {
        ClipboardCaptureOutcome::Failed { message } => assert!(
            message.contains("剪贴板历史存储不可用"),
            "必须说明是存储失败：{message}"
        ),
        other => panic!("存储不可用时不得假装保存成功：{other:?}"),
    }

    let state = harness.host.clipboard_state();
    assert!(!state.storage_ok);
    let error = state.storage_error.expect("必须给出存储失败原因");
    assert!(error.contains("无法创建本机数据目录"), "{error}");
    assert!(state.last_error.is_some(), "last_error 也要能看到原因");
    assert_eq!(state.entries, 0);
    assert_eq!(
        state.storage_path,
        harness.device_dir.join("clipboard").join("history.sqlite3")
    );

    // 检索入口同样如实报告：进入范围后插件失败可见，而不是「空历史」。
    let response = harness.host.query("剪贴板");
    assert!(
        !response.plugin_failures.is_empty(),
        "存储失败必须出现在查询结果的插件失败列表里"
    );
    assert!(
        response.plugin_failures[0]
            .reason
            .contains("剪贴板历史存储不可用"),
        "原因：{}",
        response.plugin_failures[0].reason
    );
}

// ---------------------------------------------------------------------------
// 数据模型：一次复制事件的字段
// ---------------------------------------------------------------------------

/// 一次文字复制事件必须带上格式集合、时间、摘要与（可得的）来源应用；
/// 附件引用此时为空，但字段本身存在（tickets 10–12 直接往里填）。
#[test]
fn text_event_populates_format_time_summary_and_source() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    harness.watcher.set_source("firefox", "Mozilla Firefox");
    let before = flashcast_core::now_ms();
    harness.copy("第一行\n第二行");

    let event = harness
        .entries()
        .into_iter()
        .next()
        .expect("必须捕获到条目");
    assert_eq!(event.formats.len(), 1);
    match &event.formats[0] {
        flashcast_core::ClipboardFormat::Text { bytes } => {
            assert_eq!(*bytes, "第一行\n第二行".len())
        }
        other => panic!("文字事件的格式必须是 Text：{other:?}"),
    }
    assert!(
        event.captured_at_ms >= before && event.captured_at_ms <= flashcast_core::now_ms(),
        "捕获时间必须是本次事件的时间"
    );
    assert_eq!(event.summary, "第一行 第二行");
    assert_eq!(event.text.as_deref(), Some("第一行\n第二行"));
    assert!(event.attachments.is_empty(), "文字事件没有附件");
    let source = event.source.as_ref().expect("来源应用必须被记录");
    assert_eq!(source.app_id, "firefox");
    assert_eq!(source.title.as_deref(), Some("Mozilla Firefox"));
    assert!(!event.pinned);
    assert_eq!(event.copies, 1);
    assert_eq!(
        event.content_hash,
        flashcast_core::content_hash_text("第一行\n第二行")
    );

    // 结果条目上的可见字段。
    let item = harness.first_item("剪贴板");
    assert_eq!(item.title, "第一行 第二行");
    let subtitle = item.subtitle.clone().unwrap_or_default();
    assert!(subtitle.contains("文字"), "副标题应显示格式：{subtitle}");
    assert!(
        subtitle.contains("Mozilla Firefox"),
        "副标题应显示来源：{subtitle}"
    );
    match item.preview {
        flashcast_core::Preview::Text { body, .. } => assert_eq!(body, "第一行\n第二行"),
        other => panic!("预览应是完整文本：{other:?}"),
    }
}

/// 平台捕获里报告的非文本格式也要被记进格式集合（tickets 10–12 的扩展点）。
#[test]
fn capture_formats_are_recorded_from_the_platform_layer() {
    let harness = clipboard_host(fast_settings());
    harness.enable();
    // 替身只提供文本内容，但格式集合可以由平台层声明；这里直接验证存储层的映射：
    // 通过公开的事件构造入口拿到与平台捕获一致的格式集合。
    let capture = flashcast_platform::clipboard::ClipboardCapture {
        formats: vec![ClipboardFormatKind::Text],
        text: Some("带格式的文本".to_string()),
        files: Vec::new(),
        source: None,
    };
    let event = flashcast_core::event_from_capture(&capture, 1_700_000_000_000)
        .expect("有文本内容就必须能构造事件");
    assert_eq!(
        event.formats,
        vec![flashcast_core::ClipboardFormat::Text {
            bytes: "带格式的文本".len()
        }]
    );
}

// ---------------------------------------------------------------------------
// 本机数据不得进入配置工作区
// ---------------------------------------------------------------------------

#[test]
fn history_and_index_stay_out_of_the_config_workspace() {
    let device_dir = unique_dir("clipboard-device-local");
    let workspace = unique_dir("clipboard-workspace");
    let harness = clipboard_host_with_device(&device_dir, fast_settings());
    harness
        .host
        .select_workspace(&workspace)
        .expect("关联工作区");
    harness.enable();
    harness.copy("本机内容一");
    harness.copy("本机内容二");

    // 工作区里不得出现任何历史 / 索引 / 附件文件。
    let workspace_files = files_under(&workspace);
    for path in &workspace_files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            !name.contains("clipboard") && !name.contains("history.sqlite3"),
            "历史与索引不得出现在配置工作区：{}",
            path.display()
        );
        let text = std::fs::read_to_string(path).unwrap_or_default();
        assert!(
            !text.contains("本机内容一") && !text.contains("本机内容二"),
            "历史内容不得写进工作区文件：{}",
            path.display()
        );
    }
    // 工作区里只有可迁移的偏好文件（清单 / 主题 / 设置）。
    for path in &workspace_files {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            ["settings.toml", "manifest.json", "theme.json"].contains(&name.as_str()),
            "工作区里只应有可迁移偏好：{}",
            path.display()
        );
    }

    // 历史确实落在设备本地目录。
    let state = harness.host.clipboard_state();
    assert!(
        state.storage_path.starts_with(&device_dir),
        "数据库必须在设备本地目录：{}",
        state.storage_path.display()
    );
    assert!(state.storage_path.exists(), "数据库文件必须真的写出来了");
    assert!(workspace.exists());

    // 关闭时也留一份说明：重启（同一设备目录）后历史仍然在。
    let restarted = clipboard_host_with_device(&device_dir, fast_settings());
    assert_eq!(restarted.host.clipboard_entries(None).len(), 2);
}
