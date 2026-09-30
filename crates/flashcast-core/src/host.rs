//! 无头宿主 `Host`。对应 ADR §2–§4：
//!
//! - `query` / `execute` 是唯一对外表达业务结果的入口；
//! - 选择状态由宿主持有，鼠标移动不调用 `set_selection`，因此不会抢走键盘选择；
//! - 记录进入查询范围时的 `input` / `scope` / `selection`，`back()` 恢复它们；
//! - 搜索结果的排序完全确定，与目录读取顺序无关。

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use flashcast_platform::capability::{Capabilities, CapabilityProbe};
use flashcast_platform::catalog::{AppCatalog, AppEntry};
use flashcast_platform::launch::AppLauncher;
use flashcast_platform::launch_request::LaunchRequest;

use crate::clone::{self, CloneControl, CloneOutcome, CloneProgress, CredentialProvider};
use crate::device::{CredentialStore, DeviceStore, StoredToken};
use crate::git::{CommitOutcome, GitError, WorkspaceChanges};
use crate::model::{
    ActionOutcome, BackOutcome, DefaultAction, ItemKind, Notice, PluginFailure, Preview,
    QueryResponse, QueryScope, Score, SearchItem, COMMAND_CAPABILITIES, COMMAND_PREFIX,
    COMMAND_RESCAN, HOST_SOURCE,
};
use crate::plugin::{PluginScope, SearchContext};
use crate::ranking::{score_match, sort_ranked, RankedItem};
use crate::registry::PluginRegistry;
use crate::settings::{Settings, SettingsError};
use crate::sync::{
    self, PullOutcome, PushOutcome, SyncControl, SyncError, SyncProgress, SyncStatus,
};
use crate::watch::WorkspaceWatcher;
use crate::workspace::{
    Workspace, WorkspaceError, WorkspaceReload, WorkspaceRemote, WorkspaceStatus,
};

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
    /// 设备本地的 Git 凭证（https 令牌）；与工作区严格分离。
    credentials: CredentialStore,
    /// 当前工作区的文件监听器。切换工作区时整体替换。
    watch: Mutex<Option<WorkspaceWatcher>>,
    /// 最近一次克隆操作的进度与取消信号（UI 轮询，另一线程执行克隆）。
    clone: Mutex<CloneControl>,
    /// 最近一次同步（拉取 / 推送）的进度与取消信号。
    sync: Mutex<SyncControl>,
    /// 是否有 Git 操作正在进行（网络操作期间为 `true`）。
    git_busy: AtomicBool,
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
        let credentials = CredentialStore::new(deps.device_dir.clone());
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
            credentials,
            watch: Mutex::new(None),
            clone: Mutex::new(CloneControl::new()),
            sync: Mutex::new(SyncControl::new()),
            git_busy: AtomicBool::new(false),
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
                status.remote = self
                    .device
                    .workspace_remote(workspace.root())
                    .ok()
                    .flatten()
                    .or_else(|| workspace.remote());
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

    /// 从远端 Git 仓库克隆配置工作区（ticket 14）。
    ///
    /// - 目标目录非空时拒绝，绝不覆盖已有文件；
    /// - 失败、取消或克隆出来的配置无效时回滚本次创建的内容，当前工作区与
    ///   有效设置保持不变；
    /// - 成功后校验并关联工作区（已有功能随即读取其中内容），记录远端关系。
    ///
    /// 阻塞调用：Tauri 外壳把它放到后台线程，UI 通过 [`Host::clone_progress`]
    /// 轮询进度、用 [`Host::cancel_clone`] 取消。
    pub fn clone_workspace(
        &self,
        remote_url: &str,
        target: &Path,
    ) -> Result<CloneOutcome, WorkspaceError> {
        let control = CloneControl::new();
        control.start();
        self.clone_workspace_with_control(remote_url, target, &control)
    }

    /// 用调用方提供的进度 / 取消信号执行克隆。
    ///
    /// 外壳把同一个信号交给后台线程（执行克隆）与 UI（轮询进度、请求取消），
    /// 调用方也可以在开始前先请求取消。`control` 会登记为「最近一次克隆」，
    /// 之后 [`Host::clone_progress`] 与 [`Host::cancel_clone`] 都作用于它。
    pub fn clone_workspace_with_control(
        &self,
        remote_url: &str,
        target: &Path,
        control: &CloneControl,
    ) -> Result<CloneOutcome, WorkspaceError> {
        *lock(&self.clone) = control.clone();
        self.clone_workspace_with(remote_url, target, control)
    }

    fn clone_workspace_with(
        &self,
        remote_url: &str,
        target: &Path,
        control: &CloneControl,
    ) -> Result<CloneOutcome, WorkspaceError> {
        let url = remote_url.trim();
        if url.is_empty() {
            return Err(self.clone_failure(control, "克隆地址不能为空".to_string()));
        }
        // 地址里带口令的写法一律拒绝：它会被 git 写进工作区的 .git/config。
        if clone::url_password(url).is_some() {
            let message = "克隆地址包含密码，已拒绝：请改用访问令牌（保存在本机设备目录）或系统 git 的凭证管理，不要把口令写在地址里"
                .to_string();
            return Err(self.clone_failure(control, message));
        }

        // 凭证来源：设备本地为该主机保存的 https 令牌（如有）。
        let provider = match clone::host_of(url) {
            Some(host) => {
                let token = self
                    .credentials
                    .token(&host)
                    .map_err(|error| WorkspaceError::Io(error.to_string()))?;
                CredentialProvider::with_token(token)
            }
            None => CredentialProvider::new(),
        };

        let cloned = clone::clone_repository(url, target, control, &provider)?;

        // 目标目录必须能作为配置工作区打开（目录可写、settings.toml 有效）。
        let workspace = match Workspace::open(target) {
            Ok(workspace) => workspace,
            Err(error) => {
                cloned.rollback();
                return Err(self.clone_failure(control, error.to_string()));
            }
        };
        let remote = match workspace.remote() {
            Some(remote) => remote,
            None => {
                cloned.rollback();
                return Err(self.clone_failure(
                    control,
                    "克隆完成但没有找到远端记录，无法建立同步关系".to_string(),
                ));
            }
        };

        // 关联：切换工作区、开始监听、恢复其中的设置（含插件启停选择）。
        if let Err(error) = self.activate_workspace(workspace) {
            cloned.rollback();
            return Err(self.clone_failure(control, error.to_string()));
        }

        let unavailable_plugins = self.unavailable_plugins();
        let recorded_theme = lock(&self.inner)
            .workspace
            .as_ref()
            .and_then(|workspace| workspace.recorded_theme());
        let status = self.workspace_status();
        // 记录远端 ↔ 工作区关系：ticket 16 的同步据此确定远端、默认分支与上游。
        if let Some(path) = status.path.clone() {
            if let Err(error) = self.device.set_workspace_remote(&path, Some(&remote)) {
                lock(&self.inner).workspace_error = Some(error.to_string());
            }
        }
        let status = self.workspace_status();
        Ok(CloneOutcome {
            workspace: status,
            remote,
            unavailable_plugins,
            recorded_theme,
        })
    }

    /// 最近一次克隆的进度快照（UI 轮询）。
    pub fn clone_progress(&self) -> CloneProgress {
        lock(&self.clone).progress()
    }

    /// 请求取消正在进行的克隆。回调在下一次触发时中断，随后自动回滚。
    pub fn cancel_clone(&self) {
        lock(&self.clone).cancel();
    }

    /// 重新读取当前工作区的远端关系（供 ticket 16 的同步与界面显示）。
    ///
    /// 地址一律去掉 userinfo 后再返回与保存。
    pub fn workspace_remote(&self) -> Option<WorkspaceRemote> {
        let workspace = lock(&self.inner).workspace.clone()?;
        let remote = workspace.remote()?;
        let _ = self
            .device
            .set_workspace_remote(workspace.root(), Some(&remote));
        Some(remote)
    }

    /// 记住某个远端主机要用的 https 令牌。**只**写进设备本地目录。
    pub fn remember_git_token(
        &self,
        remote_url: &str,
        username: &str,
        token: &str,
    ) -> Result<(), WorkspaceError> {
        let host = clone::host_of(remote_url)
            .ok_or_else(|| WorkspaceError::Clone("无法从克隆地址识别主机".to_string()))?;
        let username = if username.trim().is_empty() {
            "x-access-token".to_string()
        } else {
            username.trim().to_string()
        };
        self.credentials
            .set_token(&StoredToken {
                host,
                username,
                token: token.to_string(),
            })
            .map_err(|error| WorkspaceError::Io(error.to_string()))
    }

    /// 忘记某个远端主机的 https 令牌。
    pub fn forget_git_token(&self, remote_url: &str) -> Result<(), WorkspaceError> {
        let host = clone::host_of(remote_url)
            .ok_or_else(|| WorkspaceError::Clone("无法从克隆地址识别主机".to_string()))?;
        self.credentials
            .remove_token(&host)
            .map_err(|error| WorkspaceError::Io(error.to_string()))
    }

    /// 结束一次失败的克隆尝试：记录中文原因并给出失败阶段。
    fn clone_failure(&self, control: &CloneControl, message: String) -> WorkspaceError {
        let message = clone::redact(&message);
        control.finish(crate::clone::ClonePhase::Failed, Some(message.clone()));
        WorkspaceError::Clone(message)
    }

    /// 工作区记录为「停用」但本机没有对应实现的插件 id（如实报告，不假装已恢复）。
    fn unavailable_plugins(&self) -> Vec<String> {
        let known: std::collections::HashSet<String> = self
            .deps
            .plugins
            .manifests()
            .into_iter()
            .map(|(manifest, _enabled)| manifest.id)
            .collect();
        let mut missing: Vec<String> = self
            .settings()
            .disabled_plugins
            .into_iter()
            .filter(|id| !known.contains(id))
            .collect();
        missing.sort();
        missing.dedup();
        missing
    }

    /// Git 操作忙标志：置位期间丢弃工作区文件事件。
    ///
    /// ticket 15/16 的 status / commit / pull 用它包住整个 Git 操作，操作结束后按
    /// Git 状态显式重建界面（见 [`crate::watch`] 的模块文档）。
    pub fn set_git_busy(&self, busy: bool) {
        self.git_busy.store(busy, Ordering::SeqCst);
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.set_git_busy(busy);
        }
    }

    /// 是否有 Git 操作正在进行（同步状态里如实呈现）。
    pub fn git_busy(&self) -> bool {
        self.git_busy.load(Ordering::SeqCst)
    }

    /// 当前工作区的 Git 变更：状态分类、分支与逐文件真实差异（ADR §3 的补充入口）。
    ///
    /// 只读，不修改仓库，也不改变宿主状态。未关联工作区时返回
    /// [`WorkspaceChanges::unlinked`]；工作区不是 Git 仓库或读取失败时，
    /// 结果里的 `error` 给出中文原因。
    pub fn workspace_changes(&self) -> WorkspaceChanges {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return WorkspaceChanges::unlinked(),
        };
        crate::git::changes(&workspace)
    }

    /// 创建一次 Git 提交，范围**只包含** `paths` 里显式给出的路径（ADR §3 的补充入口）。
    ///
    /// 提交前把工作区标记为「Git 操作进行中」，期间丢弃文件监听事件；提交后由调用方
    /// 用返回值里的 `changes`（按真实仓库状态重新读取）刷新界面。提交说明为空、
    /// 未选择路径、身份未配置、工作区异常或索引被占用时返回中文原因，
    /// 且不产生提交、不改动工作区文件。
    pub fn commit_workspace(
        &self,
        message: &str,
        paths: &[String],
    ) -> Result<CommitOutcome, GitError> {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Err(GitError::NoWorkspace),
        };
        self.set_git_busy(true);
        let result = crate::git::commit(&workspace, message, paths);
        self.set_git_busy(false);
        result
    }

    /// 当前工作区的同步状态（ADR §3 的补充入口）。
    ///
    /// **只读**：不发网络请求、不写 `.git/index`、也不改变宿主状态。设置页据此展示
    /// 分支、远端、领先 / 落后、未提交改动、能否同步、是否有操作进行中，以及
    /// 拉取 / 推送的阻塞原因与中文指引。
    pub fn sync_status(&self) -> SyncStatus {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return SyncStatus::unlinked(),
        };
        // 设备本地记录的远端关系（ticket 14）作为仓库内无远端时的兜底展示。
        let recorded = self
            .device
            .workspace_remote(workspace.root())
            .ok()
            .flatten();
        let mut status = sync::status(&workspace, recorded.as_ref());
        status.busy = self.git_busy();
        status
    }

    /// 重新检测同步状态（UI 的「重新检测」入口）。
    ///
    /// 用户在应用外部解决阻塞（提交、合并、中止变基、`git remote add`…）之后调用它：
    /// 重新读取并脱敏远端关系，然后按真实 Git 状态重建状态与指引。
    pub fn redetect_sync_state(&self) -> SyncStatus {
        let _ = self.workspace_remote();
        self.sync_status()
    }

    /// 仅快进拉取当前工作区（ADR §3 的补充入口）。
    ///
    /// - 拉取前先判脏：未提交修改、冲突、进行中的 Git 操作、分离 HEAD 都在
    ///   网络操作之前返回，工作区与索引逐字节不变；
    /// - 只接受快进；分叉一律拒绝，绝不自动合并、不强推；
    /// - 成功后重新加载生效设置，并重新读取主题与备忘录（见 [`PullOutcome`]）。
    ///
    /// 阻塞调用：Tauri 外壳把它放到后台线程，UI 用 [`Host::sync_progress`] 轮询进度、
    /// 用 [`Host::cancel_sync`] 取消。
    pub fn pull_workspace(&self) -> Result<PullOutcome, SyncError> {
        self.pull_workspace_with_control(&SyncControl::new())
    }

    /// 用调用方提供的进度 / 取消信号执行拉取。`control` 会登记为「最近一次同步」。
    pub fn pull_workspace_with_control(
        &self,
        control: &SyncControl,
    ) -> Result<PullOutcome, SyncError> {
        *lock(&self.sync) = control.clone();
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Err(SyncError::NoWorkspace),
        };
        let provider = self.sync_credentials(&workspace)?;

        self.set_git_busy(true);
        let report = sync::pull(&workspace, &provider, control);
        self.set_git_busy(false);

        let report = report?;
        // 拉取后按 Git 状态显式重建视图：重新加载生效设置、主题与备忘录。
        let reload = self.reload_from_workspace(&workspace.settings_path());
        Ok(PullOutcome {
            result: report.result,
            message: report.message,
            reload,
            theme: workspace.recorded_theme(),
            memos: workspace.memo_files(),
            status: self.sync_status(),
        })
    }

    /// 显式推送当前分支到它的上游（ADR §3 的补充入口）。
    ///
    /// refspec 不带 `+`，因此永不 force：远端拒绝非快进时如实报告并让用户先拉取。
    /// 「没有需要推送的提交」「未配置上游」「鉴权失败」是三个独立分类。
    /// 未提交修改不阻塞推送（推送只搬运已提交对象），但会在状态里如实呈现。
    pub fn push_workspace(&self) -> Result<PushOutcome, SyncError> {
        self.push_workspace_with_control(&SyncControl::new())
    }

    /// 用调用方提供的进度 / 取消信号执行推送。
    pub fn push_workspace_with_control(
        &self,
        control: &SyncControl,
    ) -> Result<PushOutcome, SyncError> {
        *lock(&self.sync) = control.clone();
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Err(SyncError::NoWorkspace),
        };
        let provider = self.sync_credentials(&workspace)?;

        self.set_git_busy(true);
        let report = sync::push(&workspace, &provider, control);
        self.set_git_busy(false);

        let report = report?;
        Ok(PushOutcome {
            branch: report.branch,
            remote: report.remote,
            updated: report.updated,
            message: report.message,
            status: self.sync_status(),
        })
    }

    /// 最近一次同步的进度快照（UI 轮询）。
    pub fn sync_progress(&self) -> SyncProgress {
        lock(&self.sync).progress()
    }

    /// 请求取消正在进行的同步。fetch 在下次进度回调中断，push 在协商阶段中止。
    pub fn cancel_sync(&self) {
        lock(&self.sync).cancel();
    }

    /// 同步（克隆之外的 fetch / push）用的凭证来源：设备本地为该主机保存的令牌，
    /// 其余交给 ssh-agent / `~/.ssh` / 系统 git 的凭证 helper。
    fn sync_credentials(&self, workspace: &Workspace) -> Result<CredentialProvider, SyncError> {
        let url = self
            .workspace_remote()
            .map(|remote| remote.url)
            .or_else(|| workspace.remote().map(|remote| remote.url));
        Ok(match url.as_deref().and_then(clone::host_of) {
            Some(host) => {
                let token = self
                    .credentials
                    .token(&host)
                    .map_err(|error| SyncError::Git(error.to_string()))?;
                CredentialProvider::with_token(token)
            }
            None => CredentialProvider::new(),
        })
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
                // 外部修改的插件启停也要真正生效（已有功能读取其中内容）。
                self.apply_plugin_choices(&settings.disabled_plugins);
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
        // 工作区记录的插件启停立刻生效：克隆 / 切换后已有功能随即按新选择工作。
        self.apply_plugin_choices(&self.settings().disabled_plugins);
        *lock(&self.watch) = Some(watcher);
        // 记住本机路径，重启后恢复。这是设备本地数据，不写进工作区。
        if let Err(error) = self.device.set_workspace_path(Some(workspace.root())) {
            lock(&self.inner).workspace_error = Some(error.to_string());
        }
        Ok(self.workspace_status())
    }

    /// 把工作区记录的插件启停选择应用到插件注册表。
    ///
    /// `disabledPlugins` 是工作区里的可迁移偏好（ticket 05 的 `settings.toml`），
    /// 这里是「已有功能读取其中内容」的落点；本机没有对应实现的 id 由
    /// [`Host::unavailable_plugins`] 如实报告，不会被静默当成已恢复。
    fn apply_plugin_choices(&self, disabled: &[String]) {
        for (manifest, enabled) in self.deps.plugins.manifests() {
            let should_enable = !disabled.iter().any(|id| id == &manifest.id);
            if enabled != should_enable {
                self.deps.plugins.set_enabled(&manifest.id, should_enable);
            }
        }
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
