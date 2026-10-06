//! 备忘录插件的集成测试（ticket 07）。
//!
//! 全部经由宿主入口（ADR §3 / §10）：`query` / `execute` / `preview` /
//! 备忘录 CRUD / 插件启停 / 工作区重载与监听。工作区是真实的临时目录，
//! 备忘录是磁盘上真实的 Markdown 文件——测试断言的是用户看得到的结果与
//! 工作区内容，不直接调用 `memo` 模块或插件内部的函数。

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use flashcast_core::plugins::memo::memo_item_id;
use flashcast_core::{
    ActionStatus, DefaultAction, ItemKind, MatchTier, MemoError, PluginFailureKind, PluginKind,
    PluginManifestFile, PluginRegistry, Preview, QueryScope, MEMOS_DIR, MEMO_PLUGIN_ID,
};
use flashcast_platform::clipboard::ClipboardError;
use flashcast_platform::fake::{FakeCapabilityProbe, FakeClipboard};
use flashcast_platform::{Capabilities, OsKind, SessionType, Support};

use support::{
    app, cleanup, fast_settings, official_host_restarted, official_host_with_plugins, unique_dir,
    FailingPlugin, FaultyScopePlugin, ScopeFault, StaticPlugin,
};

/// 等待真实文件事件的上限（去抖窗口 500ms，给足余量）。
const WAIT: Duration = Duration::from_secs(6);
/// 「没有发生重载」的观察时长：必须明显长于去抖窗口与每路径静默窗口（600ms）。
const QUIET: Duration = Duration::from_millis(1600);

/// 把最近一次事件轨迹写成断言消息里的一行（与 `workspace_watch` 用例同一手法）。
fn watch_trace(host: &flashcast_core::Host) -> String {
    let describe = |label: &str, trace: Option<flashcast_core::WatchEventTrace>| match trace {
        Some(trace) => format!("{label}：{}", trace.describe()),
        None => format!("{label}：无"),
    };
    format!(
        "{}；{}",
        describe("最后接受的事件", host.last_watch_event()),
        describe("最后的事件决策", host.last_watch_decision())
    )
}

/// 一个已关联真实工作区的宿主，连同它的剪贴板替身与临时目录。
///
/// 临时目录在 `Drop` 里删除：断言失败时也会清理。
struct MemoHost {
    host: flashcast_core::Host,
    clipboard: Arc<FakeClipboard>,
    device: PathBuf,
    parent: PathBuf,
    root: PathBuf,
}

impl MemoHost {
    fn new() -> Self {
        Self::with_plugins_and_capabilities(
            Arc::new(PluginRegistry::new()),
            Arc::new(FakeCapabilityProbe::linux_x11()),
            Arc::new(FakeClipboard::new()),
        )
    }

    fn with_clipboard(clipboard: Arc<FakeClipboard>) -> Self {
        Self::with_plugins_and_capabilities(
            Arc::new(PluginRegistry::new()),
            Arc::new(FakeCapabilityProbe::linux_x11()),
            clipboard,
        )
    }

    fn with_plugins_and_capabilities(
        plugins: Arc<PluginRegistry>,
        capabilities: Arc<dyn flashcast_platform::CapabilityProbe>,
        clipboard: Arc<FakeClipboard>,
    ) -> Self {
        let (host, _launcher, clipboard, device) = official_host_with_plugins(
            vec![app("firefox", "Firefox")],
            fast_settings(),
            plugins,
            capabilities,
            clipboard,
        );
        let parent = unique_dir("memo-ws");
        let root = parent.join("flashcast-config");
        // 初始化真实工作区（含 Git 仓库与 manifest.json），再规范化路径以便比较。
        host.init_workspace(&root).expect("初始化工作区必须成功");
        let root = root.canonicalize().expect("规范化工作区路径");
        Self {
            host,
            clipboard,
            device,
            parent,
            root,
        }
    }

    /// 工作区里某条备忘录的文件。
    fn memo_file(&self, id: &str) -> PathBuf {
        self.root.join(MEMOS_DIR).join(format!("{id}.md"))
    }

    /// 结果里的备忘录条目 id。
    fn item_id(&self, memo_id: &str) -> String {
        memo_item_id(memo_id)
    }
}

