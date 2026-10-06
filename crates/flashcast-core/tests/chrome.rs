//! Chrome 书签的集成测试（ticket 13）。全部经由宿主入口
//! （`Host::query` / `Host::execute` / `Host::chrome_state` / `Host::associate_chrome_profile`）。
//!
//! **夹具是自己写的临时 profile**：一个假的 Chrome 可执行文件、一份 `Local State`
//! （`profile.info_cache`）与两份 `Bookmarks` 样本。发现逻辑走
//! `flashcast_platform::chrome` 的真实实现（存在性检查 + 真实 JSON 解析）；
//! 只有**进程启动**被换成记录 argv 的替身，因此这里可以精确断言交给 Chrome 的参数
//! 向量，而不会真的打开浏览器、也不会碰到用户自己的 Chrome profile。
//!
//! 范围说明：本文件**不**证明「页面真的打开了」。真实 Chrome 的启动检查在 ticket 13 的
//! Comments 里单独记录，并区分「参数向量已断言 / 进程已启动 / 页面已打开」。

mod support;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flashcast_core::{
    ActionStatus, DefaultAction, Host, ItemKind, MatchTier, PluginRegistry, Settings,
};
use flashcast_platform::chrome::{
    BinaryCandidate, ChromeBrand, ChromeProvider, UserDataCandidate, UserDataOrigin,
};
use flashcast_platform::fake::FakeChrome;
use support::{fast_settings, files_under, official_host_with_chrome, unique_dir};

/// 一份 `Local State`：两个 profile，其一是企业管理的「工作」。
const LOCAL_STATE: &str = r#"{
   "os_crypt": { "encrypted_key": "测试不会读取它" },
   "profile": {
      "info_cache": {
         "Default": { "name": "个人", "user_name": "me@example.com" },
         "Profile 1": {
            "name": "工作",
            "is_managed": true,
            "hosted_domain": "corp.example"
         }
      }
   }
}"#;

/// 默认 profile 的书签：目录嵌套、非 ASCII 标题、带查询串的 URL。
const DEFAULT_BOOKMARKS: &str = r#"{
   "checksum": "永不校验",
   "checksum_sha256": "永不校验",
   "roots": {
      "bookmark_bar": {
         "children": [
            { "id": "7", "name": "Rust 官网", "type": "url", "url": "https://www.rust-lang.org/" },
            { "id": "8", "name": "开发", "type": "folder", "children": [
               { "id": "9", "name": "Rust 文档", "type": "url", "url": "https://doc.rust-lang.org/std/?q=中文&x=1" }
            ] }
         ],
         "id": "1", "name": "书签栏", "type": "folder"
      },
      "other": {
         "children": [
            { "id": "10", "name": "内网登录", "type": "url", "url": "https://intranet.example.com/login?token=abc&next=首页" },
            { "id": "11", "name": "分析工具", "type": "url", "url": "https://rust-analyzer.github.io/" }
         ],
         "id": "2", "name": "其他书签", "type": "folder"
      },
      "synced": { "children": [], "id": "3", "name": "移动设备书签", "type": "folder" }
   },
   "sync_metadata": "忽略",
   "version": 1
}"#;

/// 工作 profile 的书签：与默认 profile 完全不同，用于验证「换关联后检索跟着换」。
const WORK_BOOKMARKS: &str = r#"{
   "roots": {
      "bookmark_bar": {
         "children": [
            { "id": "20", "name": "工作台", "type": "url", "url": "https://work.example.com/dashboard" }
         ],
         "id": "1", "name": "书签栏", "type": "folder"
      },
      "other": { "children": [], "id": "2", "name": "其他书签", "type": "folder" },
      "synced": { "children": [], "id": "3", "name": "移动设备书签", "type": "folder" }
   },
   "version": 1
}"#;

/// 一份临时 Chrome profile 夹具。
struct Fixture {
    root: PathBuf,
    /// 用户数据目录。
    udd: PathBuf,
    binary: PathBuf,
}

impl Fixture {
    fn new(prefix: &str) -> Self {
        let root = unique_dir(prefix);
        let binary = root.join("bin/google-chrome");
        write(&binary, "#!/bin/sh\nexit 0\n");
        let udd = root.join("udd");
        write(&udd.join("Local State"), LOCAL_STATE);
        write(&udd.join("Default").join("Bookmarks"), DEFAULT_BOOKMARKS);
        write(&udd.join("Profile 1").join("Bookmarks"), WORK_BOOKMARKS);
        Self { root, udd, binary }
    }

    /// 默认可执行文件 + 默认用户数据目录（Default origin，不需要 --user-data-dir）。
    fn candidates(&self) -> (Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
        (
            vec![BinaryCandidate::new(
                ChromeBrand::Chrome,
                self.binary.clone(),
            )],
            vec![UserDataCandidate::new(
                ChromeBrand::Chrome,
                self.udd.clone(),
                UserDataOrigin::Default,
            )],
        )
    }

