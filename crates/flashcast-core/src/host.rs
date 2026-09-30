//! 无头宿主 `Host`。对应 ADR §2–§4：
//!
//! - `query` / `execute` 是唯一对外表达业务结果的入口；
//! - 选择状态由宿主持有，鼠标移动不调用 `set_selection`，因此不会抢走键盘选择；
//! - 记录进入查询范围时的 `input` / `scope` / `selection`，`back()` 恢复它们；
//! - 搜索结果的排序完全确定，与目录读取顺序无关。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use flashcast_platform::capability::{Capabilities, CapabilityProbe};
use flashcast_platform::catalog::{AppCatalog, AppEntry};
use flashcast_platform::launch::AppLauncher;
use flashcast_platform::launch_request::LaunchRequest;

use crate::device::DeviceStore;
use crate::manifest::{ManifestEntry, ManifestError, PluginManifestFile};
use crate::model::{
    ActionOutcome, BackOutcome, DefaultAction, ItemKind, Notice, PluginFailure, Preview,
    QueryResponse, QueryScope, Score, SearchItem, COMMAND_CAPABILITIES, COMMAND_PREFIX,
    COMMAND_RESCAN, HOST_SOURCE,
};
use crate::plugin::{PluginKind, PluginScope, SearchContext};
use crate::ranking::{score_match, sort_ranked, RankedItem};
use crate::registry::PluginRegistry;
use crate::settings::{Settings, SettingsError};
use crate::theme::{
    Appearance, ThemeDocument, ThemeEntry, ThemeError, ThemeLibrary, ThemeSelection, ThemeState,
    ThemeTokens, THEME_LIGHT,
};
use crate::watch::WorkspaceWatcher;
use crate::workspace::{write_atomic, Workspace, WorkspaceError, WorkspaceReload, WorkspaceStatus};

/// 宿主的注入依赖。不含任何 Tauri 类型。
#[derive(Clone)]
pub struct HostDeps {
    pub catalog: Arc<dyn AppCatalog>,
    pub launcher: Arc<dyn AppLauncher>,
    pub capabilities: Arc<dyn CapabilityProbe>,
    pub plugins: Arc<PluginRegistry>,
    /// 设备本地数据根目录（应用数据目录）。工作区之外的本机数据都放这里：
    /// 当前工作区的路径、缓存、设备路径、权限状态、日志与凭证。
    pub device_dir: PathBuf,
}

/// 一次查询历史。`back()` 用它恢复此前的查询、范围与选择。
#[derive(Debug, Clone)]
struct HistoryEntry {
    input: String,
    scope: QueryScope,
    selection: usize,
}

struct HostInner {
    input: String,
    scope: QueryScope,
    items: Vec<SearchItem>,
    selection: usize,
    apps: Vec<AppEntry>,
    /// 最近一次扫描失败的原因。
    apps_error: Option<String>,
    plugin_failures: Vec<PluginFailure>,
    history: Vec<HistoryEntry>,
    /// 已进入的插件范围对象。ticket 01 只用它验证 `back()` 与范围切换。
    plugin_scopes: HashMap<String, Box<dyn PluginScope>>,
    settings: Settings,
    /// 插件清单：插件标识、种类、版本与启用状态。主题与功能插件共用。
    manifest: PluginManifestFile,
    /// 主题库：内置主题 + 从工作区 `themes/` 读入的本地主题包。
    theme_library: ThemeLibrary,
    /// 当前选中的主题 id。
    selected_theme: String,
    /// 当前系统外观。「跟随系统」的主题据此解析。
    system_appearance: Appearance,
    /// 最近一次成功解析出的 token：无效主题时保留它，即「上一次可用外观」。
    theme_tokens: ThemeTokens,
    /// 最近一次成功解析出的实际外观。
    theme_appearance: Appearance,
    /// 当前选中主题解析失败的中文原因（持续到该主题重新可用）。
    theme_error: Option<String>,
    /// 工作区主题配置 / 插件清单的问题（持续到配置修好或被重新选择）。
    theme_notice: Option<String>,
    /// 当前配置工作区；`None` 表示尚未关联。
    workspace: Option<Workspace>,
    /// 最近一次工作区失败的中文原因。
    workspace_error: Option<String>,
    /// 已应用的外部重载次数（自写抑制的观测点）。
    reloads: u64,
}

impl HostInner {
    /// 文件的 `.desktop` id 去掉扩展名，作为元数据参与匹配。
    fn desktop_stem(entry: &AppEntry) -> String {
        entry
            .id
            .strip_suffix(".desktop")
            .unwrap_or(&entry.id)
            .to_string()
    }
}

/// 无头宿主。
pub struct Host {
    inner: Mutex<HostInner>,
    seq: AtomicU64,
    deps: HostDeps,
    /// 设备本地存储：位于应用数据目录，与配置工作区分离。
    device: DeviceStore,
    /// 当前工作区的文件监听器。切换工作区时整体替换。
    watch: Mutex<Option<WorkspaceWatcher>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// [`Host::wait_for_workspace_change`] 的轮询间隔。
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// 查询方式：用户输入会改变输入状态，快照类操作不会。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    UserInput,
}

impl Host {
    /// 构造宿主。首次扫描在此完成，扫描失败会记录为提示，不影响宿主创建。
    ///
    /// 构造时会尝试恢复上次使用的配置工作区（指针保存在设备本地存储里）；
    /// 恢复失败不影响宿主可用性，只把原因记录到工作区状态。
    pub fn new(deps: HostDeps, settings: Settings) -> Self {
        let device = DeviceStore::new(deps.device_dir.clone());
        let host = Self {
            inner: Mutex::new(HostInner {
                input: String::new(),
                scope: QueryScope::Home,
                items: Vec::new(),
                selection: 0,
                apps: Vec::new(),
                apps_error: None,
                plugin_failures: Vec::new(),
                history: Vec::new(),
                plugin_scopes: HashMap::new(),
                settings,
                // 默认清单 = 浅色 / 深色 / 跟随系统三个默认主题。
                manifest: PluginManifestFile::defaults(),
                theme_library: ThemeLibrary::with_builtins(),
                selected_theme: THEME_LIGHT.to_string(),
                system_appearance: Appearance::Light,
                theme_tokens: crate::theme::light_tokens(),
                theme_appearance: Appearance::Light,
                theme_error: None,
                theme_notice: None,
                workspace: None,
                workspace_error: None,
                reloads: 0,
            }),
            seq: AtomicU64::new(0),
            deps,
            device,
            watch: Mutex::new(None),
        };
        host.rescan_catalog();
        host.restore_workspace();
        host
    }