impl Drop for MemoHost {
    fn drop(&mut self) {
        cleanup(&self.parent);
        cleanup(&self.device);
    }
}

fn tags(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

fn titles(response: &flashcast_core::QueryResponse) -> Vec<String> {
    response
        .items
        .iter()
        .map(|item| item.title.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 统一插件契约与清单
// ---------------------------------------------------------------------------

/// 备忘录插件由工作区清单加载：声明标识、种类、版本、关键词别名与所需能力。
#[test]
fn memo_plugin_is_loaded_from_the_workspace_manifest() {
    let mh = MemoHost::new();

    let manifests = mh.host.plugin_manifests();
    let (manifest, enabled) = manifests
        .iter()
        .find(|(manifest, _)| manifest.id == MEMO_PLUGIN_ID)
        .expect("清单必须包含备忘录插件");
    assert_eq!(manifest.name, "备忘录");
    assert_eq!(manifest.kind, PluginKind::Feature);
    assert!(!manifest.version.is_empty(), "必须声明版本");
    assert!(enabled, "默认启用");
    assert!(
        manifest.requires("clipboard.write"),
        "必须声明复制所需的 clipboard.write 能力：{:?}",
        manifest.capabilities
    );
    for alias in ["备忘录", "memo", "memos"] {
        assert!(
            manifest.keywords.iter().any(|keyword| keyword == alias),
            "缺少关键词别名「{alias}」：{:?}",
            manifest.keywords
        );
    }

    // 清单文件是唯一权威：磁盘上确实记录了这条功能插件条目。
    let text = fs::read_to_string(mh.root.join("manifest.json")).expect("读取清单文件");
    let on_disk = PluginManifestFile::from_json(&text).expect("清单必须能解析");
    let entry = on_disk.get(MEMO_PLUGIN_ID).expect("清单必须记录备忘录插件");
    assert_eq!(entry.kind, PluginKind::Feature);
    assert_eq!(entry.name, "备忘录");
    assert!(entry.enabled);

    // 停用写进清单后，重启必须仍以清单为准。
    mh.host
        .set_plugin_enabled(MEMO_PLUGIN_ID, false)
        .expect("停用插件");
    let restarted = official_host_restarted(&mh.device, fast_settings());
    assert!(
        !restarted.memo_plugin_enabled(),
        "重启后必须以清单里的停用状态为准"
    );
}

// ---------------------------------------------------------------------------
// 关键词别名与首屏标签
// ---------------------------------------------------------------------------

/// 每个关键词别名都能进入备忘录范围，并列出全部备忘录。
#[test]
fn every_keyword_alias_enters_the_memo_scope() {
    let mh = MemoHost::new();
    let first = mh
        .host
        .create_memo(
            "常用回复",
            &tags(&["回复", "工作"]),
            "收到，我看一下再回复你。",
        )
        .expect("创建备忘录");
    let second = mh
        .host
        .create_memo("会议邀请", &tags(&["会议"]), "下午三点在三楼开会。")
        .expect("创建备忘录");

    let mut expected = vec![mh.item_id(&first.id), mh.item_id(&second.id)];
    expected.sort();

    for alias in ["备忘录", "memo", "memos", "MEMO"] {
        let _ = mh.host.reset_home();
        assert!(mh.host.query(alias).scope.is_home());
        let response = support::plugin_query(&mh.host, alias);
        assert_eq!(
            response.scope,
            QueryScope::Plugin {
                id: MEMO_PLUGIN_ID.to_string(),
                keyword: "备忘录".to_string(),
            },
            "别名「{alias}」必须进入备忘录范围"
        );
        let mut ids: Vec<String> = response.items.iter().map(|item| item.id.clone()).collect();
        ids.sort();
        assert_eq!(ids, expected, "别名「{alias}」应列出全部备忘录");
        assert!(
            response
                .items
                .iter()
                .all(|item| item.kind == ItemKind::Memo),
            "范围里的结果必须是备忘录条目"
        );
        assert!(
            response
                .items
                .iter()
                .all(|item| item.default_action == DefaultAction::Paste),
            "备忘录的默认操作是粘贴"
        );
        assert!(
            response
                .items
                .iter()
                .all(|item| item.source == MEMO_PLUGIN_ID),
            "来源必须是备忘录插件"
        );
        assert!(response.plugin_failures.is_empty());
    }
}

/// 首屏输入标签（完整匹配）直接命中备忘录，并带来源。
#[test]
fn home_tag_search_finds_memos_by_exact_tag() {
    let mh = MemoHost::new();
    let work = mh
        .host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");
    mh.host
        .create_memo("会议邀请", &tags(&["会议"]), "下午三点开会。")
        .expect("创建备忘录");

    let response = mh.host.query("工作");

    assert_eq!(response.scope, QueryScope::Home, "标签命中留在首屏");
    let hit = response
        .items
        .iter()
        .find(|item| item.id == mh.item_id(&work.id))
        .expect("首屏必须给出带来源的备忘录候选");
    assert_eq!(hit.source, MEMO_PLUGIN_ID);
    assert_eq!(hit.score.tier, MatchTier::KeywordOrTagExact);
    assert_eq!(
        hit.subtitle.as_deref(),
        Some("标签：工作"),
        "副标题必须显示标签"
    );

    // 不完整的标签不命中首屏：较弱的匹配留给插件范围，避免噪声。
    let partial = mh.host.query("工");
    assert!(
        !partial
            .items
            .iter()
            .any(|item| item.source == MEMO_PLUGIN_ID),
        "非精确标签不应在首屏命中备忘录"
    );
}

// ---------------------------------------------------------------------------
// 范围搜索：标题、标签、正文
// ---------------------------------------------------------------------------

/// 范围内可按标题、标签与正文检索。
#[test]
fn scope_search_covers_title_tags_and_body() {
    let mh = MemoHost::new();
    mh.host
        .create_memo(
            "常用回复",
            &tags(&["回复", "工作"]),
            "收到，我看一下再回复你。",
        )
        .expect("创建备忘录");
    mh.host
        .create_memo("会议邀请", &tags(&["会议"]), "下午三点在三楼开会。")
        .expect("创建备忘录");

    // 先按关键词进入范围；之后继续在同一个输入框里检索（含关键词前缀）。
    let entered = support::plugin_query(&mh.host, "备忘录");
    assert_eq!(entered.items.len(), 2, "刚进入范围时列出全部备忘录");

    let by_title = support::plugin_query(&mh.host, "备忘录 常用");
    assert_eq!(titles(&by_title), vec!["常用回复"], "标题前缀命中");
    assert_eq!(by_title.items[0].score.tier, MatchTier::TitlePrefix);

    let by_tag = support::plugin_query(&mh.host, "备忘录 工作");
    assert_eq!(titles(&by_tag), vec!["常用回复"], "标签命中");
    assert_eq!(by_tag.items[0].score.tier, MatchTier::MetadataSubstring);

    let by_body = support::plugin_query(&mh.host, "备忘录 三楼");
    assert_eq!(titles(&by_body), vec!["会议邀请"], "正文命中");
    assert_eq!(by_body.items[0].score.tier, MatchTier::MetadataSubstring);

    // 英文别名进入的范围，同样可以继续检索。
    assert_eq!(
        titles(&support::plugin_query(&mh.host, "memos")),
        vec!["常用回复", "会议邀请"]
    );
    let by_alias = support::plugin_query(&mh.host, "memos 常用");
    assert_eq!(titles(&by_alias), vec!["常用回复"]);

    // 完整正文预览：粘贴前要能确认结果。
    let item = by_body.items[0].clone();
    assert_eq!(
        mh.host.preview(&item.id),
        Some(Preview::Text {
            title: Some("会议邀请".to_string()),
            body: "下午三点在三楼开会。".to_string(),
        }),
        "预览必须是完整正文"
    );
}

// ---------------------------------------------------------------------------
// 创建 / 编辑 / 删除 / 重启保留 / 磁盘 Markdown
// ---------------------------------------------------------------------------

/// 创建后重启保留；编辑与删除都落到同一份 Markdown 文件上，标识保持稳定。
#[test]
fn create_edit_delete_persist_as_readable_markdown() {
    let mh = MemoHost::new();
    let memo = mh
        .host
        .create_memo(
            "常用回复",
            &tags(&["回复", "工作"]),
            "收到，我看一下再回复你。",
        )
        .expect("创建备忘录");

    // 磁盘上就是人类可读的 Markdown：front matter + 正文。
    let expected = format!(
        "---\nid: {}\ntitle: 常用回复\ntags: [回复, 工作]\n---\n\n收到，我看一下再回复你。\n",
        memo.id
    );
    assert_eq!(
        fs::read_to_string(mh.memo_file(&memo.id)).expect("备忘录文件必须存在"),
        expected,
        "工作区里的备忘录必须是可读的 Markdown"
    );
    assert!(mh.host.memo_problems().is_empty());

    // 重启（同一设备目录恢复同一工作区）后内容仍在，并且可以搜索。
    let restarted = official_host_restarted(&mh.device, fast_settings());
    let memos = restarted.memos();
    assert_eq!(memos.len(), 1, "重启后必须保留备忘录");
    assert_eq!(memos[0].id, memo.id, "标识必须稳定");
    assert_eq!(memos[0].title, "常用回复");
    assert_eq!(memos[0].tags, tags(&["回复", "工作"]));
    assert_eq!(memos[0].body, "收到，我看一下再回复你。");
    assert_eq!(restarted.query("memo").items.len(), 1);

    // 编辑：标识不变，文件被改写成新的 Markdown。
    let edited = restarted
        .update_memo(
            &memo.id,
            "常用回复（改）",
            &tags(&["回复", "客服"]),
            "改过的正文。",
        )
        .expect("编辑备忘录");
    assert_eq!(edited.id, memo.id);
    let text = fs::read_to_string(mh.memo_file(&memo.id)).expect("读取改写后的文件");
    assert!(
        text.starts_with(&format!("---\nid: {}\n", memo.id)),
        "{text}"
    );
    assert!(text.contains("title: 常用回复（改）\n"), "{text}");
    assert!(text.contains("tags: [回复, 客服]\n"), "{text}");
    assert!(text.ends_with("改过的正文。\n"), "{text}");

    // 编辑后的内容在重启后仍然生效。
    let after_edit = official_host_restarted(&mh.device, fast_settings());
    assert_eq!(after_edit.memos()[0].title, "常用回复（改）");
    assert_eq!(after_edit.memos()[0].tags, tags(&["回复", "客服"]));
    // 首屏按**完整标签**命中；子串留给插件范围。
    assert_eq!(after_edit.query("客服").items.len(), 1);
    assert_eq!(after_edit.query("客").items.len(), 0);
    // 进入范围后可以按标签/标题/正文检索（同一个输入框继续输入）。
    assert_eq!(support::plugin_query(&after_edit, "memo").items.len(), 1);
    assert_eq!(
        support::plugin_query(&after_edit, "memo 客服").items.len(),
        1
    );

    // 删除：文件消失、结果消失，重启后也不会复活。
    after_edit.delete_memo(&memo.id).expect("删除备忘录");
    assert!(
        !mh.memo_file(&memo.id).exists(),
        "删除必须同时删掉工作区里的文件"
    );
    assert!(after_edit.memos().is_empty());
    assert!(support::plugin_query(&after_edit, "memo").items.is_empty());
    assert!(matches!(
        after_edit.delete_memo(&memo.id),
        Err(MemoError::NotFound(_))
    ));
    let after_delete = official_host_restarted(&mh.device, fast_settings());
    assert!(after_delete.memos().is_empty(), "删除必须持久");
}

// ---------------------------------------------------------------------------
// 外部修改
// ---------------------------------------------------------------------------

/// 外部对 `memos/*.md` 的有效修改会被重新加载：既支持普通 Markdown，
/// 也支持带 front matter 的写法；显式重载入口与文件监听两条路径都生效。
#[test]
fn external_valid_edit_of_a_memo_file_is_reloaded() {
    let mh = MemoHost::new();
    let dir = mh.root.join(MEMOS_DIR);
    fs::create_dir_all(&dir).expect("创建备忘录目录");
    let path = dir.join("hand-written.md");
    // 没有 front matter 的普通 Markdown：标题取第一行 `#` 标题。
    fs::write(&path, "# 手写标题\n\n手写的正文。\n").expect("外部写入备忘录");

    let reload = mh.host.reload_workspace();
    assert!(reload.applied, "外部有效修改必须生效：{reload:?}");
    let memos = mh.host.memos();
    assert_eq!(memos.len(), 1);
    assert_eq!(memos[0].id, "hand-written");
    assert_eq!(memos[0].title, "手写标题");
    assert!(memos[0].tags.is_empty());
    assert_eq!(memos[0].body, "手写的正文。");
    assert!(mh.host.memo_problems().is_empty());

    // 先让第一次外部写入的事件流与自读账本收敛。宿主刚刚读过这个文件，账本会把
    // 紧随其后（每路径静默窗口 600ms 内）的重复事件当作噪声吞掉，这是 ticket 05b
    // 自写抑制的设计行为；这里等一个明显长于静默窗口的观察期，再模拟
    // 「过一会儿编辑器又改了它」——那才是真实的用户场景。
    assert!(
        mh.host.wait_for_workspace_change(QUIET).is_none(),
        "读取内容本身不得触发重载（{}）",
        watch_trace(&mh.host)
    );

    // 外部改成带 front matter 的写法：同样生效，并且能按新标签搜到。
    fs::write(
        &path,
        "---\nid: hand-written\ntitle: 改过的标题\ntags: [工作]\n---\n\n新的正文。\n",
    )
    .expect("外部再次写入备忘录");
    let watched = mh
        .host
        .wait_for_workspace_change(WAIT)
        .unwrap_or_else(|| panic!("外部修改必须经文件监听生效（{}）", watch_trace(&mh.host)));
    assert!(watched.applied, "监听路径也必须应用这次修改");
    let memos = mh.host.memos();
    assert_eq!(memos[0].title, "改过的标题");
    assert_eq!(memos[0].tags, tags(&["工作"]));
    assert_eq!(memos[0].body, "新的正文。");
    assert!(
        mh.host
            .query("工作")
            .items
            .iter()
            .any(|item| item.source == MEMO_PLUGIN_ID),
        "外部修改后的标签必须能搜到"
    );
}

/// 无效的外部修改保留可用内容并如实报告原因。
#[test]
fn an_invalid_external_memo_file_is_reported_without_losing_content() {
    let mh = MemoHost::new();
    let good = mh
        .host
        .create_memo("好备忘录", &tags(&["工作"]), "正常内容。")
        .expect("创建备忘录");
    let dir = mh.root.join(MEMOS_DIR);
    fs::write(
        dir.join("broken.md"),
        "---\nid: broken\nunknownKey: 在\n---\n\n正文\n",
    )
    .expect("外部写入无效备忘录");

    let reload = mh.host.reload_workspace();
    assert!(reload.applied, "可用内容的变化仍应生效");

    // 可用的那条原样保留，坏文件不被静默纳入，也不覆盖任何内容。
    let memos = mh.host.memos();
    assert_eq!(memos.len(), 1);
    assert_eq!(memos[0].id, good.id);
    let problems = mh.host.memo_problems();
    assert_eq!(problems.len(), 1, "坏文件必须如实报告");
    assert!(
        problems[0].reason.contains("unknownKey"),
        "原因要指出不认识的键：{}",
        problems[0].reason
    );
    assert!(problems[0].path.ends_with("broken.md"));
}

// ---------------------------------------------------------------------------
// 插件启停
// ---------------------------------------------------------------------------

/// 停用插件后：不贡献首屏标签结果、不再进入范围、拒绝写入；重启仍按清单停用。
#[test]
fn disabling_the_plugin_removes_results_and_refuses_writes() {
    let mh = MemoHost::new();
    mh.host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");
    assert!(
        !mh.host.query("工作").items.is_empty(),
        "启用时首屏应命中标签"
    );
    assert_eq!(support::plugin_query(&mh.host, "备忘录").items.len(), 1);

    mh.host
        .set_plugin_enabled(MEMO_PLUGIN_ID, false)
        .expect("停用备忘录插件");

    assert!(!mh.host.memo_plugin_enabled());
    assert!(!mh.host.plugins().is_enabled(MEMO_PLUGIN_ID));
    assert!(
        !mh.host
            .query("工作")
            .items
            .iter()
            .any(|item| item.source == MEMO_PLUGIN_ID),
        "停用后不得再贡献首屏结果"
    );
    let in_scope = support::plugin_query(&mh.host, "备忘录");
    assert_eq!(in_scope.scope, QueryScope::Home, "停用后关键词不再进入范围");
    assert!(in_scope
        .items
        .iter()
        .all(|item| item.source != MEMO_PLUGIN_ID));
    // 管理入口同样拒绝写入：停用即「不提供该能力」。
    assert!(matches!(
        mh.host.create_memo("新的", &[], "正文"),
        Err(MemoError::PluginDisabled)
    ));
    assert!(matches!(
        mh.host.delete_memo("whatever"),
        Err(MemoError::PluginDisabled)
    ));

    // 清单里的停用状态落到磁盘，重启后依然停用。
    let text = fs::read_to_string(mh.root.join("manifest.json")).expect("读取清单文件");
    let on_disk = PluginManifestFile::from_json(&text).expect("清单必须能解析");
    assert!(!on_disk.get(MEMO_PLUGIN_ID).expect("清单条目").enabled);
    let restarted = official_host_restarted(&mh.device, fast_settings());
    assert!(!restarted.memo_plugin_enabled());
    assert!(
        restarted
            .query("工作")
            .items
            .iter()
            .all(|item| item.source != MEMO_PLUGIN_ID),
        "重启后仍不得贡献结果"
    );
}

// ---------------------------------------------------------------------------
// 失败隔离
// ---------------------------------------------------------------------------

/// 备忘录插件报错不阻断宿主搜索与软件搜索。
#[test]
fn a_failing_memo_plugin_does_not_break_host_search() {
    let plugins = Arc::new(PluginRegistry::new());
    // 同名注册：`register` 保留先注册的那个，因此这里就是「备忘录插件坏了」。
    plugins.register(Arc::new(FailingPlugin::new(MEMO_PLUGIN_ID)));

    let mh = MemoHost::with_plugins_and_capabilities(
        Arc::clone(&plugins),
        Arc::new(FakeCapabilityProbe::linux_x11()),
        Arc::new(FakeClipboard::new()),
    );

    let response = mh.host.query("随便什么");
    let failure = response
        .plugin_failures
        .iter()
        .find(|failure| failure.plugin_id == MEMO_PLUGIN_ID)
        .expect("必须记录备忘录插件的错误");
    assert_eq!(failure.kind, PluginFailureKind::Error);
    assert!(failure.reason.contains("插件内部错误"));

    // 软件搜索不受影响。
    let apps = mh.host.query("fire");
    assert_eq!(titles(&apps), vec!["Firefox"]);
}

/// 备忘录插件的范围搜索挂起时被超时隔离，且不拖慢整轮查询。
#[test]
fn a_hanging_memo_scope_is_timed_out_without_blocking_search() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(FaultyScopePlugin::new(
        MEMO_PLUGIN_ID,
        "备忘录",
        ScopeFault::Hang(Duration::from_millis(1500)),
    )));

    let mh = MemoHost::with_plugins_and_capabilities(
        Arc::clone(&plugins),
        Arc::new(FakeCapabilityProbe::linux_x11()),
        Arc::new(FakeClipboard::new()),
    );

    let started = Instant::now();
    let outcome = mh
        .host
        .execute_plugin_command(&format!("flashcast.plugin.{MEMO_PLUGIN_ID}"));
    assert_eq!(outcome.status, flashcast_core::ActionStatus::Done);
    let response = mh.host.snapshot();
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_millis(1000),
        "插件超时必须生效，实际耗时 {elapsed:?}"
    );
    let failure = response
        .plugin_failures
        .iter()
        .find(|failure| failure.plugin_id == MEMO_PLUGIN_ID)
        .expect("必须记录超时");
    assert_eq!(failure.kind, PluginFailureKind::Timeout);
    assert!(failure.reason.contains("毫秒"));
    assert!(response.items.is_empty());

    // 清空输入回到首屏后，宿主与软件搜索都照常可用。
    assert_eq!(mh.host.reset_home().scope, QueryScope::Home);
    assert_eq!(titles(&mh.host.query("fire")), vec!["Firefox"]);
}

