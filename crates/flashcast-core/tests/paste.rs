//! 自动粘贴流程的集成测试（ticket 08）。
//!
//! 全部经宿主入口（ADR §3 / §10）：`set_paste_target` → `query` → `execute` →
//! `complete_paste`，断言的是用户看得到的结果（状态与中文反馈）、剪贴板内容、
//! 平台适配层收到的调用。工作区是真实的临时目录，备忘录是磁盘上真实的 Markdown。
//!
//! 这里的焦点、剪贴板与粘贴都是**替身**：替身通过只能证明宿主的决策与顺序正确，
//! 不能证明任何平台真的能注入按键。真实平台的证据由 `flashcast-platform-check`
//! 与各平台 runner 提供，结论写在 ticket 的「未覆盖」清单里。

mod support;

use std::path::PathBuf;
use std::sync::Arc;

use flashcast_core::plugins::memo::memo_item_id;
use flashcast_core::{ActionOutcome, ActionStatus, Host, ItemKind, QueryScope, SearchItem};
use flashcast_platform::fake::{FakeCapabilityProbe, FakeFocusTracker, FakePaster};
use flashcast_platform::{
    Capabilities, CapabilityProbe, FocusError, FocusedApp, OsKind, SessionType, Support,
};

use support::{app, cleanup, fast_settings, official_host_with_paste, unique_dir};

/// 一个已关联真实工作区的宿主，连同自动粘贴的三个替身与临时目录。
struct PasteHost {
    host: Host,
    focus: Arc<FakeFocusTracker>,
    paster: Arc<FakePaster>,
    clipboard: Arc<flashcast_platform::fake::FakeClipboard>,
    device: PathBuf,
    parent: PathBuf,
}

impl PasteHost {
    fn new(capabilities: Arc<dyn flashcast_platform::CapabilityProbe>) -> Self {
        Self::with_adapters(
            capabilities,
            Arc::new(FakeFocusTracker::default()),
            Arc::new(flashcast_platform::fake::FakeClipboard::new()),
            Arc::new(FakePaster::new()),
        )
    }

    /// 焦点与粘贴替身都可用，且粘贴替身能观察剪贴板内容（用于证明粘贴的是哪一条）。
    fn observing(capabilities: Arc<dyn flashcast_platform::CapabilityProbe>) -> Self {
        let clipboard = Arc::new(flashcast_platform::fake::FakeClipboard::new());
        let paster = Arc::new(FakePaster::observing(Arc::clone(&clipboard)));
        Self::with_adapters(
            capabilities,
            Arc::new(FakeFocusTracker::default()),
            clipboard,
            paster,
        )
    }

    fn with_adapters(
        capabilities: Arc<dyn flashcast_platform::CapabilityProbe>,
        focus: Arc<FakeFocusTracker>,
        clipboard: Arc<flashcast_platform::fake::FakeClipboard>,
        paster: Arc<FakePaster>,
    ) -> Self {
        let harness = official_host_with_paste(
            vec![app("firefox", "Firefox")],
            fast_settings(),
            capabilities,
            Arc::clone(&focus),
            Arc::clone(&paster),
            Arc::clone(&clipboard),
        );
        let parent = unique_dir("paste-ws");
        let root = parent.join("flashcast-config");
        harness.host.init_workspace(&root).expect("初始化工作区");
        Self {
            host: harness.host,
            focus,
            paster,
            clipboard: harness.clipboard,
            device: harness.device_dir,
            parent,
        }
    }

    /// 唤起：外壳在显示窗口之前捕获前台应用，并把它交给宿主。
    fn summon(&self, target: &FocusedApp) {
        self.focus.set_active(Some(target.clone()));
        self.host.set_paste_target(Some(target.clone()));
    }

    /// 唤起但拿不到前台应用（Wayland 等）。
    fn summon_without_target(&self) {
        self.focus.set_active(None);
        self.host.set_paste_target(None);
    }

    fn create_memo(&self, title: &str, tags: &[&str], body: &str) -> String {
        let memo = self
            .host
            .create_memo(
                title,
                &tags.iter().map(|t| t.to_string()).collect::<Vec<_>>(),
                body,
            )
            .expect("创建备忘录");
        memo.id
    }

