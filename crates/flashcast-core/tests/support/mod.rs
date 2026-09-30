//! 集成测试共用的构造与插件替身。
//!
//! 插件替身放在这里而不是 `flashcast-platform::fake`，原因是插件契约
//! （`FeaturePlugin`）定义在 `flashcast-core`，而 `flashcast-platform` 是
//! `flashcast-core` 的依赖：把插件替身放进平台层会形成循环依赖。
//! 平台适配层的替身仍然只在 `flashcast-platform::fake`（ADR §5）。

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use flashcast_core::{
    DefaultAction, FeaturePlugin, Host, HostDeps, ItemKind, Keyword, PluginError, PluginManifest,
    PluginRegistry, PluginScope, Preview, Score, SearchContext, SearchItem, Settings,
};
use flashcast_platform::catalog::{AppEntry, AppSource, IconRef};
use flashcast_platform::fake::{FakeAppCatalog, FakeCapabilityProbe, FakeLauncher};

/// 构造一个软件条目。
pub fn app(id: &str, name: &str) -> AppEntry {
    AppEntry {
        id: format!("{id}.desktop"),
        name: name.to_string(),
        comment: None,
        icon: None,
        exec: vec![format!("/usr/bin/{id}")],
        desktop_file: None,
        working_dir: None,
        wm_class: None,
        terminal: false,
        keywords: Vec::new(),
        source: AppSource::Desktop,
    }
}

/// 构造一个带说明、关键词与图标的软件条目。
pub fn app_rich(id: &str, name: &str, comment: &str, keywords: &[&str]) -> AppEntry {
    AppEntry {
        comment: Some(comment.to_string()),
        icon: Some(IconRef {
            name: id.to_string(),
            path: None,
        }),
        keywords: keywords.iter().map(|k| k.to_string()).collect(),
        ..app(id, name)
    }
}

/// 构造一条插件结果。
pub fn item(id: &str, title: &str, source: &str, relevance: u8) -> SearchItem {
    SearchItem {
        id: id.to_string(),
        title: title.to_string(),
        subtitle: None,
        icon: None,
        source: source.to_string(),
        kind: ItemKind::Memo,
        default_action: DefaultAction::Paste,
        preview: Preview::None,
        score: Score::new(flashcast_core::MatchTier::KeywordOrTagExact, relevance),
    }
}

/// 构造宿主与替身启动器。
pub fn host_with(apps: Vec<AppEntry>, settings: Settings) -> (Host, Arc<FakeLauncher>) {
    let (host, launcher, _device) = host_with_device(apps, settings);
    (host, launcher)
}

/// 构造宿主，并返回它的**设备本地**数据目录（应用数据目录）。
/// 配置工作区之外的本机数据都放在这里；测试用它断言工作区里没有本机数据。
pub fn host_with_device(
    apps: Vec<AppEntry>,
    settings: Settings,
) -> (Host, Arc<FakeLauncher>, PathBuf) {
    let device_dir = unique_dir("device");
    let (host, launcher) = build_host(apps, settings, Arc::new(PluginRegistry::new()), device_dir.clone());
    (host, launcher, device_dir)
}

/// 使用给定插件注册表构造宿主。
pub fn host_with_plugins(
    apps: Vec<AppEntry>,
    settings: Settings,
    plugins: Arc<PluginRegistry>,
) -> (Host, Arc<FakeLauncher>) {
    let (host, launcher, _device) = host_with_plugins_device(apps, settings, plugins);
    (host, launcher)
}

/// 使用给定插件注册表与设备目录构造宿主。
pub fn host_with_plugins_device(
    apps: Vec<AppEntry>,
    settings: Settings,
    plugins: Arc<PluginRegistry>,
) -> (Host, Arc<FakeLauncher>, PathBuf) {
    let device_dir = unique_dir("device");
    let (host, launcher) = build_host(apps, settings, plugins, device_dir.clone());
    (host, launcher, device_dir)
}