/// 插件范围 panic 被隔离，其他插件与宿主结果照常。
#[test]
fn a_panicking_scope_is_isolated_from_the_memo_plugin() {
    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(FaultyScopePlugin::new(
        "chaos",
        "混乱",
        ScopeFault::Panic,
    )));

    let mh = MemoHost::with_plugins_and_capabilities(
        plugins,
        Arc::new(FakeCapabilityProbe::linux_x11()),
        Arc::new(FakeClipboard::new()),
    );
    mh.host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");

    let panicked = support::plugin_query(&mh.host, "混乱");
    let failure = panicked
        .plugin_failures
        .iter()
        .find(|failure| failure.plugin_id == "chaos")
        .expect("必须记录 panic");
    assert_eq!(failure.kind, PluginFailureKind::Panic);
    assert!(failure.reason.contains("panic"));

    // 备忘录插件不受影响：范围照常进入，结果照常返回。
    assert_eq!(support::plugin_query(&mh.host, "备忘录").items.len(), 1);
    // 清空输入回到首屏后，宿主与软件搜索照常。
    assert_eq!(mh.host.reset_home().scope, QueryScope::Home);
    assert_eq!(titles(&mh.host.query("fire")), vec!["Firefox"]);
}

// ---------------------------------------------------------------------------
// 返回、预览与复制
// ---------------------------------------------------------------------------

