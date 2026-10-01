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
    ClipboardCaptureOutcome, ClipboardEvent, DefaultAction, FeaturePlugin, Host, HostDeps,
    ItemKind, Keyword, PluginError, PluginManifest, PluginRegistry, PluginScope, Preview, Score,
    SearchContext, SearchItem, Settings, CLIPBOARD_PLUGIN_ID,
};
use flashcast_platform::catalog::{AppEntry, AppSource, IconRef};
use flashcast_platform::chrome::ChromeProvider;
use flashcast_platform::fake::{
    FakeAppCatalog, FakeCapabilityProbe, FakeChrome, FakeClipboard, FakeClipboardWatcher,
    FakeFocusTracker, FakeLauncher, FakePaster,
};

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

/// 默认的 Chrome 替身：测试环境里「没有安装 Chrome」。需要真实发现的用例
/// 用 [`FakeChrome::from_candidates`] 指向自己写的临时夹具。
pub fn no_chrome() -> Arc<FakeChrome> {
    Arc::new(FakeChrome::not_installed("测试环境未配置 Chrome"))
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
    let (host, launcher) = build_host(
        apps,
        settings,
        Arc::new(PluginRegistry::new()),
        device_dir.clone(),
    );
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
    host_restarted_with(
        device_dir,
        settings,
        no_chrome(),
        Arc::new(PluginRegistry::new()),
    )
}

/// 用同一个设备目录重建宿主，并注入给定的 Chrome 替身与插件注册表。
pub fn host_restarted_with(
    device_dir: &Path,
    settings: Settings,
    chrome: Arc<FakeChrome>,
    plugins: Arc<PluginRegistry>,
) -> Host {
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(Vec::new())),
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        clipboard: Arc::new(FakeClipboard::new()),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome,
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
        plugins,
        device_dir: device_dir.to_path_buf(),
    };
    Host::new(deps, settings)
}

/// 用同一个设备目录重建宿主，并安装随应用提供的官方功能插件：模拟「重启应用」。
pub fn official_host_restarted(device_dir: &Path, settings: Settings) -> Host {
    let host = host_restarted(device_dir, settings);
    host.install_official_plugins();
    host
}

/// 同 [`official_host_restarted`]，但注入给定的 Chrome 替身：重启后重新关联的用例用它。
pub fn official_host_restarted_with_chrome(
    device_dir: &Path,
    settings: Settings,
    chrome: Arc<FakeChrome>,
) -> Host {
    let host = host_restarted_with(
        device_dir,
        settings,
        chrome,
        Arc::new(PluginRegistry::new()),
    );
    host.install_official_plugins();
    host
}

/// 构造带官方功能插件（备忘录）的宿主与剪贴板替身。
pub fn official_host(
    apps: Vec<AppEntry>,
    settings: Settings,
) -> (Host, Arc<FakeLauncher>, Arc<FakeClipboard>, PathBuf) {
    official_host_with_device(apps, settings)
}

/// 同 [`official_host`]，并返回设备本地数据目录。
pub fn official_host_with_device(
    apps: Vec<AppEntry>,
    settings: Settings,
) -> (Host, Arc<FakeLauncher>, Arc<FakeClipboard>, PathBuf) {
    let device_dir = unique_dir("device");
    let launcher = Arc::new(FakeLauncher::always_succeeds());
    let clipboard = Arc::new(FakeClipboard::new());
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(apps)),
        launcher: launcher.clone(),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        clipboard: clipboard.clone(),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: no_chrome(),
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.clone(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    (host, launcher, clipboard, device_dir)
}