    /// 首屏按标签命中后取回条目（模拟 UI 从列表里拿到的那一项）。
    fn home_item(&self, tag: &str, memo_id: &str) -> SearchItem {
        let response = self.host.query(tag);
        let item = response
            .items
            .iter()
            .find(|item| item.id == memo_item_id(memo_id))
            .expect("标签命中");
        if response.scope.is_home() {
            let first = self.host.execute(item);
            assert_eq!(first.status, ActionStatus::Done);
            assert_eq!(self.clipboard.write_count(), 0, "第一次回车只进入页面");
        }
        self.host
            .snapshot()
            .items
            .into_iter()
            .find(|item| item.id == memo_item_id(memo_id))
            .expect("首屏按标签必须命中该备忘录")
    }

    fn clipboard_text(&self) -> Option<String> {
        self.clipboard.last_write()
    }
}

impl Drop for PasteHost {
    fn drop(&mut self) {
        cleanup(&self.parent);
        cleanup(&self.device);
    }
}

/// 唤起前的目标应用（测试替身里的「上一次前台」）。
fn target_app() -> FocusedApp {
    FocusedApp {
        id: "code".to_string(),
        name: "Visual Studio Code".to_string(),
        wm_class: Some("code".to_string()),
        pid: Some(4242),
        window: Some(7_000_042),
    }
}

/// 另一个应用：用来模拟「焦点恢复后前台变成了别人」。
fn other_app() -> FocusedApp {
    FocusedApp {
        id: "firefox".to_string(),
        name: "Firefox".to_string(),
        wm_class: Some("firefox".to_string()),
        pid: Some(4243),
        window: Some(7_000_043),
    }
}

/// 一个把 `auto_paste` 改成指定状态的能力替身（其余字段与 X11 快照一致）。
fn probe_with_auto_paste(auto_paste: Support) -> Arc<FakeCapabilityProbe> {
    let mut capabilities: Capabilities = FakeCapabilityProbe::linux_x11().probe();
    capabilities.auto_paste = auto_paste;
    Arc::new(FakeCapabilityProbe::new(capabilities))
}

/// 断言「复制 + 手动粘贴」的中文反馈：说清已复制、为什么没有自动粘贴、用户该做什么。
fn assert_manual_paste(outcome: &ActionOutcome, reason_fragment: &str) -> String {
    assert_eq!(
        outcome.status,
        ActionStatus::CopiedNeedsManualPaste,
        "必须是「已复制，需手动粘贴」：{:?}",
        outcome
    );
    let message = outcome.message.clone().expect("必须给出中文反馈");
    assert!(
        message.contains("已复制"),
        "反馈必须先说清已复制：{message}"
    );
    assert!(
        message.contains(reason_fragment),
        "反馈必须说清原因（期望包含「{reason_fragment}」）：{message}"
    );
    assert!(
        message.contains("手动粘贴"),
        "反馈必须告诉用户手动粘贴：{message}"
    );
    message
}

// ---------------------------------------------------------------------------
// 自动粘贴可用：准备剪贴板 → 外壳关窗 → 恢复目标 → 注入
// ---------------------------------------------------------------------------