/// `back()` 恢复进入备忘录范围之前的查询、范围与选择。
#[test]
fn back_restores_query_selection_and_scope() {
    let mh = MemoHost::new();
    mh.host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");

    mh.host.reset_home();
    assert_eq!(mh.host.set_selection(2).selection, 2);
    let previous = mh.host.snapshot();

    let in_scope = support::plugin_query(&mh.host, "备忘录");
    assert!(matches!(in_scope.scope, QueryScope::Plugin { .. }));
    assert_eq!(in_scope.selection, 0, "进入范围后选择归零");

    let back = mh.host.back();

    assert!(back.restored);
    assert_eq!(back.response.scope, QueryScope::Home, "范围恢复为首屏");
    assert_eq!(back.response.input, previous.input, "查询恢复");
    assert_eq!(back.response.selection, 2, "选择恢复");
    assert_eq!(
        back.response.items, previous.items,
        "恢复后的列表必须与进入范围前一致"
    );

    // 已在最外层时不再恢复，UI 据此关闭窗口。
    assert!(!mh.host.back().restored);
}

/// 备忘录的默认操作是复制：状态为「已复制，需手动粘贴」，反馈准确，
/// 剪贴板里就是正文。
///
/// 本用例从未调用 `Host::set_paste_target`（也就是没有唤起前的应用），因此即使能力
/// 报告说支持自动粘贴，也必须降级为手动粘贴。自动粘贴本身的完整流程（粘贴计划、
/// 关窗后恢复与注入、过期选择与失败降级）在 `tests/paste.rs`。
#[test]
fn executing_a_memo_copies_its_body_and_asks_for_a_manual_paste() {
    let mh = MemoHost::new();
    let memo = mh
        .host
        .create_memo("常用回复", &tags(&["工作"]), "收到，我看一下再回复你。")
        .expect("创建备忘录");
    let item = support::plugin_query(&mh.host, "备忘录")
        .items
        .into_iter()
        .find(|item| item.id == mh.item_id(&memo.id))
        .expect("范围里必须有这条备忘录");

    assert_eq!(item.default_action, DefaultAction::Paste);
    assert_eq!(item.default_action.label_zh(), "粘贴");

    let outcome = mh.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::CopiedNeedsManualPaste);
    let message = outcome.message.expect("复制必须给出反馈");
    assert!(message.contains("已复制"), "{message}");
    assert!(message.contains("手动粘贴"), "{message}");
    assert_eq!(
        mh.clipboard.last_write().as_deref(),
        Some("收到，我看一下再回复你。"),
        "写进剪贴板的必须是正文"
    );
    assert_eq!(mh.clipboard.write_count(), 1);

    // 内容以工作区里当前的内容为准，而不是列表快照。
    mh.host
        .update_memo(&memo.id, "常用回复", &tags(&["工作"]), "改过的正文。")
        .expect("编辑备忘录");
    mh.host.execute(&item);
    assert_eq!(mh.clipboard.last_write().as_deref(), Some("改过的正文。"));
}