/// 构造带官方功能插件与**真实发现**的 Chrome 替身的宿主。
///
/// 返回的 `Arc<FakeChrome>` 记录交给 Chrome 的 argv，供测试精确断言参数向量。
pub fn official_host_with_chrome(
    apps: Vec<AppEntry>,
    settings: Settings,
    chrome: Arc<FakeChrome>,
) -> (Host, Arc<FakeChrome>, PathBuf) {
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
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.clone(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    (host, chrome, device_dir)
}

/// 自动粘贴流程的测试载体：宿主 + 可观察的剪贴板 / 焦点 / 粘贴替身。
///
/// 焦点与粘贴替身必须由测试自己持有才能断言（「恢复了几次」「注入了几次」
/// 「注入时剪贴板里是什么」），因此这里用一个显式的结构而不是元组。
pub struct PasteHarness {
    pub host: Host,
    pub clipboard: Arc<FakeClipboard>,
    pub focus: Arc<FakeFocusTracker>,
    pub paster: Arc<FakePaster>,
    pub device_dir: PathBuf,
}

impl PasteHarness {
    /// 唤起：外壳在显示窗口之前捕获前台应用，并把它交给宿主。
    pub fn summon(&self, app: flashcast_platform::FocusedApp) {
        self.focus.set_active(Some(app.clone()));
        self.host.set_paste_target(Some(app));
    }

    /// 唤起但拿不到前台应用（Wayland 等）：宿主必须降级为手动粘贴。
    pub fn summon_without_target(&self) {
        self.focus.set_active(None);
        self.host.set_paste_target(None);
    }
}

/// 构造带官方功能插件的宿主，并暴露剪贴板、焦点与粘贴替身（ticket 08）。
pub fn official_host_with_paste(
    apps: Vec<AppEntry>,
    settings: Settings,
    capabilities: Arc<dyn flashcast_platform::CapabilityProbe>,
    focus: Arc<FakeFocusTracker>,
    paster: Arc<FakePaster>,
    clipboard: Arc<FakeClipboard>,
) -> PasteHarness {
    let device_dir = unique_dir("device");
    // 宿主按能力注入：这里显式转成 trait 对象，替身仍由测试持有以便断言。
    let focus_dep: Arc<dyn flashcast_platform::FocusTracker> = focus.clone();
    let paster_dep: Arc<dyn flashcast_platform::Paster> = paster.clone();
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(apps)),
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities,
        clipboard: clipboard.clone(),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: no_chrome(),
        focus: focus_dep,
        paster: paster_dep,
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.clone(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    PasteHarness {
        host,
        clipboard,
        focus,
        paster,
        device_dir,
    }
}

/// 同 [`official_host_with_device`]，但使用调用方提供的插件注册表与能力探测。
pub fn official_host_with_plugins(
    apps: Vec<AppEntry>,
    settings: Settings,
    plugins: Arc<PluginRegistry>,
    capabilities: Arc<dyn flashcast_platform::CapabilityProbe>,
    clipboard: Arc<FakeClipboard>,
) -> (Host, Arc<FakeLauncher>, Arc<FakeClipboard>, PathBuf) {
    let device_dir = unique_dir("device");
    let launcher = Arc::new(FakeLauncher::always_succeeds());
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(apps)),
        launcher: launcher.clone(),
        capabilities,
        clipboard: clipboard.clone(),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: no_chrome(),
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
        plugins,
        device_dir: device_dir.clone(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    (host, launcher, clipboard, device_dir)
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
        clipboard: Arc::new(FakeClipboard::new()),
        clipboard_watcher: Arc::new(FakeClipboardWatcher::new()),
        chrome: no_chrome(),
        focus: Arc::new(FakeFocusTracker::default()),
        paster: Arc::new(FakePaster::new()),
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
///
/// 初始分支被固定为 `main`：`Repository::init` 会遵循宿主机的 `init.defaultBranch`，
/// 开发机通常配成 `main`、CI runner 上则是 `master`，断言不能依赖宿主的 Git 配置。
/// 仓库刚建好、还没有提交，所以直接写 HEAD 符号引用（对「尚未诞生的分支」也有效）。
pub fn real_git_repo(prefix: &str) -> PathBuf {
    let dir = unique_dir(prefix);
    let repo = git2::Repository::init(&dir).expect("无法初始化临时 Git 仓库");
    repo.reference_symbolic("HEAD", "refs/heads/main", true, "固定初始分支")
        .expect("无法固定初始分支");
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

// ---------------------------------------------------------------------------
// 远端夹具（ticket 16：推送 / 快进拉取 / 分叉）
// ---------------------------------------------------------------------------

/// 在裸远端上追加一次提交（不经过任何工作区），用于制造「远端有新提交」的场景。
///
/// `changes` 是「仓库相对路径 → 内容」，已存在的路径会被替换为新内容。
/// 默认分支固定为 `main`，与 [`bare_remote`] 一致。
pub fn bare_remote_commit(remote: &Path, changes: &[(&str, &str)], message: &str) -> git2::Oid {
    let repo = git2::Repository::open_bare(remote).expect("打开裸远端");
    let parent = repo
        .find_reference("refs/heads/main")
        .ok()
        .and_then(|reference| reference.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let base = parent.as_ref().and_then(|commit| commit.tree().ok());
    let files: Vec<(String, String)> = changes
        .iter()
        .map(|(path, content)| (path.to_string(), content.to_string()))
        .collect();
    let tree_oid = write_tree_over(&repo, base.as_ref(), &files);
    let tree = repo.find_tree(tree_oid).expect("读取远端 tree");
    let signature = git2::Signature::now(TEST_AUTHOR_NAME, TEST_AUTHOR_EMAIL).expect("构造签名");
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(
        Some("refs/heads/main"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .expect("远端提交失败")
}

/// 在已有 tree（可能为空）之上替换 / 新增若干路径，返回新的根 tree oid。
fn write_tree_over(
    repo: &git2::Repository,
    base: Option<&git2::Tree>,
    files: &[(String, String)],
) -> git2::Oid {
    use std::collections::BTreeMap;
    let mut builder = repo
        .treebuilder(base)
        .expect("无法基于已有 tree 创建 builder");
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
        let existing = base
            .and_then(|tree| tree.get_name(&name))
            .filter(|entry| entry.kind() == Some(git2::ObjectType::Tree))
            .and_then(|entry| repo.find_tree(entry.id()).ok());
        let oid = write_tree_over(repo, existing.as_ref(), &children);
        builder.insert(name, oid, 0o040_000).expect("无法插入子树");
    }
    builder.write().expect("无法写入 tree")
}

/// 读取裸远端某个分支的 oid。
pub fn bare_remote_oid(remote: &Path, branch: &str) -> Option<git2::Oid> {
    let repo = git2::Repository::open_bare(remote).expect("打开裸远端");
    repo.find_reference(&format!("refs/heads/{branch}"))
        .ok()
        .and_then(|reference| reference.target())
}

/// 读取裸远端某个分支下某个文件的内容。
pub fn bare_remote_file(remote: &Path, branch: &str, rel: &str) -> Option<String> {
    let repo = git2::Repository::open_bare(remote).expect("打开裸远端");
    let commit = repo
        .find_reference(&format!("refs/heads/{branch}"))
        .ok()
        .and_then(|reference| reference.target())
        .and_then(|oid| repo.find_commit(oid).ok())?;
    let tree = commit.tree().ok()?;
    let entry = tree.get_path(Path::new(rel)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    Some(String::from_utf8_lossy(blob.content()).into_owned())
}

/// 给仓库增加一个远端。
pub fn git_remote_add(repo: &Path, name: &str, url: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    repository
        .remote(name, url)
        .unwrap_or_else(|error| panic!("添加远端 {name} 失败：{error}"));
}

/// 读取仓库的某个远端地址。
pub fn git_remote_url(repo: &Path, name: &str) -> Option<String> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let remote = repository.find_remote(name).ok()?;
    remote.url().ok().map(str::to_string)
}

/// 改写仓库的某个远端地址（用于构造鉴权 / 离线失败）。
pub fn git_set_remote_url(repo: &Path, name: &str, url: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    repository
        .remote_set_url(name, url)
        .unwrap_or_else(|error| panic!("改写远端地址失败：{error}"));
}

/// 设置分支的上游关系（`branch.<名>.remote` + `branch.<名>.merge`）。
pub fn git_set_upstream(repo: &Path, branch: &str, remote: &str, remote_branch: &str) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut config = repository.config().expect("读取仓库配置");
    config
        .set_str(&format!("branch.{branch}.remote"), remote)
        .expect("写入 branch.remote");
    config
        .set_str(
            &format!("branch.{branch}.merge"),
            &format!("refs/heads/{remote_branch}"),
        )
        .expect("写入 branch.merge");
}

/// 读取某个配置键。
pub fn git_config_get(repo: &Path, key: &str) -> Option<String> {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    repository.config().ok()?.get_string(key).ok()
}

/// 制造一次真实的合并冲突：把 `other` 合并进 HEAD，冲突留在索引里。
///
/// 调用后仓库处于 `MERGE_HEAD` 状态（`Repository::state()` 为 `Merge`）。
/// 需要「索引里有冲突但没有进行中标记」这种异常组合时，再调用
/// [`git_remove_marker`] 删掉 `MERGE_HEAD`。
pub fn git_merge_other(repo: &Path, other: git2::Oid) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let annotated = repository
        .find_annotated_commit(other)
        .expect("构造 annotated commit");
    repository
        .merge(&[&annotated], None, None)
        .expect("合并应产生冲突而不是失败");
}

/// 取消（删除）一次工作区改动，用于验证「外部处理后可恢复」。
pub fn git_checkout_all(repo: &Path) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut options = git2::build::CheckoutBuilder::new();
    options.force();
    repository
        .checkout_head(Some(&mut options))
        .expect("恢复工作区失败");
}

/// 在外部解决全部冲突：把索引重置回 HEAD 的树，再强制检出 HEAD。
///
/// 用于验证「用户在应用外部处理冲突后，重新检测即可恢复同步」。
pub fn git_resolve_all_conflicts(repo: &Path) {
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let head_tree = repository
        .head()
        .expect("读取 HEAD")
        .peel_to_tree()
        .expect("HEAD 树");
    {
        let mut index = repository.index().expect("读取索引");
        index.read_tree(&head_tree).expect("重置索引到 HEAD");
        index.write().expect("写入索引");
    }
    let mut options = git2::build::CheckoutBuilder::new();
    options.force();
    repository
        .checkout_head(Some(&mut options))
        .expect("强制检出 HEAD");
    repository.cleanup_state().expect("清理合并状态");
}

/// 一份可被克隆的配置工作区内容。
///
/// `manifest.json` 用 `PluginManifestFile::defaults()` 写成，保证它是 ticket 06
/// 定义的合法清单（原先手写的 `{"plugins": []}` 缺少 `schemaVersion`，会被判为
/// 无效清单）。`theme.json` 保持 ticket 14 记录过的旧写法，`recorded_theme()` 仍能
/// 如实读出 `dark`；拉取测试会换成 ticket 06 的合法格式来验证主题重新加载。
///
/// `settings.toml` 由 [`Settings::to_toml`] 生成，而不是手写字符串：新增设置字段
/// （例如 ticket 09 的剪贴板暂停 / 保留期限 / 容量）时夹具与期望值会一起跟上，
/// 不会出现「远端夹具缺字段、期望值有字段」这类只在个别用例上失败的分歧。
pub fn workspace_files(hotkey: &str) -> Vec<(&'static str, String)> {
    let manifest = flashcast_core::PluginManifestFile::defaults()
        .to_json()
        .expect("序列化默认插件清单");
    let settings = Settings {
        hotkey: hotkey.to_string(),
        ..Settings::default()
    }
    .to_toml()
    .expect("序列化设置");
    vec![
        ("settings.toml", settings),
        ("manifest.json", manifest),
        ("theme.json", "{\"theme\": \"dark\"}\n".to_string()),
        (
            "memos/hello.md",
            "# 你好\n\n来自远端的备忘录。\n".to_string(),
        ),
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
    // CI runner 上没有全局 `user.name` / `user.email`，而开发机上的 `~/.gitconfig` 通常有，
    // `Repository::signature()` 会回退到全局配置——于是同一份测试在本地通过、在四条 CI 腿上
    // 全部失败。这里保证仓库自带身份：已有身份（测试显式设置过的）保持不动。
    {
        let probe = git2::Repository::open(repo).expect("打开测试仓库");
        if probe.signature().is_err() {
            let mut config = probe.config().expect("读取测试仓库配置");
            config
                .set_str("user.name", TEST_AUTHOR_NAME)
                .expect("写入 user.name");
            config
                .set_str("user.email", TEST_AUTHOR_EMAIL)
                .expect("写入 user.email");
        }
    }
    // 重新打开：上面的写入要等新快照才对 `signature()` 可见。
    let repository = git2::Repository::open(repo).expect("打开测试仓库");
    let mut index = repository.index().expect("读取索引");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("暂存全部文件");
    index.write().expect("写入索引");
    let tree_oid = index.write_tree().expect("写出树");
    let tree = repository.find_tree(tree_oid).expect("读取树");
    let signature = repository.signature().expect("测试仓库必须配置了提交身份");
    let parents = match repository.head() {
        Ok(head) => vec![head.peel_to_commit().expect("读取 HEAD 提交")],
        Err(_) => Vec::new(),
    };
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
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

/// 范围搜索的故障方式。
#[derive(Clone, Copy)]
pub enum ScopeFault {
    /// 阻塞指定时长（超过插件超时，用于验证超时隔离）。
    Hang(Duration),
    /// 返回插件错误。
    Fail,
    /// panic（用于验证 panic 隔离）。
    Panic,
}

/// 关键词入口正常、但**范围搜索**会故障的插件。
///
/// `KeywordPlugin` 的范围总是立刻成功，因此范围侧的隔离（ticket 07 新加的
/// `PluginRegistry::search_scope`）需要这个替身才能验证。
pub struct FaultyScopePlugin {
    pub manifest: PluginManifest,
    pub fault: ScopeFault,
}

impl FaultyScopePlugin {
    pub fn new(id: &str, keyword: &str, fault: ScopeFault) -> Self {
        Self {
            manifest: PluginManifest::feature(id, id, "0.1.0").with_keywords([keyword]),
            fault,
        }
    }
}

struct FaultyScope {
    plugin_id: String,
    keyword: String,
    fault: ScopeFault,
}

impl PluginScope for FaultyScope {
    fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    fn keyword(&self) -> &str {
        &self.keyword
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        match self.fault {
            ScopeFault::Hang(delay) => {
                std::thread::sleep(delay);
                Ok(Vec::new())
            }
            ScopeFault::Fail => Err(PluginError::failed("范围搜索内部错误")),
            ScopeFault::Panic => panic!("范围搜索故意 panic"),
        }
    }
}

impl FeaturePlugin for FaultyScopePlugin {
    fn manifest(&self) -> PluginManifest {
        self.manifest.clone()
    }

    fn contributes_to_home(&self) -> bool {
        // 首屏不参与，故障只发生在范围里。
        false
    }

    fn search(&self, _ctx: &SearchContext) -> Result<Vec<SearchItem>, PluginError> {
        Ok(Vec::new())
    }

    fn take_scope(&self, keyword: &Keyword) -> Option<Box<dyn PluginScope>> {
        Some(Box::new(FaultyScope {
            plugin_id: self.manifest.id.clone(),
            keyword: keyword.as_str().to_string(),
            fault: self.fault,
        }))
    }
}

// ---------------------------------------------------------------------------
// 剪贴板历史（ticket 09）
// ---------------------------------------------------------------------------

/// 剪贴板历史的测试载体：宿主 + 可观察的剪贴板 / 监听 / 粘贴替身。
///
/// 默认**停掉后台线程**：捕获由测试通过 [`ClipboardHarness::copy`] 显式驱动，结果完全
/// 确定。需要验证后台活动本身的用例显式调用 `sync_clipboard_runtime()`。
pub struct ClipboardHarness {
    pub host: Host,
    pub clipboard: Arc<FakeClipboard>,
    pub watcher: Arc<FakeClipboardWatcher>,
    pub focus: Arc<FakeFocusTracker>,
    pub paster: Arc<FakePaster>,
    pub device_dir: PathBuf,
}

impl ClipboardHarness {
    /// 启用剪贴板插件。插件默认关闭，必须显式启用后才会捕获。
    pub fn enable(&self) {
        self.host
            .set_plugin_enabled(CLIPBOARD_PLUGIN_ID, true)
            .expect("启用剪贴板插件");
        self.host.stop_clipboard_capture();
    }

    /// 模拟一次外部复制（只改剪贴板，不捕获）。
    pub fn copy_only(&self, text: &str) {
        self.watcher.set_text(text);
    }

    /// 模拟一次携带 HTML/RTF 的外部复制（只改剪贴板，不捕获）。
    ///
    /// 文本与富文本格式属于**同一次**事件：下一次 `capture_clipboard_once` 只报告一次
    /// 变化，因此只会产生一条历史（ticket 11 的关键要求）。
    pub fn copy_rich_only(&self, text: &str, html: Option<&str>, rtf: Option<&str>) {
        self.watcher.set_rich(text, html, rtf);
    }

    /// 唤起：外壳在显示窗口之前捕获前台应用，并交给宿主作为粘贴目标。
    ///
    /// 与 ticket 08 的 `PasteHarness::summon` 同一语义：焦点替身与宿主的目标必须
    /// 一致，否则 `complete_paste` 会（正确地）降级为手动粘贴。
    pub fn summon(&self, app: flashcast_platform::FocusedApp) {
        self.focus.set_active(Some(app.clone()));
        self.host.set_paste_target(Some(app));
    }

    /// 模拟一次外部复制并同步捕获一次。
    pub fn copy(&self, text: &str) -> ClipboardCaptureOutcome {
        self.copy_only(text);
        self.host.capture_clipboard_once()
    }

    /// 模拟一次携带富文本的外部复制并同步捕获一次。
    pub fn copy_rich(
        &self,
        text: &str,
        html: Option<&str>,
        rtf: Option<&str>,
    ) -> ClipboardCaptureOutcome {
        self.copy_rich_only(text, html, rtf);
        self.host.capture_clipboard_once()
    }

    /// 当前历史（置顶在前，然后按时间倒序）。
    pub fn entries(&self) -> Vec<ClipboardEvent> {
        self.host.clipboard_entries(None)
    }

    /// 当前历史条目的摘要，按显示顺序。
    pub fn summaries(&self) -> Vec<String> {
        self.entries()
            .into_iter()
            .map(|event| event.summary)
            .collect()
    }

    /// 进入剪贴板范围并取回其中的条目（经查询入口）。
    pub fn scope_items(&self, input: &str) -> Vec<SearchItem> {
        self.host.query(input).items
    }

    /// 进入范围并取回第一条剪贴板条目。
    pub fn first_item(&self, input: &str) -> SearchItem {
        self.scope_items(input)
            .into_iter()
            .find(|item| item.kind == ItemKind::ClipboardEntry)
            .unwrap_or_else(|| panic!("「{input}」范围内应有剪贴板条目"))
    }
}

/// 构造剪贴板历史的测试宿主（新设备目录）。
pub fn clipboard_host(settings: Settings) -> ClipboardHarness {
    clipboard_host_with_device(&unique_dir("clipboard-device"), settings)
}

/// 用给定的**设备目录**构造剪贴板历史的测试宿主：同一个目录即模拟「重启应用」。
pub fn clipboard_host_with_device(device_dir: &Path, settings: Settings) -> ClipboardHarness {
    clipboard_host_with_watcher(device_dir, settings, Arc::new(FakeClipboardWatcher::new()))
}

/// 同 [`clipboard_host_with_device`]，但注入调用方提供的监听替身
/// （用于模拟「拿不到剪贴板选区」这类环境问题）。
pub fn clipboard_host_with_watcher(
    device_dir: &Path,
    settings: Settings,
    watcher: Arc<FakeClipboardWatcher>,
) -> ClipboardHarness {
    clipboard_host_with_adapters(
        device_dir,
        settings,
        Arc::new(FakeClipboard::new()),
        watcher,
    )
}

/// 同 [`clipboard_host_with_device`]，但注入调用方提供的**剪贴板写入替身**。
///
/// ticket 11 用它模拟「平台只能提供纯文本」（Linux 的 `wl-copy`、macOS 的 `pbcopy`）：
/// 宿主仍然把全部格式交给平台，平台如实报告哪些没写进去。
pub fn clipboard_host_with_clipboard(
    device_dir: &Path,
    settings: Settings,
    clipboard: Arc<FakeClipboard>,
) -> ClipboardHarness {
    clipboard_host_with_adapters(
        device_dir,
        settings,
        clipboard,
        Arc::new(FakeClipboardWatcher::new()),
    )
}

/// 剪贴板历史测试宿主的最底层构造：读写与监听替身都由调用方提供。
pub fn clipboard_host_with_adapters(
    device_dir: &Path,
    settings: Settings,
    clipboard: Arc<FakeClipboard>,
    watcher: Arc<FakeClipboardWatcher>,
) -> ClipboardHarness {
    let focus = Arc::new(FakeFocusTracker::default());
    // 粘贴替身观察剪贴板：验证「注入时剪贴板里就是这条历史的内容」。
    let paster = Arc::new(FakePaster::observing(Arc::clone(&clipboard)));
    let deps = HostDeps {
        catalog: Arc::new(FakeAppCatalog::with_apps(Vec::new())),
        launcher: Arc::new(FakeLauncher::always_succeeds()),
        capabilities: Arc::new(FakeCapabilityProbe::linux_x11()),
        clipboard: clipboard.clone(),
        clipboard_watcher: watcher.clone(),
        chrome: no_chrome(),
        focus: focus.clone(),
        paster: paster.clone(),
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.to_path_buf(),
    };
    let host = Host::new(deps, settings);
    host.install_official_plugins();
    // 测试要确定性：安装官方插件可能启动后台线程，这里立刻停掉。
    host.stop_clipboard_capture();
    ClipboardHarness {
        host,
        clipboard,
        watcher,
        focus,
        paster,
        device_dir: device_dir.to_path_buf(),
    }
}

/// 构造一个「本机存储不可用」的剪贴板宿主：设备目录里 `clipboard` 是一个普通文件，
/// 因此数据库目录无法创建。宿主仍必须可用，并如实报告存储失败。
pub fn clipboard_host_with_broken_storage(settings: Settings) -> ClipboardHarness {
    let device_dir = unique_dir("clipboard-broken");
    std::fs::write(device_dir.join("clipboard"), b"not a directory").expect("写入占位文件");
    let harness = clipboard_host_with_device(&device_dir, settings);
    harness.enable();
    harness
}

/// 一个用于粘贴目标的应用身份。
pub fn focused_app(id: &str) -> flashcast_platform::FocusedApp {
    flashcast_platform::FocusedApp {
        id: id.to_string(),
        name: id.to_string(),
        wm_class: Some(id.to_string()),
        pid: Some(4242),
        window: Some(11),
    }
}