/// 支持自动粘贴时：`execute` 只准备剪贴板并给出粘贴计划，`complete_paste` 才恢复
/// 目标应用并注入；写进剪贴板与注入粘贴的是同一条正文。
#[test]
fn supported_auto_paste_prepares_clipboard_then_completes_on_the_target() {
    let ph = PasteHost::observing(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "收到，我看一下再回复你。");
    let target = target_app();
    ph.summon(&target);
    let item = ph.home_item("工作", &memo_id);
    assert_eq!(item.kind, ItemKind::Memo);
    assert_eq!(item.source, flashcast_core::MEMO_PLUGIN_ID);

    let outcome = ph.host.execute(&item);

    // 1. 只准备了剪贴板，还没有注入任何按键。
    assert_eq!(outcome.status, ActionStatus::PastePending);
    let plan = outcome.paste.clone().expect("必须给出粘贴计划");
    assert_eq!(plan.target.id, target.id, "目标必须是唤起前的应用");
    assert_eq!(plan.text_bytes, "收到，我看一下再回复你。".len());
    assert_eq!(
        ph.clipboard_text().as_deref(),
        Some("收到，我看一下再回复你。")
    );
    assert_eq!(ph.paster.paste_count(), 0, "关窗之前不得注入按键");
    assert!(ph.host.has_pending_paste());

    // 2. 外壳关窗之后完成粘贴。
    let outcome = ph.host.complete_paste();

    assert_eq!(outcome.status, ActionStatus::Done);
    let message = outcome.message.expect("成功也要给出反馈");
    assert!(message.contains("已粘贴"), "{message}");
    assert!(message.contains("Visual Studio Code"), "{message}");
    assert_eq!(
        ph.focus.restored(),
        vec![target.clone()],
        "必须恢复目标应用"
    );
    assert_eq!(ph.paster.paste_count(), 1, "必须恰好注入一次粘贴");
    assert_eq!(
        ph.paster.text_at_paste(),
        vec![Some("收到，我看一下再回复你。".to_string())],
        "注入粘贴时剪贴板里必须是这条正文"
    );
    assert!(!ph.host.has_pending_paste(), "完成后不留待完成计划");
}

/// 重复完成不会二次注入：第一次成功后计划已被消费。
#[test]
fn completing_twice_never_injects_twice() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    let target = target_app();
    ph.summon(&target);
    let item = ph.home_item("工作", &memo_id);

    ph.host.execute(&item);
    assert_eq!(ph.host.complete_paste().status, ActionStatus::Done);
    let second = ph.host.complete_paste();

    assert_eq!(second.status, ActionStatus::Failed);
    assert_eq!(ph.paster.paste_count(), 1, "第二次不得再注入");
}

/// 新的唤起会作废旧计划：上一次未完成的粘贴不会落到这一次的目标上。
#[test]
fn a_new_summon_discards_the_previous_plan() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    let target = target_app();
    ph.summon(&target);
    let item = ph.home_item("工作", &memo_id);
    assert_eq!(ph.host.execute(&item).status, ActionStatus::PastePending);

    // 用户没有粘贴就关掉窗口，然后重新唤起（目标换成了另一个应用）。
    let second_target = other_app();
    ph.summon(&second_target);
    let stale = ph.host.complete_paste();

    assert_eq!(stale.status, ActionStatus::Failed);
    assert_eq!(ph.paster.paste_count(), 0, "过期计划绝不能注入");
    assert!(
        ph.focus.restored().is_empty(),
        "过期计划也不应该去恢复旧目标：{:?}",
        ph.focus.restored()
    );
}

/// 关闭浮窗但没执行粘贴（Escape / 失焦）时调用 `cancel_paste`：之后即使有迟到的
/// 完成请求也不会注入。
#[test]
fn cancel_paste_makes_a_late_completion_a_noop() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    let target = target_app();
    ph.summon(&target);
    let item = ph.home_item("工作", &memo_id);
    ph.host.execute(&item);

    ph.host.cancel_paste();
    let outcome = ph.host.complete_paste();

    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(!ph.host.has_pending_paste());
    assert_eq!(ph.paster.paste_count(), 0);
}

// ---------------------------------------------------------------------------
// 降级为手动粘贴的每一种条件
// ---------------------------------------------------------------------------

/// Wayland：平台明确不支持自动粘贴，必须复制并提示手动粘贴。
#[test]
fn wayland_session_falls_back_to_manual_paste() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_wayland()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "收到，我看一下再回复你。");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);

    let outcome = ph.host.execute(&item);

    let message = assert_manual_paste(&outcome, "Wayland");
    assert!(outcome.paste.is_none(), "不支持时不得给出粘贴计划");
    assert_eq!(
        ph.clipboard_text().as_deref(),
        Some("收到，我看一下再回复你。"),
        "即使不能自动粘贴，内容也必须已经复制"
    );
    assert!(!ph.host.has_pending_paste());
    assert!(
        message.contains("Ctrl+V") || message.contains("Cmd+V"),
        "{message}"
    );

    // 之后任何完成请求都不会注入。
    assert_eq!(ph.host.complete_paste().status, ActionStatus::Failed);
    assert_eq!(ph.paster.paste_count(), 0);
}

