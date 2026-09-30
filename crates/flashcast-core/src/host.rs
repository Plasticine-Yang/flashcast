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
use crate::model::{
    ActionOutcome, BackOutcome, DefaultAction, ItemKind, Notice, PluginFailure, Preview,
    QueryResponse, QueryScope, Score, SearchItem, COMMAND_CAPABILITIES, COMMAND_PREFIX,
    COMMAND_RESCAN, HOST_SOURCE,
};
use crate::plugin::{PluginScope, SearchContext};
use crate::ranking::{score_match, sort_ranked, RankedItem};
use crate::registry::PluginRegistry;
use crate::settings::{Settings, SettingsError};
use crate::watch::WorkspaceWatcher;
use crate::workspace::{Workspace, WorkspaceError, WorkspaceReload, WorkspaceStatus};

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
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
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
        // 先读**原始字节**、再解析：宿主这次读在 macOS / Windows 上会被上报成像修改的
        // 事件（macOS 常是 `EventKind::Any`，按事件类型丢不掉）。因此无论解析结果如何，
        // 读到的内容都先记进监听层的账本，随后内容字节相同的事件才会被吞掉，而不是形成
        // 「重载 → 读 → 事件 → 重载」（见 [`crate::watch`] 模块文档）。
        let bytes = match workspace.read_settings_bytes() {
            Ok(bytes) => bytes,
            Err(error) => return self.reload_failed(changed, error.to_string()),
        };
        let settings = match &bytes {
            Some(bytes) => {
                self.record_own_read(&workspace.settings_path(), bytes);
                match workspace.parse_settings(bytes) {
                    Ok(settings) => Some(settings),
                    Err(error) => return self.reload_failed(changed, error.to_string()),
                }
            }
            None => None,
        };
        match settings {
            // 幂等：内容与生效设置一致时不做任何事，写入循环在此终止。
            Some(settings) if settings == self.settings() => WorkspaceReload {
                path: changed.to_path_buf(),
                applied: false,
                settings,
                error: None,
            },
            Some(settings) => {
                let mut inner = lock(&self.inner);
                inner.settings = settings.clone();
                inner.workspace_error = None;
                inner.reloads += 1;
                let applied = inner.reloads;
                drop(inner);
                let _ = applied;
                WorkspaceReload {
                    path: changed.to_path_buf(),
                    applied: true,
                    settings,
                    error: None,
                }
            }
            None => WorkspaceReload {
                path: changed.to_path_buf(),
                applied: false,
                settings: self.settings(),
                error: Some(format!(
                    "设置文件已不存在，继续使用上一次有效设置（{}）",
                    workspace.settings_path().display()
                )),
            },
        }
    }

    /// 记录宿主自己读到的文件内容：这次读可能被文件监听当成修改（见
    /// [`Host::reload_from_workspace`] 与 [`crate::watch`] 的模块文档）。
    fn record_own_read(&self, path: &Path, bytes: &[u8]) {
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.record_own_read(path, bytes);
        }
    }

    /// 重载失败：保留上一次有效设置，把中文原因记进工作区状态并返回。
    fn reload_failed(&self, changed: &Path, message: String) -> WorkspaceReload {
        lock(&self.inner).workspace_error = Some(message.clone());
        WorkspaceReload {
            path: changed.to_path_buf(),
            applied: false,
            settings: self.settings(),
            error: Some(message),
        }
    }

    fn unchanged_reload(&self, path: PathBuf) -> WorkspaceReload {
        WorkspaceReload {
            path,
            applied: false,
            settings: self.settings(),
            error: None,
        }
    }

    /// 让一个已校验的工作区成为当前工作区，并开始监听它的变更。
    fn activate_workspace(&self, workspace: Workspace) -> Result<WorkspaceStatus, WorkspaceError> {
        // 先读设置：无效则整体拒绝，当前工作区与设置原样保留。
        let loaded = workspace.read_settings()?;
        let watcher =
            WorkspaceWatcher::start(workspace.root(), workspace.git_dir().map(Path::to_path_buf))
                .map_err(|error| WorkspaceError::Watch(error.to_string()))?;
        {
            let mut inner = lock(&self.inner);
            if let Some(settings) = loaded {
                inner.settings = settings;
            }
            inner.workspace = Some(workspace.clone());
            inner.workspace_error = None;
        }
        *lock(&self.watch) = Some(watcher);
        // 记住本机路径，重启后恢复。这是设备本地数据，不写进工作区。
        if let Err(error) = self.device.set_workspace_path(Some(workspace.root())) {
            lock(&self.inner).workspace_error = Some(error.to_string());
        }
        Ok(self.workspace_status())
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

    /// 插件清单与启用状态。
    pub fn plugin_manifests(&self) -> Vec<(crate::plugin::PluginManifest, bool)> {
        self.deps.plugins.manifests()
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