/// 用同一个设备目录重建宿主：模拟「重启应用」。
pub fn host_restarted(device_dir: &Path, settings: Settings) -> Host {
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(Vec::new())),
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.to_path_buf(),
    };
    Host::new(deps, settings)
}

fn build_host(
    apps: Vec<AppEntry>,
    settings: Settings,
    plugins: Arc<PluginRegistry>,
    device_dir: PathBuf,
) -> (Host, Arc<FakeLauncher>) {
    let launcher = Arc::new(FakeLauncher::always_succeeds());
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(apps)),
        launcher: launcher.clone(),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        plugins,
        device_dir,
    };
    (Host::new(deps, settings), launcher)
}

/// 每次调用返回一个新的临时目录（进程内唯一，测试结束时由 `cleanup` 删除）。
pub fn unique_dir(prefix: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let index = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "flashcast-test-{prefix}-{}-{index}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("无法创建测试临时目录");
    dir
}

/// 删除测试创建的临时目录。
pub fn cleanup(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

/// 递归收集目录下的所有文件（含隐藏文件与 `.git`）。
pub fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// 用真实 `git2` 初始化一个临时 Git 仓库，返回其根目录。
pub fn real_git_repo(prefix: &str) -> PathBuf {
    let dir = unique_dir(prefix);
    git2::Repository::init(&dir).expect("无法初始化临时 Git 仓库");
    dir
}

/// 在临时目录里构造一个**裸仓库**作为远端。`files` 是「相对路径 → 内容」，
/// 直接写 tree/commit（默认分支 `main`），不经过 push，因此不依赖网络或传输实现。
pub fn bare_remote<A: AsRef<str>, B: AsRef<str>>(prefix: &str, files: &[(A, B)]) -> PathBuf {
    let dir = unique_dir(prefix);
    let repo = git2::Repository::init_bare(&dir).expect("无法初始化裸仓库");
    let files: Vec<(String, String)> = files
        .iter()
        .map(|(path, text)| (path.as_ref().to_string(), text.as_ref().to_string()))
        .collect();
    let tree_oid = write_tree_at(&repo, &files);
    let tree = repo.find_tree(tree_oid).expect("无法读取 tree");
    let signature = git2::Signature::now("Flashcast 测试", "test@localhost").expect("无法构造签名");
    repo.commit(
        Some("refs/heads/main"),
        &signature,
        &signature,
        "初始配置",
        &tree,
        &[],
    )
    .expect("无法提交初始配置");
    repo.set_head("refs/heads/main").expect("无法设置 HEAD");
    dir
}

/// 在裸仓库（无工作区）里递归写入 tree，返回根 tree 的 oid。
fn write_tree_at(repo: &git2::Repository, files: &[(String, String)]) -> git2::Oid {
    use std::collections::BTreeMap;
    let mut builder = repo.treebuilder(None).expect("无法创建 tree builder");
    let mut subdirs: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (path, content) in files {
        match path.split_once('/') {
            Some((head, rest)) => subdirs
                .entry(head.to_string())
                .or_default()
                .push((rest.to_string(), content.clone())),
            None => {
                let blob = repo.blob(content.as_bytes()).expect("无法写入 blob");
                builder
                    .insert(path, blob, 0o100_644)
                    .expect("无法插入 blob");
            }
        }
    }
    for (name, children) in subdirs {
        let oid = write_tree_at(repo, &children);
        builder.insert(name, oid, 0o040_000).expect("无法插入子树");
    }
    builder.write().expect("无法写入 tree")
}

/// 裸仓库的克隆 URL（本地路径形式，不经过网络）。
pub fn remote_url(remote: &Path) -> String {
    remote.to_string_lossy().into_owned()
}

/// 一份可被克隆的配置工作区内容。
pub fn workspace_files(hotkey: &str) -> Vec<(&'static str, String)> {
    vec![
        (
            "settings.toml",
            format!(
                "hotkey = \"{hotkey}\"\nlaunchAtStartup = false\nquickAccessLimit = 6\npluginTimeoutMs = 400\ndisabledPlugins = []\n"
            ),
        ),
        ("manifest.json", "{\"plugins\": []}\n".to_string()),
        ("theme.json", "{\"theme\": \"dark\"}\n".to_string()),
        ("memos/hello.md", "# 你好\n\n来自远端的备忘录。\n".to_string()),
    ]
}

/// 默认设置，插件超时缩短到 60ms 以便快速验证超时隔离。
pub fn fast_settings() -> Settings {
    Settings {
        plugin_timeout_ms: 60,
        ..Settings::default()
    }
}

// ---------------------------------------------------------------------------
// 插件替身
// ---------------------------------------------------------------------------

/// 正常返回固定结果的插件。
pub struct StaticPlugin {
    pub manifest: PluginManifest,
    pub home: bool,
    pub items: Vec<SearchItem>,
    pub searches: AtomicUsize,
}

impl StaticPlugin {
    pub fn new(id: &str, items: Vec<SearchItem>) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0"),
            home: true,
            items,
            searches: AtomicUsize::new(0),
        }
    }

    pub fn search_count(&self) -> usize {
        self.searches.load(Ordering::SeqCst)
    }
}