/// 剪贴板写入失败时给出准确的中文原因，并且不假装成功。
#[test]
fn a_clipboard_failure_is_reported_accurately() {
    let clipboard = Arc::new(FakeClipboard::always_fails(ClipboardError::ToolMissing {
        reason: "未找到 wl-copy / xclip / xsel".to_string(),
    }));
    let mh = MemoHost::with_clipboard(clipboard);
    let memo = mh
        .host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");
    let item = support::plugin_query(&mh.host, "备忘录").items[0].clone();

    let outcome = mh.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("失败必须给出原因");
    assert!(message.contains("无法复制"), "{message}");
    assert!(message.contains("未找到 wl-copy"), "{message}");
    assert_eq!(mh.clipboard.write_count(), 0);
    assert_eq!(memo.body, "收到。");
}

/// 系统剪贴板不可用时如实报告，不尝试写入。
#[test]
fn an_unsupported_clipboard_refuses_the_copy() {
    let capabilities = Arc::new(FakeCapabilityProbe::new(Capabilities {
        os: OsKind::Linux,
        os_version: Some("Fake Linux".to_string()),
        arch: "x86_64".to_string(),
        session: SessionType::Wayland,
        desktop_available: false,
        hotkey: Support::Unsupported {
            reason: "无桌面会话".to_string(),
        },
        clipboard: Support::Unsupported {
            reason: "当前会话没有可用的剪贴板工具".to_string(),
        },
        auto_paste: Support::Unsupported {
            reason: "无桌面会话".to_string(),
        },
        notes: vec![],
    }));
    let mh = MemoHost::with_plugins_and_capabilities(
        Arc::new(PluginRegistry::new()),
        capabilities,
        Arc::new(FakeClipboard::new()),
    );
    mh.host
        .create_memo("常用回复", &tags(&["工作"]), "收到。")
        .expect("创建备忘录");
    let item = support::plugin_query(&mh.host, "备忘录").items[0].clone();

    let outcome = mh.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("必须给出原因");
    assert!(message.contains("剪贴板不可用"), "{message}");
    assert!(message.contains("没有可用的剪贴板工具"), "{message}");
    assert_eq!(mh.clipboard.write_count(), 0, "不得尝试写入");
}