    /// 自定义用户数据目录（Custom origin，必须显式传 --user-data-dir）。
    fn custom_candidates(&self) -> (Vec<BinaryCandidate>, Vec<UserDataCandidate>) {
        (
            vec![BinaryCandidate::new(
                ChromeBrand::Chrome,
                self.binary.clone(),
            )],
            vec![UserDataCandidate::new(
                ChromeBrand::Chrome,
                self.udd.clone(),
                UserDataOrigin::Custom,
            )],
        )
    }

    /// 走真实发现的 Chrome 替身。
    fn chrome(&self) -> Arc<FakeChrome> {
        let (binaries, user_data) = self.candidates();
        Arc::new(FakeChrome::from_candidates(binaries, user_data))
    }

    fn chrome_custom(&self) -> Arc<FakeChrome> {
        let (binaries, user_data) = self.custom_candidates();
        Arc::new(FakeChrome::from_candidates(binaries, user_data))
    }

    fn bookmarks_path(&self, profile: &str) -> PathBuf {
        self.udd.join(profile).join("Bookmarks")
    }

    fn write_bookmarks(&self, profile: &str, text: &str) {
        write(&self.bookmarks_path(profile), text);
    }

    /// 断言宿主没有往 profile 目录里写任何东西：目录内容仍是夹具自己放的那些。
    fn assert_untouched(&self, profile: &str, expected: &[&str]) {
        let mut names: Vec<String> = std::fs::read_dir(self.udd.join(profile))
            .expect("profile 目录必须还能列出")
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        let mut expected: Vec<String> = expected.iter().map(|name| name.to_string()).collect();
        expected.sort();
        assert_eq!(names, expected, "宿主不得增删 profile 目录里的文件");
    }

    fn remove_profile(&self, profile: &str) {
        std::fs::remove_dir_all(self.udd.join(profile)).expect("删除 profile 目录");
    }