/// 没有捕获到唤起前的应用：无法确定目标，绝不能猜一个。
#[test]
fn missing_target_falls_back_to_manual_paste() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon_without_target();
    let item = ph.home_item("工作", &memo_id);

    let outcome = ph.host.execute(&item);

    assert_manual_paste(&outcome, "没有记录到唤起前的应用");
    assert!(outcome.paste.is_none());
    assert_eq!(ph.clipboard_text().as_deref(), Some("正文"));
    assert_eq!(ph.paster.paste_count(), 0);
}

/// 能力状态是「无法判定」时同样不能自动粘贴：未覆盖不等于可用。
#[test]
fn unknown_auto_paste_capability_falls_back_to_manual_paste() {
    let ph = PasteHost::new(probe_with_auto_paste(Support::Unknown {
        reason: "无法确定会话类型".to_string(),
    }));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);

    let outcome = ph.host.execute(&item);

    assert_manual_paste(&outcome, "无法确认");
    assert_eq!(ph.paster.paste_count(), 0);
}

/// 目标应用已退出：恢复焦点失败，降级为手动粘贴，剪贴板内容保持不变。
#[test]
fn gone_target_app_falls_back_to_manual_paste() {
    let focus = Arc::new(FakeFocusTracker::restore_fails(FocusError::Unavailable {
        reason: "唤起前的应用（Visual Studio Code）已退出，无法恢复焦点".to_string(),
    }));
    let clipboard = Arc::new(flashcast_platform::fake::FakeClipboard::new());
    let paster = Arc::new(FakePaster::observing(Arc::clone(&clipboard)));
    let ph = PasteHost::with_adapters(
        Arc::new(FakeCapabilityProbe::linux_x11()),
        focus,
        clipboard,
        paster,
    );
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);
    // 支持自动粘贴，因此先给出计划。
    assert_eq!(ph.host.execute(&item).status, ActionStatus::PastePending);

    let outcome = ph.host.complete_paste();

    assert_manual_paste(&outcome, "无法把焦点还给");
    assert_eq!(ph.paster.paste_count(), 0, "目标已经不在，绝不能注入");
    assert_eq!(
        ph.clipboard_text().as_deref(),
        Some("正文"),
        "降级之后内容仍必须在剪贴板里"
    );
}

/// 焦点恢复后前台不是唤起前的应用：绝不注入（spec 明确禁止粘贴到别的应用）。
#[test]
fn never_injects_when_the_restored_foreground_is_another_app() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);
    assert_eq!(ph.host.execute(&item).status, ActionStatus::PastePending);

    // 窗口管理器把焦点给了别人（用户切走了，或恢复没有被接受）。
    ph.focus.set_active(Some(other_app()));
    let outcome = ph.host.complete_paste();

    assert_manual_paste(&outcome, "不是唤起前的");
    assert_eq!(ph.paster.paste_count(), 0, "前台不是目标时绝不能注入");
    assert_eq!(ph.clipboard_text().as_deref(), Some("正文"));
}

/// 注入粘贴本身失败（例如权限在运行中被收回）：内容留在剪贴板，提示手动粘贴。
#[test]
fn injection_failure_leaves_the_content_for_a_manual_paste() {
    let clipboard = Arc::new(flashcast_platform::fake::FakeClipboard::new());
    let paster = Arc::new(FakePaster::with_failures(vec![
        flashcast_platform::PasteError::PermissionMissing {
            reason: "辅助功能权限已被撤销".to_string(),
        },
    ]));
    let ph = PasteHost::with_adapters(
        Arc::new(FakeCapabilityProbe::linux_x11()),
        Arc::new(FakeFocusTracker::default()),
        clipboard,
        paster,
    );
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);
    assert_eq!(ph.host.execute(&item).status, ActionStatus::PastePending);

    let outcome = ph.host.complete_paste();

    assert_manual_paste(&outcome, "自动粘贴没有成功");
    assert_eq!(ph.paster.paste_count(), 1, "失败也要计入「尝试过」");
    assert_eq!(ph.clipboard_text().as_deref(), Some("正文"));
}

