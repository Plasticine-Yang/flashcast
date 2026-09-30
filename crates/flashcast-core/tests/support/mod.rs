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

/// 默认设置，插件超时缩短到 60ms 以便快速验证超时隔离。
pub fn fast_settings() -> Settings {
    Settings {
        plugin_timeout_ms: 60,
        ..Settings::default()
    }
}

// ---------------------------------------------------------------------------
// 真实 Git 仓库夹具（ticket 15）
// ---------------------------------------------------------------------------

/// 测试用的固定提交身份。写进**仓库本地**配置，因此不依赖运行环境的 git 配置。
pub const TEST_AUTHOR_NAME: &str = "Flashcast 测试";
pub const TEST_AUTHOR_EMAIL: &str = "flashcast-test@example.invalid";

/// 在临时目录上初始化仓库、写入给定文件并提交一次，返回仓库根目录。
///
/// 提交身份写在仓库本地配置里，测试因此不受全局 `~/.gitconfig` 影响。
pub fn git_repo_with_commit(prefix: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = real_git_repo(prefix);
    set_identity(&dir, TEST_AUTHOR_NAME, TEST_AUTHOR_EMAIL);
    for (rel, content) in files {
        git_write(&dir, rel, content);
    }
    git_commit_all(&dir, "初始提交");
    dir
}

/// 设置仓库本地的提交身份。
pub fn set_identity(repo: &Path, name: &str, email: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut config = repository.config().expect("读取测试仓库配置");
    config.set_str("user.name", name).expect("写入 user.name");
    config
        .set_str("user.email", email)
        .expect("写入 user.email");
}

/// 清空仓库本地的提交身份（写入空值，覆盖全局配置）。
pub fn clear_identity(repo: &Path) {
    set_identity(repo, "", "");
}

/// 写文件（自动创建父目录）。
pub fn git_write(repo: &Path, rel: &str, content: &str) {
    let path = repo.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("创建测试文件父目录");
    }
    std::fs::write(&path, content).expect("写入测试文件");
}

/// 删除文件。
pub fn git_remove(repo: &Path, rel: &str) {
    std::fs::remove_file(repo.join(rel)).expect("删除测试文件");
}

/// 把给定路径加入索引（`git add`）。
pub fn git_stage(repo: &Path, rel: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut index = repository.index().expect("读取索引");
    index
        .add_path(Path::new(rel))
        .unwrap_or_else(|error| panic!("暂存 {rel} 失败：{error}"));
    index.write().expect("写入索引");
}

/// 把所有工作区改动提交一次，返回提交 oid。
pub fn git_commit_all(repo: &Path, message: &str) -> git2::Oid {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut index = repository.index().expect("读取索引");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("暂存全部文件");
    index.write().expect("写入索引");
    let tree_oid = index.write_tree().expect("写出树");
    let tree = repository.find_tree(tree_oid).expect("读取树");
    let signature = repository
        .signature()
        .expect("测试仓库必须配置了提交身份");
    let parents = match repository.head() {
        Ok(head) => vec![head.peel_to_commit().expect("读取 HEAD 提交")],
        Err(_) => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repository
        .commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
        .expect("创建测试提交")
}

/// 当前 HEAD 提交 oid。
pub fn git_head_oid(repo: &Path) -> git2::Oid {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let head = repository.head().expect("读取 HEAD");
    let commit = head.peel_to_commit().expect("HEAD 必须指向提交");
    commit.id()
}

/// 某个提交的树里所有文件路径（仓库相对，`/` 分隔，已排序）。
pub fn git_tree_paths(repo: &Path, commit: git2::Oid) -> Vec<String> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let commit = repository.find_commit(commit).expect("读取提交");
    let tree = commit.tree().expect("读取提交树");
    let mut found = Vec::new();
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if entry.kind() == Some(git2::ObjectType::Blob) {
            found.push(format!("{dir}{}", entry.name().unwrap_or_default()));
        }
        git2::TreeWalkResult::Ok
    })
    .expect("遍历提交树");
    found.sort();
    found
}

/// 读取提交里某个文件的内容；不存在返回 `None`。
pub fn git_show(repo: &Path, commit: git2::Oid, rel: &str) -> Option<String> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let commit = repository.find_commit(commit).expect("读取提交");
    let tree = commit.tree().expect("读取提交树");
    let entry = tree.get_path(Path::new(rel)).ok()?;
    let blob = repository.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// 索引文件的原始字节。用于证明提交过程没有破坏用户已有的暂存状态。
pub fn git_index_bytes(repo: &Path) -> Vec<u8> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let path = repository.path().join("index");
    std::fs::read(&path).expect("索引文件必须存在")
}

/// 提交的作者/提交者身份。
pub fn git_commit_signature(repo: &Path, commit: git2::Oid) -> (String, String, usize) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let commit = repository.find_commit(commit).expect("读取提交");
    let author = commit.author();
    (
        author.name().unwrap_or_default().to_string(),
        author.email().unwrap_or_default().to_string(),
        commit.parent_count(),
    )
}

/// 在 gitdir 下放一个标记文件或锁文件（例如 `index.lock`、`MERGE_HEAD`）。
pub fn git_put_marker(repo: &Path, rel: &str, content: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let path = repository.path().join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("创建 gitdir 子目录");
    }
    std::fs::write(&path, content).expect("写入 gitdir 标记文件");
}

/// 删除 gitdir 下的标记文件。
pub fn git_remove_marker(repo: &Path, rel: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let _ = std::fs::remove_file(repository.path().join(rel));
}

/// 删除 gitdir 下的目录（例如 `rebase-merge`）。
pub fn git_remove_dir(repo: &Path, rel: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let _ = std::fs::remove_dir_all(repository.path().join(rel));
}

/// 当前索引里某个路径的 stage 0 条目 oid。
pub fn git_index_oid(repo: &Path, rel: &str) -> Option<git2::Oid> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let index = repository.index().expect("读取索引");
    index.get_path(Path::new(rel), 0).map(|entry| entry.id)
}

/// 索引里某个路径 stage 0 条目的**内容**；不在索引里返回 `None`。
pub fn git_index_blob(repo: &Path, rel: &str) -> Option<String> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let index = repository.index().expect("读取索引");
    let entry = index.get_path(Path::new(rel), 0)?;
    let blob = repository.find_blob(entry.id).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// 当前 HEAD 指向的引用名（例如 `refs/heads/main`）。
pub fn git_head_ref(repo: &Path) -> String {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let reference = repository.find_reference("HEAD").expect("读取 HEAD 引用");
    reference
        .symbolic_target()
        .ok()
        .flatten()
        .unwrap_or_default()
        .to_string()
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