    fn cleanup(&self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn write(path: &Path, text: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("创建夹具父目录");
    }
    std::fs::write(path, text).expect("写入夹具文件");
}

/// 关联前的干净宿主：Chrome 已安装、profile 已发现，但还没选择 profile。
fn host_with_fixture(prefix: &str) -> (Host, Arc<FakeChrome>, PathBuf, Fixture) {
    let fixture = Fixture::new(prefix);
    let chrome = fixture.chrome();
    let (host, chrome, device_dir) = official_host_with_chrome(Vec::new(), fast_settings(), chrome);
    (host, chrome, device_dir, fixture)
}

/// 已关联 Default profile 的宿主。
fn linked_host(prefix: &str) -> (Host, Arc<FakeChrome>, PathBuf, Fixture) {
    let (host, chrome, device_dir, fixture) = host_with_fixture(prefix);
    host.associate_chrome_profile("Default")
        .expect("关联 Default profile 必须成功");
    (host, chrome, device_dir, fixture)
}

fn titles(items: &[flashcast_core::SearchItem]) -> Vec<String> {
    items.iter().map(|item| item.title.clone()).collect()
}

/// 进入插件范围并检索。
///
/// 与真实 UI 的输入顺序一致：用户先输入完整关键词进入范围，再继续输入查询
/// （关键词必须**完整匹配**才进入范围，见 `PluginManifest::matches_keyword`）。
fn search(host: &Host, query: &str) -> flashcast_core::QueryResponse {
    support::plugin_query(&host, "chrome bookmarks");
    support::plugin_query(&host, &format!("chrome bookmarks {query}"))
}

// ---------------------------------------------------------------------------
// 插件清单与入口
// ---------------------------------------------------------------------------

#[test]
fn plugin_is_loaded_from_the_manifest_with_both_aliases_and_its_capability() {
    let (host, _chrome, _device, fixture) = host_with_fixture("manifest");

    let manifests = host.plugin_manifests();
    let entry = manifests
        .iter()
        .find(|(manifest, _)| manifest.id == "chrome-bookmarks")
        .expect("Chrome 书签插件必须出现在功能插件清单里");
    assert_eq!(entry.0.name, "Chrome 书签");
    assert_eq!(
        entry.0.keywords,
        vec![
            "chrome bookmarks",
            "chrome 书签",
            "bookmark",
            "bookmarks",
            "书签"
        ]
        .into_iter()
        .map(String::from)
        .collect::<Vec<_>>(),
        "中英文两个关键词别名都要声明"
    );
    assert!(
        entry.0.capabilities.iter().any(|cap| cap == "chrome.open"),
        "必须声明 chrome.open 能力：{:?}",
        entry.0.capabilities
    );
    assert!(entry.1, "插件默认启用");
    assert!(
        host.manifest_entries()
            .iter()
            .any(|entry| entry.id == "chrome-bookmarks" && entry.enabled),
        "清单文件里也要有这条记录"
    );
    fixture.cleanup();
}

#[test]
fn both_keyword_aliases_enter_the_scope_but_home_never_searches_bookmarks() {
    let (host, _chrome, _device, fixture) = linked_host("aliases");

    for alias in ["chrome bookmarks", "chrome 书签"] {
        let _ = host.reset_home();
        let home = host.query(alias);
        assert!(home.scope.is_home());
        let response = support::plugin_query(&host, alias);
        assert_eq!(
            response.scope.label_zh(),
            "chrome bookmarks 范围".to_string(),
            "完整匹配 {alias} 必须进入插件范围"
        );
        assert!(
            response
                .items
                .iter()
                .all(|item| item.kind == ItemKind::Bookmark),
            "范围内只应有书签条目"
        );
        assert_eq!(response.items.len(), 4, "进入范围先列出全部书签");
    }

    // 首屏不检索书签：先清空输入回到首屏，再输入书签标题的片段。
    host.reset_home();
    let home = host.query("rust");
    assert!(home.scope.is_home());
    assert!(
        home.items
            .iter()
            .all(|item| item.kind != ItemKind::Bookmark),
        "首屏不应该出现书签：{:?}",
        titles(&home.items)
    );
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 检索与排序
// ---------------------------------------------------------------------------

#[test]
fn scope_search_matches_title_url_and_folder() {
    let (host, _chrome, _device, fixture) = linked_host("search");

    let by_title = search(&host, "rust 文档");
    assert!(
        by_title.plugin_failures.is_empty(),
        "{:?}",
        by_title.plugin_failures
    );
    assert_eq!(titles(&by_title.items), vec!["Rust 文档"]);

    let by_url = search(&host, "doc.rust-lang.org");
    assert_eq!(titles(&by_url.items), vec!["Rust 文档"], "按网址命中");

    let by_folder = search(&host, "开发");
    assert_eq!(titles(&by_folder.items), vec!["Rust 文档"], "按目录命中");

    let non_ascii_url = search(&host, "q=中文");
    assert_eq!(
        titles(&non_ascii_url.items),
        vec!["Rust 文档"],
        "非 ASCII 查询串也要能匹配网址"
    );

    let none = search(&host, "完全不存在的词");
    assert!(none.items.is_empty());
    fixture.cleanup();
}

#[test]
fn ranking_prefers_title_matches_over_url_matches() {
    let (host, _chrome, _device, fixture) = linked_host("ranking");

    let response = search(&host, "rust");
    let titles = titles(&response.items);
    assert_eq!(
        titles,
        vec!["Rust 官网", "Rust 文档", "分析工具"],
        "标题匹配（层级更高）必须排在只有网址匹配的条目之前"
    );
    assert_eq!(response.items[2].score.tier, MatchTier::MetadataSubstring);
    assert_eq!(response.items[0].score.tier, MatchTier::TitlePrefix);

    // 每条结果都带网址与目录，默认操作是在 Chrome 打开。
    let first = &response.items[0];
    assert_eq!(first.kind, ItemKind::Bookmark);
    assert_eq!(first.default_action, DefaultAction::OpenInChrome);
    assert_eq!(first.default_action.label_zh(), "在 Chrome 打开");
    let subtitle = first.subtitle.clone().expect("结果必须带副标题");
    assert!(
        subtitle.contains("https://www.rust-lang.org/"),
        "{subtitle}"
    );
    assert!(subtitle.contains("目录：书签栏"), "{subtitle}");
    assert_eq!(first.id, "chrome-bookmark:7", "条目标识要稳定且可还原");
    fixture.cleanup();
}

#[test]
fn preview_shows_the_full_url_and_folder() {
    let (host, _chrome, _device, fixture) = linked_host("preview");
    let response = search(&host, "intranet");
    let item = response.selected().expect("应有结果").clone();
    let preview = host.preview(&item.id).expect("书签必须能预览");
    match preview {
        flashcast_core::Preview::Text { body, .. } => {
            assert!(body.contains("token=abc"), "{body}");
            assert!(body.contains("目录：其他书签"), "{body}");
        }
        other => panic!("书签预览应是文本，实际 {other:?}"),
    }
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 变化后刷新
// ---------------------------------------------------------------------------

#[test]
fn index_refreshes_after_the_bookmarks_file_changes() {
    let (host, _chrome, _device, fixture) = linked_host("refresh");

    assert_eq!(search(&host, "新增").items.len(), 0);
    fixture.write_bookmarks(
        "Default",
        r#"{"roots":{"bookmark_bar":{"children":[
             { "id": "30", "name": "新增书签", "type": "url", "url": "https://new.example.com/" }
           ],"id":"1","name":"书签栏","type":"folder"}},"version":1}"#,
    );

    // 只经宿主入口查询：范围搜索按 mtime + size 发现变化并重建索引。
    let response = search(&host, "新增");
    assert!(
        response.plugin_failures.is_empty(),
        "{:?}",
        response.plugin_failures
    );
    assert_eq!(titles(&response.items), vec!["新增书签"]);

    // 显式刷新入口也要看到变化（丢弃指纹后重读）。
    fixture.write_bookmarks("Default", DEFAULT_BOOKMARKS);
    let state = host.refresh_chrome_bookmarks();
    assert!(
        state.bookmarks_label.contains("4 条"),
        "{}",
        state.bookmarks_label
    );
    fixture.cleanup();
}