impl FeaturePlugin for StaticPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        self.home
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        self.searches.fetch_add(1, Ordering::SeqCst);
        Ok(self.items.clone())
    }

    fn take_scope(&self, _keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        None
    }
}

/// 搜索必定失败的插件。
pub struct FailingPlugin {
    pub manifest: PluginManifest,
}

impl FailingPlugin {
    pub fn new(id: &str) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0"),
        }
    }
}

impl FeaturePlugin for FailingPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        true
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Err(PluginError::failed("插件内部错误"))
    }

    fn take_scope(&self, _keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        None
    }
}

/// 搜索会阻塞很久的插件，用于验证超时隔离。
pub struct SlowPlugin {
    pub manifest: PluginManifest,
    pub delay: Duration,
}

impl SlowPlugin {
    pub fn new(id: &str, delay: Duration) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0"),
            delay,
        }
    }
}

impl FeaturePlugin for SlowPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        true
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        std::thread::sleep(self.delay);
        Ok(vec![item("slow:1", "慢插件结果", "slow", 50)])
    }

    fn take_scope(&self, _keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        None
    }
}

/// 搜索时 panic 的插件。
pub struct PanicPlugin {
    pub manifest: PluginManifest,
}

impl PanicPlugin {
    pub fn new(id: &str) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0"),
        }
    }
}

impl FeaturePlugin for PanicPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        true
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        panic!("插件故意 panic");
    }

    fn take_scope(&self, _keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        None
    }
}

/// 带关键词入口的插件，用于验证进入范围与 `back()`。
pub struct KeywordPlugin {
    pub manifest: PluginManifest,
    pub scope_items: Vec<SearchItem>,
    pub keyword_seen: Mutex<Vec<String>>,
}

impl KeywordPlugin {
    pub fn new(id: &str, keyword: &str, scope_items: Vec<SearchItem>) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0").with_keywords([keyword]),
            scope_items,
            keyword_seen: Mutex::new(Vec::new()),
        }
    }
}

struct KeywordScope {
    plugin_id: String,
    keyword: String,
    items: Vec<SearchItem>,
}

impl PluginScope for KeywordScope {
    fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    fn keyword(&self) -> &str {
        &self.keyword
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Ok(self.items.clone())
    }
}

impl FeaturePlugin for KeywordPlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        // 只在关键词完整匹配时进入范围，不参与首屏搜索。
        false
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Ok(Vec::new())
    }

    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        self.keyword_seen
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(keyword.as_str().to_string());
        Some(Box::new(KeywordScope {
            plugin_id: self.manifest.id.clone(),
            keyword: keyword.as_str().to_string(),
            items: self.scope_items.clone(),
        }))
    }
}