// ---------------------------------------------------------------------------
// 过期选择：快速切换 / 连续执行不得粘贴上一次的选择
// ---------------------------------------------------------------------------

/// 连续执行两条备忘录：只有最后一条会被粘贴，剪贴板与注入内容都必须是它。
#[test]
fn rapid_executes_only_paste_the_last_selection() {
    let ph = PasteHost::observing(Arc::new(FakeCapabilityProbe::linux_x11()));
    let first = ph.create_memo("回复甲", &["工作"], "甲的内容");
    let second = ph.create_memo("回复乙", &["工作"], "乙的内容");
    ph.summon(&target_app());
    let item_first = ph.home_item("工作", &first);
    let item_second = ph.home_item("工作", &second);

    let first_outcome = ph.host.execute(&item_first);
    let first_epoch = first_outcome.paste.clone().expect("第一条应有计划").epoch;
    assert_eq!(ph.clipboard_text().as_deref(), Some("甲的内容"));

    let second_outcome = ph.host.execute(&item_second);
    let second_epoch = second_outcome.paste.clone().expect("第二条应有计划").epoch;
    assert!(
        second_epoch > first_epoch,
        "计划序号必须单调递增：{first_epoch} -> {second_epoch}"
    );
    assert_eq!(
        ph.clipboard_text().as_deref(),
        Some("乙的内容"),
        "切换结果后剪贴板必须是新选中的那条"
    );

    // 外壳只完成一次粘贴：必须是最后一次执行的那条。
    let outcome = ph.host.complete_paste();

    assert_eq!(outcome.status, ActionStatus::Done);
    let message = outcome.message.expect("成功反馈");
    assert!(message.contains("回复乙"), "{message}");
    assert!(!message.contains("回复甲"), "不得粘贴上一条：{message}");
    assert_eq!(ph.paster.paste_count(), 1);
    assert_eq!(
        ph.paster.text_at_paste(),
        vec![Some("乙的内容".to_string())],
        "注入粘贴时剪贴板里必须是最后执行的那条"
    );
}

/// 列表里拿到的是**旧快照**，但内容必须在执行时按当前工作区内容重新读取。
#[test]
fn execute_uses_the_current_memo_body_not_the_list_snapshot() {
    let ph = PasteHost::observing(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "旧正文");
    ph.summon(&target_app());
    let stale_item = ph.home_item("工作", &memo_id);

    // 用户在执行之前编辑了这条备忘录（列表还没有刷新）。
    ph.host
        .update_memo(
            &memo_id,
            "常用回复",
            &["工作".to_string()],
            "新正文（执行时读取）",
        )
        .expect("编辑备忘录");

    let outcome = ph.host.execute(&stale_item);

    assert_eq!(outcome.status, ActionStatus::PastePending);
    assert_eq!(
        ph.clipboard_text().as_deref(),
        Some("新正文（执行时读取）"),
        "写进剪贴板的必须是当前正文，而不是列表快照"
    );
    ph.host.complete_paste();
    assert_eq!(
        ph.paster.text_at_paste(),
        vec![Some("新正文（执行时读取）".to_string())]
    );
}

/// 已删除的备忘录不会把旧内容写进剪贴板，也不会产生粘贴计划。
#[test]
fn deleted_memo_never_reaches_the_clipboard() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let memo_id = ph.create_memo("常用回复", &["工作"], "正文");
    ph.summon(&target_app());
    let item = ph.home_item("工作", &memo_id);
    ph.host.delete_memo(&memo_id).expect("删除备忘录");

    let outcome = ph.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(
        outcome.message.expect("失败原因").contains("找不到"),
        "必须说明原因"
    );
    assert_eq!(ph.clipboard.write_count(), 0, "不得写入任何内容");
    assert!(outcome.paste.is_none());
    assert!(!ph.host.has_pending_paste());
    assert_eq!(ph.paster.paste_count(), 0);
}