#[test]
fn association_survives_a_restart() {
    let (host, chrome, device_dir, fixture) = linked_host("restart");
    assert_eq!(
        search(&host, "工作台").items.len(),
        0,
        "默认 profile 里没有它"
    );
    drop(host);

    // 同一个设备目录 + 同一个 Chrome 夹具重建宿主：模拟重启应用。
    let restarted = support::official_host_restarted_with_chrome(
        &device_dir,
        fast_settings(),
        Arc::clone(&chrome),
    );
    let state = restarted.chrome_state();
    assert_eq!(
        state.associated.as_deref(),
        Some("Default"),
        "重启后必须恢复到同一个 profile：{:?}",
        state.error
    );
    assert!(state.available, "重启后仍然要能发现 Chrome");
    let response = search(&restarted, "rust 官网");
    assert_eq!(
        titles(&response.items),
        vec!["Rust 官网"],
        "重启后必须还能检索书签"
    );
    fixture.cleanup();
}

#[test]
fn changing_the_association_switches_the_searched_bookmarks() {
    let (host, _chrome, _device, fixture) = linked_host("switch");
    assert_eq!(search(&host, "rust").items.len(), 3);

    let state = host
        .associate_chrome_profile("Profile 1")
        .expect("改关联必须成功");
    assert_eq!(state.associated.as_deref(), Some("Profile 1"));
    assert_eq!(state.associated_name.as_deref(), Some("工作"));
    assert!(
        state
            .profiles
            .iter()
            .any(|profile| profile.dir == "Profile 1" && profile.associated),
        "状态里要标出当前关联的 profile"
    );
    assert_eq!(titles(&search(&host, "工作台").items), vec!["工作台"]);
    assert!(
        search(&host, "rust").items.is_empty(),
        "换关联后不应再看到旧 profile 的书签"
    );
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 状态与可操作失败
// ---------------------------------------------------------------------------

#[test]
fn missing_bookmarks_file_is_a_normal_empty_state() {
    let (host, _chrome, _device, fixture) = host_with_fixture("missing");
    // 新建一个还没有 Bookmarks 的 profile（只剩 Preferences 目录）。
    write(&fixture.udd.join("Profile 2").join("Preferences"), "{}");
    let state = host
        .associate_chrome_profile("Profile 2")
        .expect("没有 Bookmarks 文件也必须能关联");
    assert!(
        state.bookmarks_label.contains("正常空状态"),
        "{}",
        state.bookmarks_label
    );
    let response = support::plugin_query(&host, "chrome bookmarks");
    assert!(
        response.items.is_empty() && response.plugin_failures.is_empty(),
        "缺失书签文件不是错误：{:?}",
        response.plugin_failures
    );
    fixture.cleanup();
}

#[test]
fn corrupt_json_is_reported_and_keeps_the_previous_index() {
    let (host, _chrome, _device, fixture) = linked_host("corrupt");
    assert_eq!(host.chrome_bookmarks().entries.len(), 4);

    fixture.write_bookmarks("Default", "{ 这不是 JSON");
    let started = Instant::now();
    let state = host.chrome_state();
    let elapsed = started.elapsed();
    assert!(
        matches!(
            state.bookmarks.status,
            flashcast_core::BookmarksStatus::Corrupt { .. }
        ),
        "损坏必须如实报告：{:?}",
        state.bookmarks.status
    );
    assert!(
        elapsed >= Duration::from_millis(450),
        "解析失败必须先重读一次再报错（实际耗时 {elapsed:?}）"
    );
    assert_eq!(
        state.bookmarks.entries.len(),
        4,
        "报告损坏的同时必须保留上一次可用的索引"
    );

    // 范围搜索把原因带回给用户，而不是静默返回空列表。
    let response = search(&host, "rust");
    assert_eq!(
        response.plugin_failures.len(),
        1,
        "{:?}",
        response.plugin_failures
    );
    assert!(
        response.plugin_failures[0].reason.contains("无法解析"),
        "{}",
        response.plugin_failures[0].reason
    );
    fixture.cleanup();
}

#[test]
fn a_mid_write_read_is_retried_and_recovers() {
    let (host, _chrome, _device, fixture) = linked_host("midwrite");

    // 模拟「写入中途读到半个文件」：第一次读是坏的，约 150ms 后文件已经写完。
    fixture.write_bookmarks("Default", "{\"roots\":");
    let path = fixture.bookmarks_path("Default");
    let valid = DEFAULT_BOOKMARKS.to_string();
    let writer = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        std::fs::write(&path, valid).expect("补写完整的书签文件");
    });

    let state = host.chrome_state();
    writer.join().expect("补写线程");
    assert!(
        matches!(
            state.bookmarks.status,
            flashcast_core::BookmarksStatus::Ok { .. }
        ),
        "重读一次之后必须恢复，而不是判定损坏：{:?}",
        state.bookmarks.status
    );
    assert_eq!(state.bookmarks.entries.len(), 4);
    fixture.cleanup();
}