/// 没有声明 `clipboard.write` 能力的插件拿不到剪贴板：权限校验在原生边界。
#[test]
fn a_plugin_without_the_capability_cannot_write_the_clipboard() {
    let plugins = Arc::new(PluginRegistry::new());
    // 同一契约下的另一个功能插件：贡献一条备忘录条目，但没有声明任何能力。
    // 条目 id 走 `memo:` 前缀（宿主据此还原备忘录），来源是它自己的插件 id。
    plugins.register(Arc::new(StaticPlugin::new(
        "nocap",
        vec![support::item("memo:nocap-1", "无能力条目", "nocap", 50)],
    )));

    let mh = MemoHost::with_plugins_and_capabilities(
        plugins,
        Arc::new(FakeCapabilityProbe::linux_x11()),
        Arc::new(FakeClipboard::new()),
    );
    let item = mh
        .host
        .query("x")
        .items
        .into_iter()
        .find(|item| item.source == "nocap")
        .expect("无能力插件的条目必须在结果里");

    let outcome = mh.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("必须给出原因");
    assert!(message.contains("clipboard.write"), "{message}");
    assert!(message.contains("没有声明"), "{message}");
    assert_eq!(mh.clipboard.write_count(), 0, "不得写入剪贴板");
}

/// 结果来源不在清单里时拒绝执行（防止伪造来源拿到原生能力）。
#[test]
fn an_item_from_an_unknown_source_is_refused() {
    let mh = MemoHost::new();
    let mut item = support::item("memo:forged", "伪造条目", "不存在的插件", 50);
    item.id = "memo:forged".to_string();

    let outcome = mh.host.execute(&item);

    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("必须给出原因");
    assert!(message.contains("不在插件清单里"), "{message}");
    assert_eq!(mh.clipboard.write_count(), 0);
}

/// 未关联工作区时拒绝创建备忘录，并说明原因。
#[test]
fn creating_a_memo_without_a_workspace_is_refused() {
    let (host, _launcher, _clipboard, device) =
        support::official_host(vec![app("firefox", "Firefox")], fast_settings());

    assert!(matches!(
        host.create_memo("标题", &[], "正文"),
        Err(MemoError::NoWorkspace)
    ));
    assert!(host.memos().is_empty());
    cleanup(&device);
}