/// 首屏按标签命中多条备忘录时全部列出，来源与默认操作都正确（ticket 08 第 6 条）。
#[test]
fn tag_hits_list_every_memo_with_its_source() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    let first = ph.create_memo("回复甲", &["工作", "回复"], "甲");
    let second = ph.create_memo("回复乙", &["工作"], "乙");
    ph.create_memo("别的", &["私人"], "丙");

    let response = ph.host.query("工作");
    assert_eq!(response.scope, QueryScope::Home, "标签命中必须留在首屏");
    let items: Vec<&SearchItem> = response
        .items
        .iter()
        .filter(|item| item.kind == ItemKind::Memo)
        .collect();
    assert_eq!(items.len(), 2, "同一标签下的两条都必须列出：{:?}", items);
    for item in &items {
        assert_eq!(item.source, flashcast_core::MEMO_PLUGIN_ID);
        assert_eq!(item.default_action, flashcast_core::DefaultAction::Paste);
        assert_eq!(
            item.score.tier,
            flashcast_core::MatchTier::KeywordOrTagExact
        );
    }
    let ids: Vec<String> = items.iter().map(|item| item.id.clone()).collect();
    assert!(ids.contains(&memo_item_id(&first)));
    assert!(ids.contains(&memo_item_id(&second)));

    // 选中任意一条都能执行：宿主持有的选择可以移动。
    let moved = ph.host.move_selection(1);
    assert!(moved.selection <= 1);
}

/// 首屏标签匹配是**完整相等**：非完整标签不命中（与 ticket 07 的约定一致）。
#[test]
fn tag_search_requires_the_complete_tag() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    ph.create_memo("回复甲", &["工作回复"], "甲");

    let response = ph.host.query("工作");

    assert!(
        response
            .items
            .iter()
            .all(|item| item.kind != ItemKind::Memo),
        "非完整标签不得命中：{:?}",
        response.items
    );
}

/// 目标应用的显示名可能为空（macOS 上取不到本地化名时）：反馈里不能出现空引号。
#[test]
fn manual_paste_message_uses_the_right_modifier_key_per_os() {
    assert!(flashcast_platform::manual_paste_hint(OsKind::Macos).contains("Cmd+V"));
    assert!(flashcast_platform::manual_paste_hint(OsKind::Windows).contains("Ctrl+V"));
    assert!(flashcast_platform::manual_paste_hint(OsKind::Linux).contains("Ctrl+V"));
    let message = flashcast_platform::manual_paste_message("标题", OsKind::Macos, Some("没有权限"));
    assert!(message.contains("已复制「标题」"), "{message}");
    assert!(message.contains("没有权限"), "{message}");
    assert!(message.contains("Cmd+V"), "{message}");
    // 会话类型只影响 Linux 的措辞，不能出现在 macOS 的提示里。
    assert!(!message.contains("Wayland"));
    let _ = SessionType::Wayland;
}

// ---------------------------------------------------------------------------
// 关键词与标签冲突：插件入口与备忘录候选同时保留（ADR §4）
// ---------------------------------------------------------------------------