#[test]
fn unreadable_bookmarks_file_is_reported_without_destroying_anything() {
    let (host, _chrome, _device, fixture) = linked_host("unreadable");
    // 用一个同名目录顶掉文件：读取固定返回 EISDIR（与权限无关，测试因此不受 uid 影响）。
    let path = fixture.bookmarks_path("Default");
    std::fs::remove_file(&path).expect("删除夹具书签文件");
    std::fs::create_dir(&path).expect("用目录顶替");

    let state = host.chrome_state();
    assert!(
        matches!(
            state.bookmarks.status,
            flashcast_core::BookmarksStatus::Unreadable { .. }
        ),
        "读不到必须如实报告：{:?}",
        state.bookmarks.status
    );
    // 宿主绝不修改、删除或重建 Chrome 的文件：目录里只有夹具自己放的那一项。
    fixture.assert_untouched("Default", &["Bookmarks"]);
    fixture.cleanup();
}

#[test]
fn chrome_not_installed_is_actionable() {
    let fixture = Fixture::new("not-installed");
    let chrome = Arc::new(FakeChrome::not_installed(
        "已尝试 /usr/bin/google-chrome、/snap/bin/chromium",
    ));
    let (host, _chrome, _device) = official_host_with_chrome(Vec::new(), fast_settings(), chrome);

    let state = host.chrome_state();
    assert!(!state.available);
    let error = state.error.clone().expect("必须说明 Chrome 未安装");
    assert!(error.contains("没有找到 Chrome"), "{error}");
    assert!(
        error.contains("/usr/bin/google-chrome"),
        "要给出可操作的线索：{error}"
    );

    let failure = host
        .associate_chrome_profile("Default")
        .expect_err("未安装时不能关联");
    assert!(failure.to_string().contains("没有找到 Chrome"), "{failure}");
    fixture.cleanup();
}

#[test]
fn associating_an_unknown_profile_directory_is_refused() {
    let (host, _chrome, _device, fixture) = host_with_fixture("unknown-profile");
    let failure = host
        .associate_chrome_profile("Profile 99")
        .expect_err("不存在的 profile 目录必须被拒绝");
    assert!(
        failure.to_string().contains("profile 目录不存在"),
        "{failure}"
    );
    assert!(failure.to_string().contains("Profile 99"), "{failure}");
    // 没有产生关联：状态里仍然是未关联，且索引指向空。
    let state = host.chrome_state();
    assert_eq!(state.associated, None);
    fixture.cleanup();
}

#[test]
fn execute_refuses_when_the_profile_directory_disappeared() {
    let (host, chrome, _device, fixture) = linked_host("profile-gone");
    // 先拿到列表条目（用户是从列表里按回车的），再让 profile 目录消失。
    let item = search(&host, "rust 官网")
        .selected()
        .expect("应有结果")
        .clone();
    fixture.remove_profile("Default");
    let outcome = host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("中文反馈");
    assert!(message.contains("profile 目录不存在"), "{message}");
    assert_eq!(chrome.launch_count(), 0, "校验失败时绝不能启动 Chrome");
    fixture.cleanup();
}