    /// 便捷构造：使用给定设置。
    pub fn with_deps(deps: HostDeps) -> Self {
        Self::new(deps, Settings::default())
    }

    pub fn plugins(&self) -> &Arc<PluginRegistry> {
        &self.deps.plugins
    }

    /// 设备本地数据存储（应用数据目录）。
    pub fn device(&self) -> &DeviceStore {
        &self.device
    }

    pub fn settings(&self) -> Settings {
        lock(&self.inner).settings.clone()
    }

    /// 更新设置：先落到工作区文件，成功后才改为生效状态。
    ///
    /// 未关联工作区时设置只在内存中生效（`workspace_status().persisted == false`）。
    /// 写入失败时保留上一次可用状态并返回中文原因。
    pub fn update_settings(&self, settings: Settings) -> Result<Settings, SettingsError> {
        settings.validate()?;
        self.persist_settings(&settings)?;
        let mut inner = lock(&self.inner);
        inner.settings = settings.clone();
        Ok(settings)
    }

    /// 当前工作区与它的有效性。
    pub fn workspace_status(&self) -> WorkspaceStatus {
        let inner = lock(&self.inner);
        match &inner.workspace {
            Some(workspace) => {
                let mut status = WorkspaceStatus::linked(workspace);
                status.error = inner.workspace_error.clone();
                status.valid = inner.workspace_error.is_none();
                status
            }
            None => WorkspaceStatus::unlinked(inner.workspace_error.clone()),
        }
    }

    /// 把一个已存在的本地仓库 / 目录关联为当前工作区。
    ///
    /// 先校验目标目录与它已有的配置；失败时当前工作区与有效设置都保持不变。
    pub fn select_workspace(&self, path: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
        let workspace = Workspace::open(path)?;
        self.activate_workspace(workspace)
    }

    /// 在新目录（必须为空或不存在）上初始化工作区**及其 Git 仓库**。
    ///
    /// 非空目录一律拒绝，绝不覆盖已有用户文件。
    pub fn init_workspace(&self, path: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
        let workspace = Workspace::init(path)?;
        self.activate_workspace(workspace)
    }