/// 输入正好等于插件关键词、同时又有备忘录带这个标签时：首屏**同时**给出插件入口与
/// 标签命中的备忘录；执行入口进入范围，选择备忘录仍然可以直接粘贴。
#[test]
fn keyword_and_tag_collision_keeps_both_sides() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    // 一条备忘录的标签与插件关键词「memo」完全相同：这就是冲突场景。
    let tagged = ph.create_memo("命名风波", &["memo"], "标签与关键词同名");
    let other = ph.create_memo("无关", &["工作"], "另一条");

    let response = ph.host.query("memo");

    assert_eq!(
        response.scope,
        QueryScope::Home,
        "冲突时必须留在首屏，否则标签命中会消失"
    );
    let entry_id = format!(
        "{}{}",
        flashcast_core::PLUGIN_ENTRY_PREFIX,
        flashcast_core::MEMO_PLUGIN_ID
    );
    let entry = response
        .items
        .iter()
        .find(|item| item.id == entry_id)
        .expect("必须给出插件入口条目");
    assert_eq!(entry.kind, ItemKind::Command);
    assert_eq!(entry.default_action, flashcast_core::DefaultAction::Open);
    assert_eq!(entry.title, "备忘录");
    assert_eq!(
        response.items.first().map(|item| item.id.clone()),
        Some(memo_item_id(&tagged)),
        "标签命中的备忘录必须排在最前：直接回车粘贴正文"
    );
    let memo_item = response
        .items
        .iter()
        .find(|item| item.id == memo_item_id(&tagged))
        .expect("标签命中的备忘录不得被关键词吞掉");
    assert_eq!(memo_item.kind, ItemKind::Memo);
    assert_eq!(memo_item.source, flashcast_core::MEMO_PLUGIN_ID);
    assert_eq!(
        memo_item.default_action,
        flashcast_core::DefaultAction::Paste
    );
    assert!(
        !response
            .items
            .iter()
            .any(|item| item.id == memo_item_id(&other)),
        "只有标签命中的那条参与首屏结果"
    );

    // 选择插件入口并回车：进入插件范围，范围内列出全部备忘录。
    let outcome = ph.host.execute(&entry);
    assert_eq!(outcome.status, ActionStatus::Done);
    let scope = ph.host.snapshot();
    assert_eq!(
        scope.scope,
        QueryScope::Plugin {
            id: flashcast_core::MEMO_PLUGIN_ID.to_string(),
            keyword: "备忘录".to_string(),
        },
        "执行入口必须真的进入范围"
    );
    assert!(scope.items.iter().all(|item| item.kind == ItemKind::Memo));
    assert!(scope
        .items
        .iter()
        .any(|item| item.id == memo_item_id(&tagged)));
    assert!(scope
        .items
        .iter()
        .any(|item| item.id == memo_item_id(&other)));

    // UI 在执行命令条目后会按当前输入重新查询：这一次不能又被弹回首屏。
    let again = ph.host.query("memo");
    assert_eq!(
        again.scope,
        QueryScope::Plugin {
            id: flashcast_core::MEMO_PLUGIN_ID.to_string(),
            keyword: "备忘录".to_string(),
        },
        "已经在范围内时，同样的输入必须留在范围内"
    );

    // 从入口进入范围之后仍可返回首屏，并且首屏的冲突结果还在。
    let back = ph.host.back();
    assert!(back.restored, "应能返回首屏");
    assert_eq!(back.response.scope, QueryScope::Home);
}

/// 没有冲突时，输入完整关键词仍然**直接**进入插件范围（ticket 07 的行为不能被改坏）。
#[test]
fn exact_keyword_without_collision_requires_an_explicit_command() {
    let ph = PasteHost::new(Arc::new(FakeCapabilityProbe::linux_x11()));
    ph.create_memo("无关", &["工作"], "正文");
    let response = ph.host.query("memo");
    assert!(response.scope.is_home());
    let entry = response
        .items
        .iter()
        .find(|i| i.id == "flashcast.plugin.memo")
        .unwrap();
    assert_eq!(ph.host.execute(entry).status, ActionStatus::Done);
    let page = ph.host.snapshot();
    assert!(!page.scope.is_home());
    assert_eq!(page.input, "");
    assert!(page.items.iter().all(|i| i.kind == ItemKind::Memo));
}

#[test]
fn colliding_tag_hit_can_still_be_pasted() {
    let ph = PasteHost::observing(Arc::new(FakeCapabilityProbe::linux_x11()));
    let tagged = ph.create_memo("命名风波", &["memo"], "标签与关键词同名");
    ph.summon(&target_app());

    let response = ph.host.query("memo");
    let item = response
        .items
        .iter()
        .find(|item| item.id == memo_item_id(&tagged))
        .expect("标签命中必须可选")
        .clone();

    assert_eq!(ph.host.execute(&item).status, ActionStatus::Done);
    assert_eq!(ph.clipboard.write_count(), 0);
    let outcome = ph.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::PastePending);
    assert_eq!(ph.clipboard_text().as_deref(), Some("标签与关键词同名"));
    assert_eq!(ph.host.complete_paste().status, ActionStatus::Done);
}