#[test]
fn execute_reports_a_failed_chrome_start() {
    let fixture = Fixture::new("launch-fails");
    let chrome = Arc::new(
        FakeChrome::from_candidates(fixture.candidates().0, fixture.candidates().1)
            .always_fails_launch(flashcast_platform::ChromeError::LaunchFailed(
                "Permission denied (os error 13)".to_string(),
            )),
    );
    let (host, _chrome, _device) = official_host_with_chrome(Vec::new(), fast_settings(), chrome);
    host.associate_chrome_profile("Default").expect("关联成功");

    let item = search(&host, "rust 官网")
        .selected()
        .expect("应有结果")
        .clone();
    let outcome = host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("中文反馈");
    assert!(message.contains("无法启动 Chrome"), "{message}");
    assert!(message.contains("Permission denied"), "{message}");
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 交给 Chrome 的参数向量
// ---------------------------------------------------------------------------

#[test]
fn execute_passes_profile_directory_and_url_as_separate_argv_elements() {
    let (host, chrome, _device, fixture) = linked_host("argv");

    // 「Profile 1」这个目录名带空格；默认 profile 里没有「工作台」，先改关联。
    host.associate_chrome_profile("Profile 1")
        .expect("关联成功");
    let item = search(&host, "工作台")
        .selected()
        .expect("应有结果")
        .clone();
    assert_eq!(item.id, "chrome-bookmark:20");

    let outcome = host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::Done, "{:?}", outcome.message);
    let request = chrome.last_launch().expect("必须有一次启动请求");
    assert_eq!(chrome.launch_count(), 1);
    assert_eq!(
        request.argv(),
        vec![
            fixture.binary.to_string_lossy().into_owned(),
            "--profile-directory=Profile 1".to_string(),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            "https://work.example.com/dashboard".to_string(),
        ],
        "参数必须逐元素断言：profile 目录名带空格也只是同一个 argv 元素"
    );
    // 默认用户数据目录不应传 --user-data-dir（会造成第二个浏览器进程）。
    assert!(
        !request
            .argv()
            .iter()
            .any(|arg| arg.starts_with("--user-data-dir")),
        "{:?}",
        request.argv()
    );
    // 反馈必须说清楚「只是请求了 Chrome」，不能声称页面已打开。
    let message = outcome.message.expect("反馈");
    assert!(message.contains("无法据此确认页面是否已加载"), "{message}");
    fixture.cleanup();
}

#[test]
fn custom_user_data_dir_is_added_before_the_url() {
    let fixture = Fixture::new("argv-custom");
    let chrome = fixture.chrome_custom();
    let (host, chrome, _device) = official_host_with_chrome(Vec::new(), fast_settings(), chrome);
    host.associate_chrome_profile("Default").expect("关联成功");

    let item = search(&host, "intranet")
        .selected()
        .expect("应有结果")
        .clone();
    assert_eq!(
        item.id, "chrome-bookmark:10",
        "带查询串与非 ASCII 的 URL 条目"
    );
    let outcome = host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::Done, "{:?}", outcome.message);

    let request = chrome.last_launch().expect("必须有一次启动请求");
    assert_eq!(
        request.argv(),
        vec![
            fixture.binary.to_string_lossy().into_owned(),
            "--profile-directory=Default".to_string(),
            "--no-first-run".to_string(),
            "--no-default-browser-check".to_string(),
            format!("--user-data-dir={}", fixture.udd.display()),
            "https://intranet.example.com/login?token=abc&next=首页".to_string(),
        ],
        "默认位置之外的目录要显式传 --user-data-dir，URL 带 & 也不会被 shell 拆开"
    );
    fixture.cleanup();
}

#[test]
fn non_http_urls_are_refused_before_launching() {
    let (host, chrome, _device, fixture) = linked_host("bad-url");
    fixture.write_bookmarks(
        "Default",
        r#"{"roots":{"bookmark_bar":{"children":[
             { "id": "40", "name": "脚本书签", "type": "url", "url": "javascript:alert(1)" }
           ],"id":"1","name":"书签栏","type":"folder"}},"version":1}"#,
    );

    let item = search(&host, "脚本书签")
        .selected()
        .expect("列表里应有它")
        .clone();
    let outcome = host.execute(&item);
    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("中文反馈");
    assert!(message.contains("只支持 http / https"), "{message}");
    assert_eq!(chrome.launch_count(), 0, "非法 URL 绝不能交给 Chrome");
    fixture.cleanup();
}

// ---------------------------------------------------------------------------
// 权限、隔离与设备本地化
// ---------------------------------------------------------------------------

#[test]
fn disabled_plugin_neither_searches_nor_opens() {
    let (host, chrome, _device, fixture) = linked_host("disabled");
    host.set_plugin_enabled("chrome-bookmarks", false)
        .expect("停用必须成功");

    let response = support::plugin_query(&host, "chrome 书签");
    assert!(response.scope.is_home(), "停用后不得进入插件范围");
    assert!(
        response
            .items
            .iter()
            .all(|item| item.kind != ItemKind::Bookmark),
        "停用后不得贡献结果"
    );

    // 直接拿停用前的结果执行也要被拒绝。
    let snapshot_item = flashcast_core::SearchItem {
        id: "chrome-bookmark:7".to_string(),
        title: "Rust 官网".to_string(),
        subtitle: None,
        icon: None,
        source: "chrome-bookmarks".to_string(),
        kind: ItemKind::Bookmark,
        default_action: DefaultAction::OpenInChrome,
        preview: flashcast_core::Preview::None,
        score: flashcast_core::Score::unordered(),
    };
    let outcome = host.execute(&snapshot_item);
    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(
        outcome.message.expect("反馈").contains("已停用"),
        "停用必须拒绝执行"
    );
    assert_eq!(chrome.launch_count(), 0);
    fixture.cleanup();
}