    /// Git 操作忙标志：置位期间丢弃工作区文件事件。
    ///
    /// ticket 15/16 的 status / commit / pull 用它包住整个 Git 操作，操作结束后按
    /// Git 状态显式重建界面（见 [`crate::watch`] 的模块文档）。
    pub fn set_git_busy(&self, busy: bool) {
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.set_git_busy(busy);
        }
    }

    /// 已应用的外部重载次数。
    pub fn workspace_reloads(&self) -> u64 {
        lock(&self.inner).reloads
    }

    /// 显式重新读取工作区配置（UI 的「重新检测」入口）。
    pub fn reload_workspace(&self) -> WorkspaceReload {
        let path = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.settings_path(),
            None => return self.unchanged_reload(PathBuf::new()),
        };
        self.reload_from_workspace(&path)
    }

    /// 等待并处理一次外部修改。
    ///
    /// 阻塞至多 `timeout`：超时、没有工作区或事件被自写抑制层吞掉时返回 `None`。
    /// 外壳在后台线程里循环调用它并把结果推送给 UI。
    ///
    /// 实现为短轮询而不是在监听通道上阻塞等待：等待期间不持有工作区监听锁，
    /// 保存设置或切换工作区不会被这里卡住。
    pub fn wait_for_workspace_change(&self, timeout: Duration) -> Option<WorkspaceReload> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let changed = {
                let watch = lock(&self.watch);
                watch.as_ref().and_then(|watcher| watcher.try_next_change())
            };
            if let Some(changed) = changed {
                return Some(self.reload_from_workspace(&changed));
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(WATCH_POLL_INTERVAL);
        }
    }

    /// 按外部修改重新加载配置。无效配置保留上一次有效状态并给出中文原因。
    fn reload_from_workspace(&self, changed: &Path) -> WorkspaceReload {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return self.unchanged_reload(changed.to_path_buf()),
        };
        let mut messages: Vec<String> = Vec::new();
        let settings = match workspace.read_settings() {
            Ok(Some(settings)) => Some(settings),
            // 设置文件被删除：保留上一次有效设置，但主题与清单仍按工作区重读。
            Ok(None) => {
                messages.push(format!(
                    "设置文件已不存在，继续使用上一次有效设置（{}）",
                    workspace.settings_path().display()
                ));
                None
            }
            Err(error) => {
                let message = error.to_string();
                lock(&self.inner).workspace_error = Some(message.clone());
                return WorkspaceReload {
                    path: changed.to_path_buf(),
                    applied: false,
                    settings: self.settings(),
                    theme: self.theme_state(),
                    error: Some(message),
                };
            }
        };

        let applied = {
            let mut inner = lock(&self.inner);
            let mut applied = false;
            if let Some(settings) = settings {
                if settings != inner.settings {
                    inner.settings = settings;
                    applied = true;
                }
            }
            let (theme_changed, theme_reason) =
                self.apply_workspace_config(&mut inner, &workspace, false);
            applied |= theme_changed;
            inner.theme_notice = theme_reason;
            if applied {
                inner.workspace_error = None;
                inner.reloads += 1;
            }
            applied
        };

        let theme = self.theme_state();
        if let Some(error) = theme.error.clone() {
            messages.push(error);
        }
        WorkspaceReload {
            path: changed.to_path_buf(),
            applied,
            settings: self.settings(),
            error: if messages.is_empty() {
                None
            } else {
                Some(messages.join("；"))
            },
            theme,
        }
    }

    /// 从工作区读取插件清单、已安装主题包与当前主题，成功部分立即生效。
    ///
    /// 返回（是否有变化，主题相关的中文原因）。任何一部分失败都不会动到
    /// 「上一次可用外观」：调用方只把原因展示出来。
    ///
    /// `activation` 为真表示正在**切换**配置工作区：目标工作区缺少清单或主题配置时
    /// 回到默认（三个内置主题 + 浅色），而不是沿用上一个工作区的选择。重新加载
    /// （`activation == false`）时缺少文件则保留当前状态，只说明原因。
    fn apply_workspace_config(
        &self,
        inner: &mut HostInner,
        workspace: &Workspace,
        activation: bool,
    ) -> (bool, Option<String>) {
        let mut changed = false;
        let mut reason: Option<String> = None;

        // 1. 插件清单。文件不存在时保留当前清单（例如用户还没提交过）。
        match workspace.read_config_text(&workspace.manifest_path()) {
            Ok(Some(text)) => match PluginManifestFile::from_json(&text) {
                Ok(file) => {
                    let extras = self.expected_manifest_extras();
                    let (merged, _) = file.merged_with(extras);
                    if merged != inner.manifest {
                        inner.manifest = merged;
                        changed = true;
                    }
                }
                Err(error) => reason = Some(format!("插件清单无效：{error}")),
            },
            Ok(None) if activation => {
                let (merged, _) = PluginManifestFile::defaults()
                    .merged_with(self.expected_manifest_extras());
                if merged != inner.manifest {
                    inner.manifest = merged;
                    changed = true;
                }
            }
            Ok(None) => {}
            Err(error) => reason = Some(format!("无法读取插件清单：{error}")),
        }

        // 2. 已安装的本地主题包（清单里 origin = installed 的主题）。
        if self.reload_theme_packages(inner, workspace) {
            changed = true;
        }

        // 3. 当前选中的主题。不可用时不切换，保留上一次可用外观。
        let target: Option<String> = match workspace.read_config_text(&workspace.theme_path()) {
            Ok(Some(text)) => match ThemeSelection::from_json(&text) {
                Ok(selection) => Some(selection.selected),
                Err(error) => {
                    reason = Some(error.to_string());
                    None
                }
            },
            // 目标工作区没有主题配置：切换工作区时回到默认主题。
            Ok(None) if activation => Some(THEME_LIGHT.to_string()),
            Ok(None) => None,
            Err(error) => {
                reason = Some(format!("无法读取主题配置：{error}"));
                None
            }
        };
        if let Some(target) = target {
            match () {
                () if target == inner.selected_theme => {}
                () => {
                    let usable = inner
                        .manifest
                        .get(&target)
                        .filter(|entry| entry.is_theme() && entry.enabled)
                        .and_then(|_| inner.theme_library.document(&target).ok())
                        .map(|document| document.resolve(inner.system_appearance).is_ok())
                        .unwrap_or(false);
                    if usable {
                        inner.selected_theme = target.clone();
                        inner.theme_error = None;
                        inner.theme_notice = None;
                        changed = true;
                    } else {
                        reason = Some(format!(
                            "主题「{target}」不可用（不存在、已停用或无法解析），继续使用上一次可用外观"
                        ));
                    }
                }
            }
        }

        (changed, reason)
    }

    /// 重新读取清单里登记的本地主题包。返回已安装主题集合是否发生变化。
    fn reload_theme_packages(&self, inner: &mut HostInner, workspace: &Workspace) -> bool {
        let ids: Vec<String> = inner
            .manifest
            .entries()
            .iter()
            .filter(|entry| {
                entry.is_theme() && entry.origin == crate::manifest::PluginOrigin::Installed
            })
            .map(|entry| entry.id.clone())
            .collect();
        let before = inner.theme_library.snapshot();
        for id in &ids {
            let path = workspace.theme_package_path(id);
            match workspace.read_config_text(&path) {
                Ok(Some(text)) => match ThemeDocument::from_json(&text) {
                    Ok(document) => inner.theme_library.install(document),
                    Err(error) => inner.theme_library.mark_broken(id, error.to_string()),
                },
                Ok(None) => inner
                    .theme_library
                    .mark_broken(id, format!("主题包文件不存在：{}", path.display())),
                Err(error) => inner.theme_library.mark_broken(
                    id,
                    format!("无法读取主题包 {}：{error}", path.display()),
                ),
            }
        }
        // 清单里已经不存在的本地主题从库里清掉。
        for id in inner.theme_library.installed_ids() {
            if !ids.contains(&id) {
                inner.theme_library.remove(&id);
            }
        }
        inner.theme_library.snapshot() != before
    }

    fn unchanged_reload(&self, path: PathBuf) -> WorkspaceReload {
        WorkspaceReload {
            path,
            applied: false,
            settings: self.settings(),
            theme: self.theme_state(),
            error: None,
        }
    }

    /// 让一个已校验的工作区成为当前工作区，并开始监听它的变更。
    fn activate_workspace(
        &self,
        workspace: Workspace,
    ) -> Result<WorkspaceStatus, WorkspaceError> {
        // 先读设置：无效则整体拒绝，当前工作区与设置原样保留。
        let loaded = workspace.read_settings()?;
        let watcher = WorkspaceWatcher::start(workspace.root(), workspace.git_dir().map(Path::to_path_buf))
            .map_err(|error| WorkspaceError::Watch(error.to_string()))?;
        {
            let mut inner = lock(&self.inner);
            if let Some(settings) = loaded {
                inner.settings = settings;
            }
            inner.workspace = Some(workspace.clone());
            inner.workspace_error = None;
            // 插件清单 / 主题配置 / 已安装主题包。切换工作区时以目标工作区为准；
            // 读不到的部分保留当前外观并记下中文原因。
            let (_, theme_reason) = self.apply_workspace_config(&mut inner, &workspace, true);
            inner.theme_notice = theme_reason;
        }
        *lock(&self.watch) = Some(watcher);
        // 补齐默认文件（清单与主题选择），让用户可以手写、提交和同步。
        self.ensure_workspace_files(&workspace);
        // 记住本机路径，重启后恢复。这是设备本地数据，不写进工作区。
        if let Err(error) = self.device.set_workspace_path(Some(workspace.root())) {
            lock(&self.inner).workspace_error = Some(error.to_string());
        }
        Ok(self.workspace_status())
    }

    /// 工作区里缺哪些配置文件就补哪些。已存在的文件一律不改写。
    fn ensure_workspace_files(&self, workspace: &Workspace) {
        let existing = workspace
            .read_config_text(&workspace.manifest_path())
            .ok()
            .flatten();
        let manifest_path = workspace.manifest_path();
        match existing {
            // 已有清单：只补齐缺失的默认条目（默认主题始终可用）。
            Some(text) => match PluginManifestFile::from_json(&text) {
                Ok(file) => {
                    let extras = self.expected_manifest_extras();
                    let (merged, changed) = file.merged_with(extras);
                    if changed {
                        if let Ok(bytes) = merged.to_json().map(String::into_bytes) {
                            let _ = self.persist_workspace_file(|_| manifest_path.clone(), &bytes);
                            lock(&self.inner).manifest = merged;
                        }
                    }
                }
                // 无效清单不覆盖用户的文件：原因已经在 apply_workspace_config 里记过。
                Err(_) => {}
            },
            None => {
                let extras = self.expected_manifest_extras();
                let (merged, _) = PluginManifestFile::defaults().merged_with(extras);
                if let Ok(bytes) = merged.to_json().map(String::into_bytes) {
                    if self
                        .persist_workspace_file(|_| manifest_path.clone(), &bytes)
                        .is_ok()
                    {
                        lock(&self.inner).manifest = merged;
                    }
                }
            }
        }

        // 尚未选择主题时补一份默认主题配置。
        let theme_path = workspace.theme_path();
        if let Ok(None) = workspace.read_config_text(&theme_path) {
            let selected = lock(&self.inner).selected_theme.clone();
            if let Ok(selection) = ThemeSelection::new(selected).to_json() {
                let _ = self.persist_workspace_file(|_| theme_path.clone(), selection.as_bytes());
            }
        }
    }

    /// 清单里应该始终存在的条目：默认主题 + 由宿主代码注册的功能插件。
    fn expected_manifest_extras(&self) -> Vec<ManifestEntry> {
        let mut extras: Vec<ManifestEntry> = crate::theme::builtin_themes()
            .iter()
            .map(|document| ManifestEntry::from_theme(document, true))
            .collect();
        for (manifest, enabled) in self.deps.plugins.manifests() {
            extras.push(ManifestEntry::from_feature(&manifest, enabled));
        }
        extras
    }

    /// 启动时恢复上次使用的配置工作区。
    fn restore_workspace(&self) {
        let path = match self.device.workspace_path() {
            Ok(Some(path)) => path,
            Ok(None) => return,
            Err(error) => {
                lock(&self.inner).workspace_error = Some(error.to_string());
                return;
            }
        };
        let workspace = match Workspace::open(&path) {
            Ok(workspace) => workspace,
            Err(error) => {
                lock(&self.inner).workspace_error =
                    Some(format!("上次使用的配置工作区已不可用：{error}"));
                return;
            }
        };
        // 已有设置文件但无效时不切换，保留构造时传入的设置并说明原因。
        if let Err(error) = self.activate_workspace(workspace) {
            lock(&self.inner).workspace_error =
                Some(format!("上次使用的配置工作区无法恢复：{error}"));
        }
    }

    /// 把设置写入当前工作区文件。
    fn persist_settings(&self, settings: &Settings) -> Result<(), SettingsError> {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Ok(()),
        };
        let bytes = settings.to_toml()?.into_bytes();
        {
            let watch = lock(&self.watch);
            if let Some(watcher) = watch.as_ref() {
                // 先记账再写文件：监听回调可能在写入后立刻看到事件。
                watcher.record_self_write(&workspace.settings_path(), &bytes);
            }
        }
        workspace
            .write_settings_bytes(&bytes)
            .map_err(|error| SettingsError::Workspace(error.to_string()))
    }

    /// 平台能力快照（如实反映当前环境）。
    pub fn capabilities(&self) -> Capabilities {
        self.deps.capabilities.probe()
    }

    /// 插件清单与启用状态（功能插件）。
    ///
    /// 启用状态以工作区里的 `manifest.json` 为准；清单里还没有记录的插件
    /// （例如刚由宿主代码注册的功能插件）按注册表的当前状态补进清单。
    pub fn plugin_manifests(&self) -> Vec<(crate::plugin::PluginManifest, bool)> {
        let mut inner = lock(&self.inner);
        let extras: Vec<ManifestEntry> = self
            .deps
            .plugins
            .manifests()
            .into_iter()
            .map(|(manifest, enabled)| {
                let mut entry = ManifestEntry::from_feature(&manifest, enabled);
                if let Some(existing) = inner.manifest.get(&manifest.id) {
                    entry.enabled = existing.enabled;
                }
                entry
            })
            .collect();
        let (merged, _) = inner.manifest.merged_with(extras);
        inner.manifest = merged;
        inner
            .manifest
            .entries()
            .iter()
            .filter(|entry| entry.kind == PluginKind::Feature)
            .map(|entry| (entry.to_feature_manifest(), entry.enabled))
            .collect()
    }

    /// 清单里的全部插件条目（功能插件与主题插件）。
    pub fn manifest_entries(&self) -> Vec<ManifestEntry> {
        // 先让功能插件条目与注册表同步。
        let _ = self.plugin_manifests();
        lock(&self.inner).manifest.entries().to_vec()
    }

    // -----------------------------------------------------------------------
    // 主题
    // -----------------------------------------------------------------------

    /// 当前主题状态：选中的主题、解析后的 token、CSS 自定义属性与可选主题列表。
    ///
    /// 选中的主题无法解析时保留 `theme_tokens` 里的上一次可用外观，并把中文原因
    /// 放进 `error`（`tokens` 仍然是可用的）。
    pub fn theme_state(&self) -> ThemeState {
        let mut inner = lock(&self.inner);
        Self::resolve_theme_state(&mut inner)
    }

    /// 选择主题。主题不存在、已停用或无法解析时不切换，只返回中文原因。
    pub fn select_theme(&self, id: &str) -> Result<ThemeState, ThemeError> {
        {
            let inner = lock(&self.inner);
            let entry = inner
                .manifest
                .get(id)
                .filter(|entry| entry.is_theme())
                .ok_or_else(|| ThemeError::Unknown(id.to_string()))?;
            if !entry.enabled {
                return Err(ThemeError::Disabled(entry.name.clone()));
            }
            inner
                .theme_library
                .document(id)?
                .resolve(inner.system_appearance)?;
        }
        // 先落盘再改生效状态：写入失败时保持原有主题可用。
        let selection = ThemeSelection::new(id);
        let bytes = selection.to_json()?.into_bytes();
        self.persist_workspace_file(|workspace| workspace.theme_path(), &bytes)
            .map_err(|error| ThemeError::Workspace(error.to_string()))?;
        {
            let mut inner = lock(&self.inner);
            inner.selected_theme = id.to_string();
            inner.theme_error = None;
            inner.theme_notice = None;
        }
        Ok(self.theme_state())
    }

    /// 安装（或更新）一个本地主题包。
    ///
    /// 主题包可以是包含 `theme.json` 的目录，也可以直接是主题 JSON 文件。
    /// 校验失败返回可读的中文原因，并且不改动任何已安装内容与当前外观。
    pub fn install_theme_package(&self, path: &Path) -> Result<ThemeState, ThemeError> {
        let document = ThemeDocument::from_package_path(path)?;
        if document.is_builtin() {
            return Err(ThemeError::Builtin("覆盖", document.id.clone()));
        }
        let workspace = self
            .workspace()?
            .ok_or(ThemeError::NoWorkspace("安装"))?;
        let package = workspace.theme_package_path(&document.id);
        let bytes = document.to_json()?.into_bytes();
        self.persist_workspace_file(|_| package.clone(), &bytes)
            .map_err(|error| ThemeError::Workspace(error.to_string()))?;

        let (manifest, entry) = {
            let mut inner = lock(&self.inner);
            inner.theme_library.install(document.clone());
            inner
                .manifest
                .upsert(ManifestEntry::from_theme(&document, true));
            (
                inner.manifest.clone(),
                inner.manifest.get(&document.id).cloned(),
            )
        };
        let _ = entry;
        self.persist_manifest(&manifest).map_err(|error| ThemeError::Workspace(error.to_string()))?;
        Ok(self.theme_state())
    }

    /// 移除一个已安装的本地主题包。内置主题不能移除。
    pub fn remove_theme(&self, id: &str) -> Result<ThemeState, ThemeError> {
        let entry = {
            let inner = lock(&self.inner);
            inner
                .manifest
                .get(id)
                .cloned()
                .filter(|entry| entry.is_theme())
                .ok_or_else(|| ThemeError::Unknown(id.to_string()))?
        };
        if !entry.origin.removable() {
            return Err(ThemeError::Builtin("移除", entry.name.clone()));
        }
        let workspace = self
            .workspace()?
            .ok_or(ThemeError::NoWorkspace("移除"))?;
        let dir = workspace.theme_package_dir(id);
        match std::fs::remove_dir_all(&dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(ThemeError::Workspace(format!(
                    "无法删除主题包 {}：{error}",
                    dir.display()
                )))
            }
        }
        let (manifest, fallback) = {
            let mut inner = lock(&self.inner);
            inner.theme_library.remove(id);
            inner.manifest.remove(id);
            let fallback = if inner.selected_theme == id {
                inner.selected_theme = THEME_LIGHT.to_string();
                inner.theme_error = None;
                Some(ThemeSelection::new(THEME_LIGHT))
            } else {
                None
            };
            (inner.manifest.clone(), fallback)
        };
        self.persist_manifest(&manifest).map_err(|error| ThemeError::Workspace(error.to_string()))?;
        let notice = fallback.as_ref().map(|_| {
            format!("主题「{}」已移除，已切换回「浅色」", entry.name)
        });
        if let Some(selection) = fallback {
            let bytes = selection.to_json()?.into_bytes();
            self.persist_workspace_file(|workspace| workspace.theme_path(), &bytes)
                .map_err(|error| ThemeError::Workspace(error.to_string()))?;
        }
        let mut state = self.theme_state();
        if let Some(notice) = notice {
            state.error = Some(notice);
        }
        Ok(state)
    }

    /// 启用或停用插件（功能插件与主题插件共用）。
    ///
    /// 停用当前选中的主题会退回内置浅色主题，并给出中文原因。
    pub fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<(), ManifestError> {
        let (manifest, selection, notice) = {
            let mut inner = lock(&self.inner);
            let entry = inner.manifest.get(id).cloned().ok_or_else(|| {
                ManifestError::invalid(format!("插件清单里没有这个标识：{id}"))
            })?;
            inner.manifest.set_enabled(id, enabled);
            if entry.kind == PluginKind::Feature {
                self.deps.plugins.set_enabled(id, enabled);
            }
            let mut notice = None;
            let selection = if entry.kind == PluginKind::Theme
                && !enabled
                && inner.selected_theme == id
            {
                inner.selected_theme = THEME_LIGHT.to_string();
                inner.theme_error = None;
                notice = Some(format!("主题「{}」已停用，已切换回「浅色」", entry.name));
                Some(ThemeSelection::new(THEME_LIGHT))
            } else {
                None
            };
            (inner.manifest.clone(), selection, notice)
        };
        self.persist_manifest(&manifest)?;
        if let Some(selection) = selection {
            let bytes = selection
                .to_json()
                .map_err(|error| ManifestError::Io(error.to_string()))?;
            self.persist_workspace_file(|workspace| workspace.theme_path(), bytes.as_bytes())
                .map_err(|error| ManifestError::Io(error.to_string()))?;
        }
        if let Some(notice) = notice {
            // 一次性反馈：通过主题状态回传给调用方，不长期占用错误位置。
            lock(&self.inner).theme_error = Some(notice);
        }
        Ok(())
    }

    /// 当前系统外观。UI 用 `prefers-color-scheme` 的变化调用它，
    /// 「跟随系统」的主题因此能在运行时跟着系统外观切换。
    pub fn set_system_appearance(&self, appearance: Appearance) -> ThemeState {
        {
            let mut inner = lock(&self.inner);
            inner.system_appearance = appearance;
        }
        self.theme_state()
    }

    pub fn system_appearance(&self) -> Appearance {
        lock(&self.inner).system_appearance
    }

    /// 当前工作区（未关联时为 `None`）。
    fn workspace(&self) -> Result<Option<Workspace>, ThemeError> {
        Ok(lock(&self.inner).workspace.clone())
    }

    /// 解析主题状态。成功时更新「上一次可用外观」，失败时保留它。
    fn resolve_theme_state(inner: &mut HostInner) -> ThemeState {
        let selected = inner.selected_theme.clone();
        let entry = inner.manifest.get(&selected).cloned();
        let mut error: Option<String> = None;
        match entry {
            None => {
                error = Some(format!(
                    "找不到已选中的主题「{selected}」，继续使用上一次可用外观"
                ))
            }
            Some(entry) if !entry.is_theme() => {
                error = Some(format!(
                    "「{}」不是主题插件，继续使用上一次可用外观",
                    entry.name
                ))
            }
            Some(entry) if !entry.enabled => error = Some(format!(
                "主题「{}」已停用，继续使用上一次可用外观",
                entry.name
            )),
            Some(_) => {
                let resolved = inner
                    .theme_library
                    .document(&selected)
                    .and_then(|document| {
                        document
                            .resolve(inner.system_appearance)
                            .map(|tokens| (document.clone(), tokens))
                    });
                match resolved {
                    Ok((document, tokens)) => {
                        inner.theme_appearance = document.resolved_appearance(inner.system_appearance);
                        inner.theme_tokens = tokens;
                    }
                    Err(failure) => error = Some(failure.to_string()),
                }
            }
        }
        // 解析失败的原因优先；没有解析失败时，回退到工作区配置层的问题。
        if error.is_none() {
            error = inner.theme_notice.clone();
        }
        // 解析成功即清掉「选中主题不可用」的原因；配置层问题保留到配置修好为止。
        inner.theme_error = if inner
            .theme_library
            .document(&inner.selected_theme)
            .map(|document| document.resolve(inner.system_appearance).is_ok())
            .unwrap_or(false)
        {
            None
        } else {
            error.clone()
        };

        let themes = inner
            .manifest
            .entries()
            .iter()
            .filter(|entry| entry.is_theme())
            .map(|entry| {
                let (usable, theme_error) = match inner.theme_library.document(&entry.id) {
                    Ok(document) => (document.resolve(inner.system_appearance).is_ok(), None),
                    Err(failure) => (false, Some(failure.to_string())),
                };
                ThemeEntry {
                    id: entry.id.clone(),
                    name: entry.name.clone(),
                    version: entry.version.clone(),
                    enabled: entry.enabled,
                    selected: entry.id == selected,
                    builtin: entry.origin == crate::manifest::PluginOrigin::Builtin,
                    appearance: entry.appearance.unwrap_or(crate::theme::ThemeAppearance::Light),
                    usable,
                    error: theme_error,
                }
            })
            .collect();

        let selected_entry = inner.manifest.get(&selected);
        ThemeState {
            selected: selected.clone(),
            selected_name: selected_entry
                .map(|entry| entry.name.clone())
                .unwrap_or_else(|| selected.clone()),
            preference: selected_entry
                .and_then(|entry| entry.appearance)
                .unwrap_or(crate::theme::ThemeAppearance::Light),
            appearance: inner.theme_appearance,
            system_appearance: inner.system_appearance,
            tokens: inner.theme_tokens.clone(),
            css_vars: inner.theme_tokens.css_vars(),
            themes,
            // 解析失败原因与配置层问题合并后的结果。
            error: error.clone(),
        }
    }

    /// 写入当前工作区的一个文件（原子 + 自写抑制）。未关联工作区时只在内存生效。
    fn persist_workspace_file(
        &self,
        path_of: impl Fn(&Workspace) -> std::path::PathBuf,
        bytes: &[u8],
    ) -> Result<(), WorkspaceError> {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Ok(()),
        };
        let path = path_of(&workspace);
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        {
            let watch = lock(&self.watch);
            if let Some(watcher) = watch.as_ref() {
                // 先记账再写文件：监听回调可能在写入后立刻看到事件。
                watcher.record_self_write(&path, bytes);
            }
        }
        write_atomic(&path, bytes)
    }

    /// 把插件清单写进工作区（原子写入 + 自写抑制）。
    fn persist_manifest(&self, manifest: &PluginManifestFile) -> Result<(), ManifestError> {
        let bytes = manifest.to_json()?.into_bytes();
        self.persist_workspace_file(|workspace| workspace.manifest_path(), &bytes)
            .map_err(|error| ManifestError::Io(error.to_string()))
    }

    /// 按 id 查找最近一次查询结果中的条目。UI 用它把命令入参还原为完整条目。
    pub fn item_by_id(&self, id: &str) -> Option<SearchItem> {
        lock(&self.inner)
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
    }

    /// 当前状态快照（不重新搜索）。
    pub fn snapshot(&self) -> QueryResponse {
        let seq = self.next_seq();
        let inner = lock(&self.inner);
        self.response(&inner, seq, None)
    }

    /// 查询入口。
    pub fn query(&self, input: &str) -> QueryResponse {
        let seq = self.next_seq();
        let mut inner = lock(&self.inner);
        self.search(&mut inner, input, SearchMode::UserInput, None, seq)
    }

    /// 键盘选择：设置为指定下标（越界时收敛到有效范围）。
    pub fn set_selection(&self, index: usize) -> QueryResponse {
        self.move_selection_to(index)
    }

    /// 键盘选择：在当前位置移动 `delta`。
    pub fn select(&self, delta: isize) -> QueryResponse {
        let current = lock(&self.inner).selection as isize;
        let target = current.saturating_add(delta).max(0) as usize;
        self.move_selection_to(target)
    }

    /// 键盘选择：在当前位置移动 `delta`。与 [`Host::select`] 等价，命名更贴近调用点。
    pub fn move_selection(&self, delta: isize) -> QueryResponse {
        self.select(delta)
    }

    fn move_selection_to(&self, index: usize) -> QueryResponse {
        let seq = self.next_seq();
        let mut inner = lock(&self.inner);
        let max = inner.items.len().saturating_sub(1);
        // 上下键在首尾处不循环，避免误触跳到最后一项。
        inner.selection = index.min(max);
        if inner.items.is_empty() {
            inner.selection = 0;
        }
        self.response(&inner, seq, None)
    }

    /// 返回上一查询范围并恢复此前的查询与选择。已在最外层时 `restored == false`。
    pub fn back(&self) -> BackOutcome {
        let seq = self.next_seq();
        let mut inner = lock(&self.inner);
        let Some(entry) = inner.history.pop() else {
            return BackOutcome {
                restored: false,
                response: self.response(&inner, seq, None),
            };
        };
        inner.input = entry.input;
        inner.scope = entry.scope;
        inner.plugin_scopes.clear();
        // 恢复历史后按恢复的输入重新计算结果，保证列表与输入一致。
        let input = inner.input.clone();
        let mut response = self.search(&mut inner, &input, SearchMode::UserInput, None, seq);
        // `search` 会把「新输入」的选择归零；返回上一范围必须恢复历史选择。
        let max = inner.items.len().saturating_sub(1);
        inner.selection = entry.selection.min(max);
        response.selection = inner.selection;
        response.items = inner.items.clone();
        response.input = inner.input.clone();
        response.scope = inner.scope.clone();
        BackOutcome {
            restored: true,
            response,
        }
    }

    /// 重新扫描软件目录，并按当前输入重算结果。
    pub fn rescan(&self) -> QueryResponse {
        let seq = self.next_seq();
        let notice = self.rescan_catalog();
        let mut inner = lock(&self.inner);
        let input = inner.input.clone();
        self.search(&mut inner, &input, SearchMode::UserInput, Some(notice), seq)
    }

    /// 命令入口。
    pub fn execute(&self, item: &SearchItem) -> ActionOutcome {
        match item.kind {
            ItemKind::Application => self.execute_application(item),
            ItemKind::Command => self.execute_command(item),
            other => ActionOutcome::failed(format!(
                "当前版本尚不支持执行这类条目（{other:?}），相关功能将在后续版本提供"
            )),
        }
    }

    fn execute_application(&self, item: &SearchItem) -> ActionOutcome {
        let entry_id = item
            .id
            .strip_prefix("app:")
            .unwrap_or(item.id.as_str())
            .to_string();
        let entry = {
            let inner = lock(&self.inner);
            inner.apps.iter().find(|a| a.id == entry_id).cloned()
        };
        let Some(entry) = entry else {
            return ActionOutcome::failed(format!(
                "找不到「{}」对应的软件条目，请重新扫描软件列表",
                item.title
            ));
        };
        let Some(argv) = entry.argv() else {
            return ActionOutcome::failed(format!("「{}」没有可执行的启动命令", item.title));
        };
        let request = LaunchRequest {
            program: argv[0].clone(),
            args: argv[1..].to_vec(),
            terminal: entry.terminal,
            working_dir: entry.working_dir.clone(),
        };
        match self.deps.launcher.launch(&request) {
            Ok(_receipt) => ActionOutcome::done(None),
            Err(error) => ActionOutcome::failed(format!("无法启动「{}」：{error}", item.title)),
        }
    }

    fn execute_command(&self, item: &SearchItem) -> ActionOutcome {
        match item.id.as_str() {
            COMMAND_RESCAN => {
                let response = self.rescan();
                let count = response.items.len();
                ActionOutcome::done(Some(format!("已重新扫描软件列表，当前结果 {count} 条")))
            }
            COMMAND_CAPABILITIES => ActionOutcome::done(Some(self.capabilities_summary())),
            other => ActionOutcome::failed(format!("未知命令：{other}")),
        }
    }

    /// 能力摘要，用于「查看平台能力」快速访问项的中文反馈。
    fn capabilities_summary(&self) -> String {
        let capabilities = self.capabilities();
        let mut parts = vec![
            format!("系统 {} {}", capabilities.os.as_str(), capabilities.arch),
            format!("会话 {}", capabilities.session.label_zh()),
            format!("全局快捷键 {}", capabilities.hotkey.label_zh()),
            format!("剪贴板 {}", capabilities.clipboard.label_zh()),
            format!("自动粘贴 {}", capabilities.auto_paste.label_zh()),
        ];
        if !capabilities.notes.is_empty() {
            parts.push(capabilities.notes.join("；"));
        }
        parts.join("，")
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// 重新扫描软件目录，返回给用户的提示。
    fn rescan_catalog(&self) -> Notice {
        let mut inner = lock(&self.inner);
        match self.deps.catalog.scan() {
            Ok(apps) => {
                let count = apps.len();
                inner.apps = apps;
                inner.apps_error = None;
                Notice::info(format!("已发现 {count} 个已安装软件"))
            }
            Err(error) => {
                let message = format!("无法读取软件列表：{error}");
                inner.apps_error = Some(message.clone());
                Notice::error(message)
            }
        }
    }

    /// 按输入重新计算结果。
    fn search(
        &self,
        inner: &mut HostInner,
        input: &str,
        mode: SearchMode,
        explicit_notice: Option<Notice>,
        seq: u64,
    ) -> QueryResponse {
        let previous_input = inner.input.clone();
        let same_input = previous_input == input && mode == SearchMode::UserInput;
        let normalized = input.trim().to_lowercase();

        // 查询范围切换：输入完整匹配插件关键词时进入该插件范围。
        if let Some((manifest, scope)) = self.deps.plugins.take_scope(input) {
            let already_in_scope = matches!(
                &inner.scope,
                QueryScope::Plugin { id, .. } if id == &manifest.id
            );
            if !already_in_scope {
                inner.history.push(HistoryEntry {
                    input: previous_input,
                    scope: inner.scope.clone(),
                    selection: inner.selection,
                });
                inner.plugin_scopes.insert(manifest.id.clone(), scope);
                inner.scope = QueryScope::Plugin {
                    id: manifest.id.clone(),
                    keyword: normalized.clone(),
                };
            }
        } else if !inner.scope.is_home() && normalized.is_empty() {
            // 清空输入即离开插件范围，回到首屏。
            inner.scope = QueryScope::Home;
            inner.plugin_scopes.clear();
        }

        inner.input = input.to_string();
        let ctx = SearchContext::new(input, inner.scope.clone(), 50);

        let (mut ranked, failures) = match inner.scope.clone() {
            QueryScope::Home => self.search_home(inner, &ctx),
            QueryScope::Plugin { id, .. } => self.search_plugin_scope(inner, &ctx, &id),
        };
        sort_ranked(&mut ranked);
        inner.items = ranked.into_iter().map(|ranked| ranked.item).collect();
        inner.plugin_failures = failures;

        // 新查询重置键盘选择；同一输入的重复渲染保留键盘选择。
        if !same_input {
            inner.selection = 0;
        }
        let max = inner.items.len().saturating_sub(1);
        inner.selection = inner.selection.min(max);

        let notice = explicit_notice.or_else(|| {
            inner
                .apps_error
                .as_ref()
                .map(|message| Notice::error(message.clone()))
        });
        self.response(inner, seq, notice)
    }

    fn search_home(
        &self,
        inner: &HostInner,
        ctx: &SearchContext,
    ) -> (Vec<RankedItem>, Vec<PluginFailure>) {
        let mut ranked = Vec::new();
        let query = ctx.query.as_str();
        let limit = inner.settings.quick_access_limit;
        let mut source_order = 0usize;

        if query.is_empty() {
            // 空查询：先给出有限数量的快速访问项（宿主命令），再给软件。
            for item in quick_access_commands() {
                ranked.push(RankedItem {
                    item,
                    source_order,
                    source_priority: 0,
                });
                source_order += 1;
            }
            let mut apps: Vec<&AppEntry> = inner.apps.iter().collect();
            apps.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
            for entry in apps.into_iter().take(limit) {
                ranked.push(RankedItem {
                    item: application_item(entry, Score::unordered()),
                    source_order,
                    source_priority: 1,
                });
                source_order += 1;
            }
            return (ranked, Vec::new());
        }

        for entry in &inner.apps {
            let stem = HostInner::desktop_stem(entry);
            let exec_name = entry
                .exec
                .first()
                .map(|program| {
                    std::path::Path::new(program)
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned())
                        .unwrap_or_else(|| program.clone())
                })
                .unwrap_or_default();
            let mut metadata: Vec<&str> = Vec::new();
            if let Some(comment) = entry.comment.as_deref() {
                metadata.push(comment);
            }
            for keyword in &entry.keywords {
                metadata.push(keyword);
            }
            metadata.push(&stem);
            if let Some(wm_class) = entry.wm_class.as_deref() {
                metadata.push(wm_class);
            }
            metadata.push(&exec_name);
            if let Some(score) = score_match(query, &entry.name, &metadata) {
                ranked.push(RankedItem {
                    item: application_item(entry, score),
                    source_order,
                    source_priority: 1,
                });
                source_order += 1;
            }
        }

        // 插件首屏贡献。每个插件只搜索一次，超时与 panic 被隔离。
        let plugin_outcome = self
            .deps
            .plugins
            .search_home(ctx, inner.settings.plugin_timeout());
        let mut source_priority = 2u32;
        for (_source, items) in plugin_outcome.results {
            for (index, item) in items.into_iter().enumerate() {
                ranked.push(RankedItem {
                    item,
                    source_order: index,
                    source_priority,
                });
            }
            source_priority += 1;
        }

        (ranked, plugin_outcome.failures)
    }

    fn search_plugin_scope(
        &self,
        inner: &HostInner,
        ctx: &SearchContext,
        plugin_id: &str,
    ) -> (Vec<RankedItem>, Vec<PluginFailure>) {
        let Some(scope) = inner.plugin_scopes.get(plugin_id) else {
            return (Vec::new(), Vec::new());
        };
        match scope.search(ctx) {
            Ok(items) => {
                let ranked = items
                    .into_iter()
                    .enumerate()
                    .map(|(index, item)| RankedItem {
                        item,
                        source_order: index,
                        source_priority: 2,
                    })
                    .collect();
                (ranked, Vec::new())
            }
            Err(error) => (
                Vec::new(),
                vec![PluginFailure {
                    plugin_id: plugin_id.to_string(),
                    reason: error.to_string(),
                    kind: crate::model::PluginFailureKind::Error,
                }],
            ),
        }
    }

    fn response(&self, inner: &HostInner, seq: u64, notice: Option<Notice>) -> QueryResponse {
        QueryResponse {
            seq,
            scope: inner.scope.clone(),
            input: inner.input.clone(),
            items: inner.items.clone(),
            selection: inner.selection,
            notice,
            plugin_failures: inner.plugin_failures.clone(),
        }
    }
}

/// 空查询下的宿主快速访问项。
pub fn quick_access_commands() -> Vec<SearchItem> {
    vec![
        command_item(
            COMMAND_RESCAN,
            "重新扫描软件",
            Some("刷新已安装软件列表".to_string()),
        ),
        command_item(
            COMMAND_CAPABILITIES,
            "查看平台能力",
            Some("显示会话类型与各能力的真实支持状态".to_string()),
        ),
    ]
}

fn command_item(id: &str, title: &str, subtitle: Option<String>) -> SearchItem {
    debug_assert!(id.starts_with(COMMAND_PREFIX));
    SearchItem {
        id: id.to_string(),
        title: title.to_string(),
        subtitle,
        icon: None,
        source: HOST_SOURCE.to_string(),
        kind: ItemKind::Command,
        default_action: DefaultAction::Open,
        preview: Preview::None,
        score: Score::unordered(),
    }
}

fn application_item(entry: &AppEntry, score: Score) -> SearchItem {
    SearchItem {
        id: format!("app:{}", entry.id),
        title: entry.name.clone(),
        subtitle: entry.comment.clone(),
        icon: entry.icon.clone(),
        source: HOST_SOURCE.to_string(),
        kind: ItemKind::Application,
        default_action: DefaultAction::Open,
        preview: Preview::None,
        score,
    }
}