#[test]
fn a_plugin_without_the_capability_cannot_open_in_chrome() {
    let fixture = Fixture::new("capability");
    let chrome = fixture.chrome();
    let (host_a, _chrome_a, _device_a) =
        official_host_with_chrome(Vec::new(), fast_settings(), Arc::clone(&chrome));
    host_a
        .associate_chrome_profile("Default")
        .expect("关联成功");

    // 一个没有声明 chrome.open 的插件伪造一条书签结果。
    let forged = flashcast_core::SearchItem {
        id: "chrome-bookmark:7".to_string(),
        title: "Rust 官网".to_string(),
        subtitle: None,
        icon: None,
        source: "no-capability".to_string(),
        kind: ItemKind::Bookmark,
        default_action: DefaultAction::OpenInChrome,
        preview: flashcast_core::Preview::None,
        score: flashcast_core::Score::unordered(),
    };
    let outcome = host_a.execute(&forged);
    assert_eq!(outcome.status, ActionStatus::Failed);
    assert!(
        outcome.message.expect("反馈").contains("不在插件清单里"),
        "伪造来源必须被拒绝"
    );

    let plugins = Arc::new(PluginRegistry::new());
    plugins.register(Arc::new(NoCapabilityPlugin::new()));
    let (host, chrome, _device) = official_host_with_chrome_with_registry(
        Vec::new(),
        fast_settings(),
        fixture.chrome(),
        plugins,
    );
    host.associate_chrome_profile("Default").expect("关联成功");
    let outcome = host.execute(&forged);
    assert_eq!(outcome.status, ActionStatus::Failed);
    let message = outcome.message.expect("反馈");
    assert!(message.contains("没有声明 chrome.open 能力"), "{message}");
    assert_eq!(chrome.launch_count(), 0);
    fixture.cleanup();
}

/// 一个声明了关键词但**没有** chrome.open 能力的插件，用于能力校验的用例。
struct NoCapabilityPlugin {
    manifest: flashcast_core::PluginManifest,
}

impl NoCapabilityPlugin {
    fn new() -> Self {
        Self {
            manifest: flashcast_core::PluginManifest::feature(
                "no-capability",
                "无能力插件",
                "0.1.0",
            )
            .with_keywords(["no capability"]),
        }
    }
}

impl flashcast_core::FeaturePlugin for NoCapabilityPlugin {
    fn manifest(&self) -> flashcast_core::PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        false
    }

    fn search(
        &self,
        _ctx: &flashcast_core::SearchContext,
    ) -> Result<Vec<flashcast_core::SearchItem>, flashcast_core::PluginError> {
        Ok(Vec::new())
    }

    fn take_scope(
        &self,
        _keyword: &flashcast_core::Keyword,
    ) -> Option<Box<dyn flashcast_core::PluginScope>> {
        None
    }
}

/// 使用给定插件注册表构造宿主（能力校验用例用）。
fn official_host_with_chrome_with_registry(
    apps: Vec<flashcast_platform::AppEntry>,
    settings: Settings,
    chrome: Arc<FakeChrome>,
    plugins: Arc<PluginRegistry>,
) -> (Host, Arc<FakeChrome>, PathBuf) {
    use flashcast_core::HostDeps;
    use flashcast_platform::fake::{
        FakeAppCatalog, FakeCapabilityProbe, FakeClipboard, FakeClipboardWatcher, FakeFocusTracker,
        FakeLauncher, FakePaster,
    };

    let device_dir = unique_dir("device");
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(apps)),
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        clipboard: Arc::new(FakeClipboard::new()),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: Arc::clone(&chrome) as Arc<dyn ChromeProvider>,
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
        plugins,
        device_dir: device_dir.clone(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    (host, chrome, device_dir)
}

#[test]
fn association_stays_device_local_and_never_touches_the_workspace() {
    let (host, _chrome, device_dir, fixture) = linked_host("device-local");
    let workspace = unique_dir("chrome-workspace");
    host.select_workspace(&workspace).expect("关联工作区");

    let device_file = device_dir.join("device-local.json");
    let text = std::fs::read_to_string(&device_file).expect("设备本地状态文件");
    assert!(
        text.contains("chrome.association"),
        "关联必须落在设备本地存储：{text}"
    );
    // 本机路径只出现在设备本地目录里，不进入配置工作区。
    for path in files_under(&workspace) {
        let bytes = std::fs::read(&path).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        assert!(
            !text.contains(&fixture.udd.to_string_lossy().into_owned()),
            "工作区文件 {} 不应包含本机 Chrome 路径",
            path.display()
        );
        assert!(
            !text.contains(&fixture.binary.to_string_lossy().into_owned()),
            "工作区文件 {} 不应包含本机 Chrome 可执行文件路径",
            path.display()
        );
    }
    fixture.cleanup();
    let _ = std::fs::remove_dir_all(&workspace);
}

#[test]
fn explicit_page_lists_more_than_fifty_bookmarks_and_blank_query_keeps_the_page() {
    let (host, _chrome, _device, fixture) = linked_host("all-bookmarks");
    let children: Vec<serde_json::Value> = (0..75).map(|id| serde_json::json!({"id":id.to_string(),"name":format!("书签 {id:02}"),"type":"url","url":format!("https://example.com/{id}")})).collect();
    fixture.write_bookmarks("Default",&serde_json::json!({"roots":{"bookmark_bar":{"type":"folder","name":"书签栏","children":children}}}).to_string());
    host.refresh_chrome_bookmarks();
    assert!(host.query("bookmark").scope.is_home());
    assert_eq!(
        host.execute_plugin_command("flashcast.plugin.chrome-bookmarks")
            .status,
        ActionStatus::Done
    );
    assert_eq!(host.snapshot().items.len(), 75);
    assert_eq!(host.query("74").items.len(), 1);
    let all = host.query("");
    assert!(!all.scope.is_home());
    assert_eq!(all.items.len(), 75);
    fixture.cleanup();
}

#[test]
fn account_bookmarks_are_discovered_searched_and_opened_without_local_bookmarks() {
    let (host, chrome, _device, fixture) = host_with_fixture("account-only");
    std::fs::remove_file(fixture.bookmarks_path("Default")).unwrap();
    write(
        &fixture.udd.join("Default/AccountBookmarks"),
        DEFAULT_BOOKMARKS,
    );
    let state = host.associate_chrome_profile("Default").unwrap();
    assert!(
        state
            .profiles
            .iter()
            .find(|p| p.dir == "Default")
            .unwrap()
            .has_bookmarks
    );
    let response = search(&host, "Rust 官网");
    assert_eq!(response.items.len(), 1, "账号书签必须进入插件页面");
    assert_eq!(response.items[0].id, "chrome-bookmark:account:7");
    assert_eq!(host.execute(&response.items[0]).status, ActionStatus::Done);
    assert_eq!(
        chrome.last_launch().unwrap().args.last().unwrap(),
        "https://www.rust-lang.org/"
    );
    fixture.assert_untouched("Default", &["AccountBookmarks"]);
    fixture.cleanup();
}

#[test]
fn local_and_account_bookmarks_keep_distinct_ids_and_refresh_both_sources() {
    let (host, chrome, _device, fixture) = linked_host("account-merge");
    let account_path = fixture.udd.join("Default/AccountBookmarks");
    let account = r#"{"roots":{"bookmark_bar":{"children":[{"id":"7","name":"账号书签","type":"url","url":"https://account.example.com/"}]}}}"#;
    write(&account_path, account);
    let all = search(&host, "");
    assert_eq!(all.items.len(), 5);
    let item = all
        .items
        .iter()
        .find(|i| i.id == "chrome-bookmark:account:7")
        .unwrap();
    assert_eq!(host.execute(item).status, ActionStatus::Done);
    assert_eq!(
        chrome.last_launch().unwrap().args.last().unwrap(),
        "https://account.example.com/"
    );
    let local = all
        .items
        .iter()
        .find(|i| i.id == "chrome-bookmark:7")
        .unwrap();
    assert_eq!(host.execute(local).status, ActionStatus::Done);
    assert_eq!(
        chrome.last_launch().unwrap().args.last().unwrap(),
        "https://www.rust-lang.org/"
    );
    write(&account_path, &account.replace("账号书签", "账号新增书签"));
    assert_eq!(search(&host, "账号新增").items.len(), 1);
    std::fs::remove_file(&account_path).unwrap();
    assert_eq!(search(&host, "").items.len(), 4);
    write(&account_path, account);
    std::fs::remove_file(fixture.bookmarks_path("Default")).unwrap();
    assert_eq!(search(&host, "").items.len(), 1);
    fixture.cleanup();
}

#[test]
fn corrupt_account_bookmarks_report_the_source_and_keep_the_previous_merged_index() {
    let (host, _chrome, _device, fixture) = linked_host("account-corrupt");
    let account_path = fixture.udd.join("Default/AccountBookmarks");
    write(&account_path, WORK_BOOKMARKS);
    assert_eq!(search(&host, "").items.len(), 5);
    write(&account_path, "{partial write");
    let state = host.refresh_chrome_bookmarks();
    assert_eq!(state.bookmarks.entries.len(), 5);
    match state.bookmarks.status {
        flashcast_core::BookmarksStatus::Corrupt { reason } => {
            assert!(reason.contains("AccountBookmarks"))
        }
        other => panic!("应报告账号文件损坏，实际为 {other:?}"),
    }
    write(&account_path, WORK_BOOKMARKS);
    assert_eq!(search(&host, "").items.len(), 5);
    fixture.cleanup();
}
