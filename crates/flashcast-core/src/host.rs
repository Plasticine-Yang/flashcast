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

use flashcast_platform::capability::{Capabilities, CapabilityProbe, Support};
use flashcast_platform::catalog::{AppCatalog, AppEntry};
use flashcast_platform::chrome::{
    build_open_args, validate_open_url, ChromeLaunchRequest, ChromeProvider,
};
use flashcast_platform::clipboard::{
    ClipboardAccess, ClipboardContent, ClipboardWatcher, ClipboardWriteReport,
};
use flashcast_platform::focus::{same_app, FocusTracker, FocusedApp};
use flashcast_platform::launch::AppLauncher;
use flashcast_platform::launch_request::LaunchRequest;
use flashcast_platform::paste::{manual_paste_message, Paster};

use crate::chrome::{
    BookmarkEntry, BookmarkIndex, ChromeAssociation, ChromeBookmarkError, ChromeProfileView,
    ChromeState, KEY_CHROME_ASSOCIATION,
};
use crate::clipboard::{
    ClipboardActionError, ClipboardCaptureOutcome, ClipboardState, ClipboardStore, CLIPBOARD_DIR,
};
use crate::clone::{self, CloneControl, CloneOutcome, CloneProgress, CredentialProvider};
use crate::device::{CredentialStore, DeviceStore, StoredToken};
use crate::git::{CommitOutcome, GitError, WorkspaceChanges};
use crate::manifest::{ManifestEntry, ManifestError, PluginManifestFile};
use crate::memo::{self, Memo, MemoBook, MemoError, MemoProblem};
use crate::model::{
    ActionOutcome, BackOutcome, DefaultAction, ItemKind, MatchTier, Notice, PastePlan,
    PluginFailure, Preview, QueryResponse, QueryScope, Score, SearchItem, COMMAND_CAPABILITIES,
    COMMAND_PREFIX, COMMAND_RESCAN, HOST_SOURCE,
};
use crate::plugin::{
    PluginKind, PluginScope, SearchContext, CAP_CLIPBOARD_READ, CAP_CLIPBOARD_WRITE,
};
use crate::ranking::{score_match, sort_ranked, RankedItem};
use crate::registry::PluginRegistry;
use crate::settings::{Settings, SettingsError};
use crate::sync::{
    self, PullOutcome, PushOutcome, SyncControl, SyncError, SyncProgress, SyncStatus,
};
use crate::theme::{
    Appearance, ThemeDocument, ThemeEntry, ThemeError, ThemeLibrary, ThemeSelection, ThemeState,
    ThemeTokens, THEME_LIGHT,
};
use crate::watch::{hash_bytes, WatchEventTrace, WorkspaceWatcher};
use crate::workspace::{
    write_atomic, Workspace, WorkspaceError, WorkspaceReload, WorkspaceRemote, WorkspaceStatus,
};

/// 宿主的注入依赖。不含任何 Tauri 类型。
#[derive(Clone)]
pub struct HostDeps {
    pub catalog: Arc<dyn AppCatalog>,
    pub launcher: Arc<dyn AppLauncher>,
    pub capabilities: Arc<dyn CapabilityProbe>,
    /// 剪贴板（ADR §5）。只有宿主在命令入口里经权限校验后调用它；
    /// 功能插件拿不到这个句柄。
    pub clipboard: Arc<dyn ClipboardAccess>,
    /// 剪贴板变化监听（ADR §5，ticket 09）。宿主在插件启用且声明了
    /// `clipboard.read` 时才会轮询它；功能插件拿不到这个句柄。
    pub clipboard_watcher: Arc<dyn ClipboardWatcher>,
    /// Chrome 发现与启动（ADR §5 的 `ChromeProvider`）。只有宿主在命令入口里经权限
    /// 校验后调用它；功能插件拿不到这个句柄。
    pub chrome: Arc<dyn ChromeProvider>,
    /// 焦点读取与恢复（ADR §5）。自动粘贴前用它把焦点还给唤起前的应用，
    /// 并在注入之前核对「恢复后的前台确实是那个应用」。
    pub focus: Arc<dyn FocusTracker>,
    /// 合成粘贴（ADR §5）。只在核对通过之后调用。
    pub paster: Arc<dyn Paster>,
    pub plugins: Arc<PluginRegistry>,
    /// 设备本地数据根目录（应用数据目录）。工作区之外的本机数据都放这里：
    /// 当前工作区的路径、缓存、设备路径、权限状态、日志与凭证。
    pub device_dir: PathBuf,
}

/// 自动粘贴的会话状态。
///
/// 分成两半是刻意的：`target` 是外壳在**唤起时**（窗口显示之前）捕获的目标应用，
/// `plan` 是最近一次 `execute` 产出的待完成粘贴。两者都不是长期状态：每次唤起都会
/// 覆盖 `target` 并作废 `plan`，因此「上一次唤起的应用」不可能被这一轮粘贴用到。
#[derive(Default)]
struct PasteState {
    /// 唤起前处于前台的应用程序。
    target: Option<FocusedApp>,
    /// 待外壳关闭浮窗后完成的粘贴计划（永远是最新一次 `execute` 的产物）。
    plan: Option<PastePlan>,
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
    /// 已进入的插件范围对象。用 `Arc` 持有，范围搜索才能放进带超时的隔离线程
    /// （ADR §6：插件任务具有隔离、取消与超时）。
    plugin_scopes: HashMap<String, Arc<dyn PluginScope>>,
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
    /// 已关联的 Chrome profile。来自设备本地存储，**不属于**配置工作区。
    chrome_association: Option<ChromeAssociation>,
    /// Chrome 关联 / 发现的问题（未安装、profile 消失、记录损坏……）。
    chrome_error: Option<String>,
    /// 当前配置工作区；`None` 表示尚未关联。
    workspace: Option<Workspace>,
    /// 当前**已应用**设置内容的哈希。
    ///
    /// 这是「一次外部修改只重载一次」的**主判据**：重载时先读磁盘原始字节，哈希与它
    /// 相同就当无事发生（不给用户发事件、不计重载、不重新生效）。它不依赖事件路径、
    /// FSEvents 的延迟、重复/合并的事件记录、读文件引发的自伤事件，也不依赖静默窗口，
    /// 因此把 [`crate::watch`] 的 1–4 层过滤全删掉仍然成立（见
    /// [`Host::reload_from_workspace`]）。
    ///
    /// 每一条能改变设置的路径都必须刷新它：应用自身写入、外部重载生效、切换工作区、
    /// 启动时恢复工作区。否则过期的哈希会把真实修改静默吞掉。
    applied_settings_hash: Option<u64>,
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
    /// 备忘录的**生效内容**。宿主在成功写入工作区之后更新它，功能插件只读快照：
    /// 插件因此不持有工作区路径，也无法绕过宿主直接改文件（ADR §6）。
    memos: Arc<MemoBook>,
    /// Chrome 书签索引：可随时从 `Bookmarks` 文件重建（文件是唯一事实来源）。
    bookmarks: Arc<BookmarkIndex>,
    /// 剪贴板历史的本机存储与后台捕获运行时（ticket 09）。
    ///
    /// 单独持有、不放进 `inner`：轮询剪贴板与写 SQLite 都是慢操作，
    /// 绝不能在持有宿主的全局锁时做。
    clipboard: Arc<crate::clipboard::ClipboardRuntime>,
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
    /// 自动粘贴的会话状态：唤起时捕获的目标应用与待完成的粘贴计划。
    ///
    /// 单独一把锁（不放进 `inner`）是为了让锁的持有时间尽可能短：粘贴流程会调用
    /// 平台适配层，绝不能在持有宿主的全局锁时做这件事。
    paste: Mutex<PasteState>,
    /// 粘贴计划序号，单调递增。作用与查询的 `seq` 相同：让外壳/测试能识别并丢弃
    /// 过期计划，快速连续执行时只有最后一次会被真正粘贴。
    paste_epoch: AtomicU64,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 自动粘贴的能力前提。返回 `Some(原因)` 表示不能自动粘贴，必须降级为手动粘贴。
///
/// 三种状态区别对待，但结论一致：
///
/// - `Supported`：可以做（还需要一个捕获到的目标应用，由调用方另行判断）；
/// - `Unsupported`：平台明确做不到（Wayland、缺辅助功能权限、没有交互桌面），
///   把平台给出的中文原因原样透传，它已经写清了「为什么」与「怎么办」；
/// - `Unknown`：当前环境无法判定——**不能**当作可以做，如实说明「无法确认」。
fn auto_paste_blocker(capabilities: &Capabilities) -> Option<String> {
    match &capabilities.auto_paste {
        Support::Supported => None,
        Support::Unsupported { reason } => Some(reason.clone()),
        Support::Unknown { reason } => Some(format!("无法确认当前环境能否自动粘贴（{reason}）")),
    }
}

/// [`Host::wait_for_workspace_change`] 的轮询间隔。
const WATCH_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// 把一条剪贴板历史还原成要写回系统剪贴板的内容。
///
/// 恢复的是**同一次复制的全部公开格式**：可索引的纯文本加上历史里保存的 HTML/RTF
/// 载荷（`clipboard_payloads`）。目标应用自己挑要哪一种——富文本目标取格式化版本，
/// 纯文本目标取文本。载荷只取本版本认识的 `html` / `rtf` 角色；未知角色的载荷不会被
/// 当作 HTML 塞进剪贴板（宁可不提供，也不猜）。
pub fn clipboard_content_for(
    event: &crate::clipboard::ClipboardEvent,
    text: String,
) -> ClipboardContent {
    let inline = |role: &str| {
        event
            .payloads
            .iter()
            .find(|payload| payload.role.tag() == role)
            .and_then(|payload| payload.inline.clone())
            .filter(|value| !value.is_empty())
    };
    ClipboardContent::text(text)
        .with_html(inline("html"))
        .with_rtf(inline("rtf"))
}

/// 剪贴板后台捕获的轮询间隔。
///
/// 剪贴板库没有变化事件（research `app-discovery-and-focus.md`），只能轮询。500ms 是
/// 「用户复制后切回 Flashcast 就会看到」与「不空耗 CPU」之间的折中；每次轮询在 Linux
/// 上会起一个 `wl-paste` / `xclip` 进程，不能再密。
const CLIPBOARD_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// 查询方式：用户输入会改变输入状态，快照类操作不会。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SearchMode {
    /// 用户在输入框里输入（含 `back()` 恢复历史后的重算）。
    ///
    /// 关键词与标签冲突时保留双方：首屏同时给出插件入口与标签命中的备忘录（ADR §4）。
    UserInput,
    /// 用户明确选择了首屏的「插件入口」条目。
    ///
    /// 此时必须真的进入该插件范围，跳过冲突保留，否则入口会变成点不动的死路。
    ExplicitPluginEntry,
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
                // 默认清单 = 浅色 / 深色 / 跟随系统三个默认主题。
                manifest: PluginManifestFile::defaults(),
                theme_library: ThemeLibrary::with_builtins(),
                selected_theme: THEME_LIGHT.to_string(),
                system_appearance: Appearance::Light,
                theme_tokens: crate::theme::light_tokens(),
                theme_appearance: Appearance::Light,
                theme_error: None,
                theme_notice: None,
                chrome_association: None,
                chrome_error: None,
                workspace: None,
                applied_settings_hash: None,
                workspace_error: None,
                reloads: 0,
            }),
            seq: AtomicU64::new(0),
            // 先克隆监听句柄（`deps` 随后被移动进结构体）。
            clipboard: Arc::new(crate::clipboard::ClipboardRuntime::new(
                Arc::new(ClipboardStore::open(deps.device_dir.join(CLIPBOARD_DIR))),
                Arc::clone(&deps.clipboard_watcher),
                CLIPBOARD_POLL_INTERVAL,
            )),
            deps,
            memos: Arc::new(MemoBook::new()),
            bookmarks: Arc::new(BookmarkIndex::new()),
            device,
            credentials,
            watch: Mutex::new(None),
            clone: Mutex::new(CloneControl::new()),
            sync: Mutex::new(SyncControl::new()),
            git_busy: AtomicBool::new(false),
            paste: Mutex::new(PasteState::default()),
            paste_epoch: AtomicU64::new(0),
        };
        host.rescan_catalog();
        host.restore_workspace();
        // Chrome 关联是设备本地数据：启动时恢复，重启后仍然能直接检索与打开。
        host.restore_chrome_association();
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
        let applied_hash = self.persist_settings(&settings)?;
        {
            let mut inner = lock(&self.inner);
            inner.settings = settings.clone();
            // 应用写入也是一次「已应用内容」的变化：设置与哈希在同一把锁内更新。
            if let Some(hash) = applied_hash {
                inner.applied_settings_hash = Some(hash);
            }
        }
        // 剪贴板的暂停 / 保留期限 / 容量改动立刻作用到后台捕获。
        self.sync_clipboard_runtime();
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
        // 关联已有工作区时**不写任何文件**：工作区里的 Git 仓库必须保持干净，
        // 直到用户真的改了插件清单或主题（见 `activate_workspace`）。
        self.activate_workspace(workspace, false)
    }

    /// 在新目录（必须为空或不存在）上初始化工作区**及其 Git 仓库**。
    ///
    /// 非空目录一律拒绝，绝不覆盖已有用户文件。
    pub fn init_workspace(&self, path: &Path) -> Result<WorkspaceStatus, WorkspaceError> {
        let workspace = Workspace::init(path)?;
        // 刚初始化的空目录：随设置文件一起写出默认插件清单与主题配置，
        // 让新工作区一开始就自描述。这里不可能覆盖用户已有文件。
        self.activate_workspace(workspace, true)
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
        // `bootstrap = false`：克隆下来的是一份已有仓库，只读不写，不能把它改脏；
        // 清单 / 主题配置缺失时用内存默认值（与「关联已有仓库」一致）。
        if let Err(error) = self.activate_workspace(workspace, false) {
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
    ///
    /// 两个来源都算「工作区的记录」：清单里 `enabled = false` 的功能插件条目，
    /// 以及一次性迁移用的历史字段 `settings.toml` 的 `disabledPlugins`（ticket 07 起
    /// 只在清单还没有该插件条目时生效）。本机有实现的 id 不算「缺失」。
    fn unavailable_plugins(&self) -> Vec<String> {
        let known: std::collections::HashSet<String> = self
            .deps
            .plugins
            .manifests()
            .into_iter()
            .map(|(manifest, _enabled)| manifest.id)
            .collect();
        // 历史字段先在锁外取出来：`lock(&self.inner)` 的临时守卫活到整条语句结束，
        // 在同一表达式里再调用 `self.settings()`（它也要锁 `inner`）会自锁死。
        let legacy = self.settings().disabled_plugins;
        let mut missing: Vec<String> = lock(&self.inner)
            .manifest
            .entries()
            .iter()
            .filter(|entry| entry.kind == PluginKind::Feature && !entry.enabled)
            .map(|entry| entry.id.clone())
            .chain(legacy)
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
        // 拉取后按 Git 状态显式重建视图：重新加载生效设置与主题（`reload_from_workspace`
        // 会重新解析 settings.toml 与 theme.json，并应用插件清单里的主题包），
        // 再重新读取备忘录列表。
        //
        // 判据是「磁盘内容与已应用内容是否一致」，已经是最新时返回 None；这里要的是
        // 一份可展示的结果，因此用当前状态补一份 applied=false 的回报（与
        // [`Host::reload_workspace`] 同一处理）。
        let settings_path = workspace.settings_path();
        let reload = self
            .reload_from_workspace(&settings_path)
            .unwrap_or_else(|| self.unchanged_reload(settings_path.clone()));
        let theme = reload.theme.selected.clone();
        Ok(PullOutcome {
            result: report.result,
            message: report.message,
            reload,
            theme: Some(theme),
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

    /// 最近一次被监听层接受的工作区事件（路径、事件类型与宿主处理结果）。诊断用。
    ///
    /// 跨平台监听缺陷的断言失败时，「多了一次重载」本身没有信息量；这个入口让测试能
    /// 在消息里带上究竟是哪条事件、走的哪个分支。
    pub fn last_watch_event(&self) -> Option<WatchEventTrace> {
        lock(&self.watch)
            .as_ref()
            .and_then(|watcher| watcher.last_accepted_event())
    }

    /// 最近一次事件过滤决策，包括被拒绝的事件。诊断用。
    pub fn last_watch_decision(&self) -> Option<WatchEventTrace> {
        lock(&self.watch)
            .as_ref()
            .and_then(|watcher| watcher.last_decision())
    }

    /// 显式重新读取工作区配置（UI 的「重新检测」入口）。
    ///
    /// 与监听路径共用同一判据：磁盘内容与已应用内容一致时不生效、不计重载，只回一份
    /// `applied == false` 的结果。UI 需要的是一份可展示的结果，因此这里总是返回
    /// [`WorkspaceReload`]，而不是像 [`Host::wait_for_workspace_change`] 那样返回 `None`。
    pub fn reload_workspace(&self) -> WorkspaceReload {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return self.unchanged_reload(PathBuf::new()),
        };
        let path = workspace.settings_path();
        match self.reload_from_workspace(&path) {
            Some(reload) => reload,
            // `reload_from_workspace` 在设置文件内容没变时会提前返回（那是重复事件的热路径
            // 判据），但 `memos/*.md` 等其它工作区内容仍可能被外部改过。显式重载入口因此
            // 补一次备忘录读取，让「重新检测」与监听路径看到同样的内容。
            None => {
                let memos_changed = self.load_memos(&workspace);
                if memos_changed {
                    lock(&self.inner).reloads += 1;
                }
                WorkspaceReload {
                    path,
                    applied: memos_changed,
                    settings: self.settings(),
                    theme: self.theme_state(),
                    error: None,
                }
            }
        }
    }

    /// 等待并处理一次外部修改。
    ///
    /// 阻塞至多 `timeout`：超时、没有工作区，或收到的事件所携带的内容与已应用内容一致
    /// 时返回 `None`。外壳在后台线程里循环调用它并把结果推送给 UI。
    ///
    /// 实现为短轮询而不是在监听通道上阻塞等待：等待期间不持有工作区监听锁，
    /// 保存设置或切换工作区不会被这里卡住。内容一致的事件被就地丢弃后继续等待，
    /// 直到超时或出现一次真正的修改。
    pub fn wait_for_workspace_change(&self, timeout: Duration) -> Option<WorkspaceReload> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let changed = {
                let watch = lock(&self.watch);
                watch.as_ref().and_then(|watcher| watcher.try_next_change())
            };
            if let Some(changed) = changed {
                if let Some(reload) = self.reload_from_workspace(&changed) {
                    return Some(reload);
                }
                // 内容未变（重复记录、自读事件、FSEvents 迟到的上报）：继续等，
                // 但不再对外发出任何事件。
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(WATCH_POLL_INTERVAL);
        }
    }

    /// 处理一次文件事件：**先读原始字节**，与已应用内容比对，只有真正变化才算一次重载。
    ///
    /// 返回 `None` 表示磁盘内容与已应用内容完全一致，因此不计数、不应用、也不对外发出
    /// 任何事件。这是跨平台正确性的**主判据**，刻意不依赖事件路径（macOS 上报的路径
    /// 未必等于写入路径）、不依赖事件类型（FSEvents 常只给 `Any`）、不依赖到达时间
    /// （可能晚于静默窗口），也不依赖 [`crate::watch`] 的账本与静默窗口——那四层只是
    /// 廉价的第一道过滤，删掉它们这些用例仍然必须通过。
    ///
    /// 字节变化但解析后与生效设置相同时，同样不对外发事件（文本变了、语义没变），
    /// 但会刷新已应用哈希，避免后续重复事件被反复重新解析。
    fn reload_from_workspace(&self, changed: &Path) -> Option<WorkspaceReload> {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return None,
        };
        let mut messages: Vec<String> = Vec::new();

        // 先读**原始字节**、再解析：宿主这次读在 macOS / Windows 上会被上报成像修改的
        // 事件（macOS 常是 `EventKind::Any`，按事件类型丢不掉）。因此无论解析结果如何，
        // 读到的内容都先记进监听层的账本，作为第一道过滤。
        let bytes = match workspace.read_settings_bytes() {
            Ok(bytes) => bytes,
            Err(error) => {
                self.record_outcome(changed, "read-failed");
                return Some(self.reload_failed(changed, error.to_string()));
            }
        };
        let incoming_hash = bytes.as_deref().map(hash_bytes);
        // 主判据只对**设置文件本身**生效：磁盘内容与已应用内容一致时什么都不做。
        // 其它工作区文件（manifest.json / theme.json / themes/<id>/theme.json）变化时
        // settings.toml 通常原封不动，这里若一并短路，就会把主题与清单的外部修改吞掉；
        // 因此先按文件名判断这次改的是不是设置文件。
        let settings_file_changed = changed.file_name() == workspace.settings_path().file_name();
        if settings_file_changed
            && incoming_hash.is_some()
            && incoming_hash == lock(&self.inner).applied_settings_hash
        {
            self.record_outcome(changed, "content-unchanged-noop");
            return None;
        }
        let settings = match &bytes {
            Some(bytes) => {
                self.record_own_read(&workspace.settings_path(), bytes);
                match workspace.parse_settings(bytes) {
                    Ok(settings) => Some(settings),
                    Err(error) => {
                        self.record_outcome(changed, "invalid-config");
                        // 即便解析失败也要刷新已应用哈希：同一份坏内容只报一次，
                        // 后续迟到 / 重复的事件才能被主判据吞掉。
                        if let Some(hash) = incoming_hash {
                            lock(&self.inner).applied_settings_hash = Some(hash);
                        }
                        return Some(self.reload_failed(changed, error.to_string()));
                    }
                }
            }
            // 设置文件被删除：保留上一次有效设置，但主题与清单仍按工作区重读。
            None => {
                self.record_outcome(changed, "settings-file-removed");
                messages.push(format!(
                    "设置文件已不存在，继续使用上一次有效设置（{}）",
                    workspace.settings_path().display()
                ));
                None
            }
        };

        // 注意：这里**不能**因为「设置解析后与当前一致」就提前返回。改了别的文件
        // （主题 / 清单）时 settings.toml 往往没变，提前返回会把那次外部修改吞掉；
        // 真正的「什么都没变」由下面 `applied` 与消息共同判定。

        // 备忘录内容（`memos/*.md`）：整个目录重新读取后与**已生效内容**比对。
        // 解析后相同就不算一次重载（文本变了、语义没变），与设置文件同一判据。
        let memos_changed = self.load_memos(&workspace);

        let applied = {
            let mut inner = lock(&self.inner);
            let mut applied = memos_changed;
            if let Some(settings) = settings {
                if settings != inner.settings {
                    inner.settings = settings;
                    applied = true;
                }
            }
            // 外部重载生效也必须刷新已应用哈希，否则同一内容会被反复重载。
            if let Some(hash) = incoming_hash {
                inner.applied_settings_hash = Some(hash);
            }
            let (config_changed, theme_reason) =
                self.apply_workspace_config(&mut inner, &workspace, false);
            applied |= config_changed;
            inner.theme_notice = theme_reason;
            if applied {
                inner.workspace_error = None;
                inner.reloads += 1;
            }
            applied
        };
        // 外部修改的插件启停也要真正生效。清单是唯一权威，因此这里无条件按清单
        // 重放一次启停（幂等；避免上一次请求留下的注册表状态覆盖刚读到的清单）。
        self.apply_manifest_plugin_state();
        // 外部改了剪贴板设置（暂停 / 保留期限 / 容量）或插件启停时同样立刻生效：
        // 后台捕获线程与记录范围都跟着更新。
        self.sync_clipboard_runtime();
        if applied {
            self.record_outcome(changed, "applied");
        } else if messages.is_empty() {
            // 事件对应的工作区内容与已应用状态完全一致（例如主题文件被触碰但外观没变，
            // 或 macOS 迟到/重复的事件）：不计重载、不发事件，避免自伤事件流。
            self.record_outcome(changed, "content-unchanged-noop");
            return None;
        }

        let theme = self.theme_state();
        if let Some(error) = theme.error.clone() {
            messages.push(error);
        }
        Some(WorkspaceReload {
            path: changed.to_path_buf(),
            applied,
            settings: self.settings(),
            theme,
            error: if messages.is_empty() {
                None
            } else {
                Some(messages.join("；"))
            },
        })
    }

    /// 记录宿主对这条已接受事件的处理结果（诊断用）。
    fn record_outcome(&self, path: &Path, outcome: &str) {
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.record_outcome(path, outcome);
        }
    }

    /// 记录宿主自己读到的文件内容：这次读可能被文件监听当成修改（见
    /// [`Host::reload_from_workspace`] 与 [`crate::watch`] 的模块文档）。
    fn record_own_read(&self, path: &Path, bytes: &[u8]) {
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.record_own_read(path, bytes);
        }
    }

    /// 重载失败：保留上一次有效配置，把中文原因记进工作区状态并返回。
    fn reload_failed(&self, changed: &Path, message: String) -> WorkspaceReload {
        lock(&self.inner).workspace_error = Some(message.clone());
        WorkspaceReload {
            path: changed.to_path_buf(),
            applied: false,
            settings: self.settings(),
            theme: self.theme_state(),
            error: Some(message),
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

        let legacy = inner.settings.disabled_plugins.clone();
        // 1. 插件清单。文件不存在时保留当前清单（例如用户还没提交过）。
        match workspace.read_config_text(&workspace.manifest_path()) {
            Ok(Some(text)) => match PluginManifestFile::from_json(&text) {
                Ok(file) => {
                    // 清单文件里已有的条目 id：这些条目的启停以文件为权威。
                    let file_ids: Vec<String> = file
                        .entries()
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect();
                    let extras = self.seed_legacy_disabled(
                        &legacy,
                        &file_ids,
                        self.expected_manifest_extras(),
                    );
                    let (merged, _) = file.merged_with(extras);
                    if merged != inner.manifest {
                        inner.manifest = merged;
                        changed = true;
                    }
                }
                Err(error) => reason = Some(format!("插件清单无效：{error}")),
            },
            Ok(None) if activation => {
                // 没有清单文件：默认条目 + 已注册的功能插件都从内存默认值来，
                // 因此历史 `disabledPlugins` 对它们全都算「清单里还没有条目」。
                let extras =
                    self.seed_legacy_disabled(&legacy, &[], self.expected_manifest_extras());
                let (merged, _) = PluginManifestFile::defaults().merged_with(extras);
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
                Err(error) => inner
                    .theme_library
                    .mark_broken(id, format!("无法读取主题包 {}：{error}", path.display())),
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
    ///
    /// `bootstrap` 为真时（刚初始化的空目录）顺手写出默认的 `manifest.json` 与
    /// `theme.json`；为假时**只读不写**——关联一个已有的 Git 仓库不能把仓库改脏。
    /// 缺少清单 / 主题配置时用内存里的默认值（三个内置主题 + 浅色），
    /// 用户真正做出选择或启停插件时才会落盘。
    ///
    /// 读**原始字节**再解析，是为了同时拿到「已应用内容」的哈希：切换 / 首次关联
    /// 工作区也必须刷新它，否则新工作区里与旧哈希碰巧一致的改动会被静默吞掉。
    fn activate_workspace(
        &self,
        workspace: Workspace,
        bootstrap: bool,
    ) -> Result<WorkspaceStatus, WorkspaceError> {
        // 先读设置：无效则整体拒绝，当前工作区与设置原样保留。
        let bytes = workspace.read_settings_bytes()?;
        let loaded = match &bytes {
            Some(bytes) => Some(workspace.parse_settings(bytes)?),
            None => None,
        };
        let applied_hash = bytes.as_deref().map(hash_bytes);
        let watcher =
            WorkspaceWatcher::start(workspace.root(), workspace.git_dir().map(Path::to_path_buf))
                .map_err(|error| WorkspaceError::Watch(error.to_string()))?;
        {
            let mut inner = lock(&self.inner);
            if let Some(settings) = loaded {
                inner.settings = settings;
            }
            inner.applied_settings_hash = applied_hash;
            inner.workspace = Some(workspace.clone());
            inner.workspace_error = None;
            // 插件清单 / 主题配置 / 已安装主题包。切换工作区时以目标工作区为准；
            // 读不到的部分保留当前外观并记下中文原因。
            let (_, theme_reason) = self.apply_workspace_config(&mut inner, &workspace, true);
            inner.theme_notice = theme_reason;
        }
        // 工作区记录的插件启停立刻生效（清单是唯一权威）：克隆 / 切换后已有功能
        // 随即按新选择工作。
        self.apply_manifest_plugin_state();
        // 新监听器先生效，读备忘录时的自读记账才能落在正确的监听器上。
        *lock(&self.watch) = Some(watcher);
        // 备忘录：切换工作区时以目标工作区为准（读到的内容就是生效内容）。
        self.load_memos(&workspace);
        if bootstrap {
            // 新工作区写出默认文件（清单与主题选择），让用户可以手写、提交和同步。
            self.bootstrap_workspace_files(&workspace);
        }
        // 记住本机路径，重启后恢复。这是设备本地数据，不写进工作区。
        if let Err(error) = self.device.set_workspace_path(Some(workspace.root())) {
            lock(&self.inner).workspace_error = Some(error.to_string());
        }
        // 新工作区里的插件启停与剪贴板设置随即生效（后台捕获也跟着启停）。
        self.sync_clipboard_runtime();
        Ok(self.workspace_status())
    }

    /// 把**清单**里的功能插件启停状态应用到插件注册表。
    ///
    /// 插件清单（`manifest.json`）是启停状态的**唯一权威**：ticket 06 的
    /// `Settings::disabled_plugins` 只是 ticket 01 的历史字段，仅在清单里还没有该插件
    /// 条目时用作一次性迁移（见 [`Host::seed_legacy_disabled`]），不再覆盖清单里的选择。
    /// 本机没有对应实现的 id 由 [`Host::unavailable_plugins`] 如实报告。
    fn apply_manifest_plugin_state(&self) {
        let entries = lock(&self.inner).manifest.entries().to_vec();
        for entry in entries {
            if entry.kind == PluginKind::Feature {
                self.deps.plugins.set_enabled(&entry.id, entry.enabled);
            }
        }
    }

    /// 注册随应用提供的官方功能插件（ticket 07 起：备忘录）。
    ///
    /// 实现随应用编译进来（与内置主题一样），而「有哪些插件、是否启用」以工作区的
    /// `manifest.json` 为唯一权威：注册后把清单里的条目按既有启用状态补进内存清单，
    /// 再把清单里的启停应用到注册表。`Host::new` **不**自动调用它，因为清单的补全
    /// 会改变「只有默认主题」时的清单内容；外壳与需要真实插件的调用方显式调用。
    pub fn install_official_plugins(&self) {
        crate::plugins::register_official(
            &self.deps.plugins,
            &self.memos,
            &self.bookmarks,
            self.clipboard.store(),
        );
        let merged = {
            let inner = lock(&self.inner);
            let (merged, _) = inner
                .manifest
                .clone()
                .merged_with(self.expected_manifest_extras());
            merged
        };
        lock(&self.inner).manifest = merged;
        self.apply_manifest_plugin_state();
        // 剪贴板历史默认关闭：只有清单里明确启用后，后台捕获线程才会启动。
        self.sync_clipboard_runtime();
    }

    /// 一次性迁移 ticket 01/05 的历史字段 `disabledPlugins`。
    ///
    /// 只对**清单文件里还没有条目**的插件生效：清单一旦记录了某个插件的启停，
    /// 它就是这个插件的唯一权威，设置文件里的旧记录不能再覆盖用户后来的选择。
    /// 关联已有工作区时仍然只读，不把迁移结果写回任何文件。
    fn seed_legacy_disabled(
        &self,
        legacy: &[String],
        file_ids: &[String],
        entries: Vec<ManifestEntry>,
    ) -> Vec<ManifestEntry> {
        if legacy.is_empty() {
            return entries;
        }
        entries
            .into_iter()
            .map(|mut entry| {
                if !file_ids.iter().any(|id| id == &entry.id)
                    && legacy.iter().any(|id| id == &entry.id)
                {
                    entry.enabled = false;
                }
                entry
            })
            .collect()
    }

    /// 为刚初始化的空工作区写出默认清单与主题配置。只在这个入口调用，
    /// 并且只写不存在的文件（目录是应用自己刚建的，不存在覆盖用户内容的风险）。
    fn bootstrap_workspace_files(&self, workspace: &Workspace) {
        let manifest_path = workspace.manifest_path();
        if let Ok(None) = workspace.read_config_text(&manifest_path) {
            let (merged, _) =
                PluginManifestFile::defaults().merged_with(self.expected_manifest_extras());
            if let Ok(bytes) = merged.to_json().map(String::into_bytes) {
                if self
                    .persist_workspace_file(|_| manifest_path.clone(), &bytes)
                    .is_ok()
                {
                    lock(&self.inner).manifest = merged;
                }
            }
        }

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
        if let Err(error) = self.activate_workspace(workspace, false) {
            lock(&self.inner).workspace_error =
                Some(format!("上次使用的配置工作区无法恢复：{error}"));
        }
    }

    /// 把设置写入当前工作区文件，返回落盘内容的哈希（未关联工作区时为 `None`）。
    ///
    /// 返回的哈希由 [`Host::update_settings`] 与设置一起写进 [`HostInner`]，保证「生效设置」
    /// 与「已应用内容」始终同步。
    fn persist_settings(&self, settings: &Settings) -> Result<Option<u64>, SettingsError> {
        let workspace = match &lock(&self.inner).workspace {
            Some(workspace) => workspace.clone(),
            None => return Ok(None),
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
            .map_err(|error| SettingsError::Workspace(error.to_string()))?;
        Ok(Some(hash_bytes(&bytes)))
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
    // 备忘录
    // -----------------------------------------------------------------------

    /// 备忘录插件是否存在于清单中且处于启用状态。
    ///
    /// 停用插件后不贡献结果也不接受写入：管理界面属于该插件的能力范围。
    pub fn memo_plugin_enabled(&self) -> bool {
        lock(&self.inner)
            .manifest
            .get(crate::plugins::MEMO_PLUGIN_ID)
            .map(|entry| entry.kind == PluginKind::Feature && entry.enabled)
            .unwrap_or(false)
            && self.deps.plugins.is_enabled(crate::plugins::MEMO_PLUGIN_ID)
    }

    /// 当前生效的备忘录（按标识排序，与目录读取顺序无关）。
    pub fn memos(&self) -> Vec<Memo> {
        self.memos.snapshot().memos
    }

    /// 无法读取的备忘录文件（保留可用内容并如实报告原因）。
    pub fn memo_problems(&self) -> Vec<MemoProblem> {
        self.memos.snapshot().problems
    }

    /// 新建一条备忘录。未关联工作区或插件已停用时拒绝。
    pub fn create_memo(&self, title: &str, tags: &[String], body: &str) -> Result<Memo, MemoError> {
        let workspace = self.memo_workspace()?;
        let memo = Memo {
            id: memo::new_memo_id(),
            title: validate_title(title)?,
            tags: normalize_tags(tags),
            body: validate_body(body)?,
        };
        let bytes = memo.to_markdown().into_bytes();
        let path = workspace.memo_path(&memo.id);
        self.persist_workspace_file(|_| path.clone(), &bytes)?;
        self.upsert_memo(memo.clone());
        Ok(memo)
    }

    /// 修改一条已存在的备忘录（标识不变）。
    pub fn update_memo(
        &self,
        id: &str,
        title: &str,
        tags: &[String],
        body: &str,
    ) -> Result<Memo, MemoError> {
        let workspace = self.memo_workspace()?;
        if self.memos.find(id).is_none() {
            return Err(MemoError::NotFound(id.to_string()));
        }
        let memo = Memo {
            id: id.to_string(),
            title: validate_title(title)?,
            tags: normalize_tags(tags),
            body: validate_body(body)?,
        };
        let bytes = memo.to_markdown().into_bytes();
        let path = workspace.memo_path(&memo.id);
        self.persist_workspace_file(|_| path.clone(), &bytes)?;
        self.upsert_memo(memo.clone());
        Ok(memo)
    }

    /// 删除一条备忘录（同时删除工作区里的文件）。
    pub fn delete_memo(&self, id: &str) -> Result<(), MemoError> {
        let workspace = self.memo_workspace()?;
        if self.memos.find(id).is_none() {
            return Err(MemoError::NotFound(id.to_string()));
        }
        let path = workspace.memo_path(id);
        // 自写抑制：删除同样要记账，否则监听会把这次删除上报成一次外部修改
        // （见 `ChangeFilter::record_self_delete`）。
        if let Some(watcher) = lock(&self.watch).as_ref() {
            watcher.record_self_delete(&path);
        }
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(MemoError::Io(error.to_string())),
        }
        let mut snapshot = self.memos.snapshot();
        snapshot.memos.retain(|memo| memo.id != id);
        snapshot.problems.retain(|problem| problem.path != path);
        self.memos.replace(snapshot);
        Ok(())
    }

    /// 预览某条结果：备忘录按**当前**内容返回完整正文，其它条目返回结果自带的预览。
    ///
    /// 这是 ADR §3 的补充入口 `preview(item)`：搜索结果是快照，预览要按需展开。
    pub fn preview(&self, item_id: &str) -> Option<Preview> {
        if let Some(bookmark_id) = BookmarkEntry::id_from_item_id(item_id) {
            if let Some(bookmark) = self.bookmarks.find(bookmark_id) {
                return Some(Preview::Text {
                    title: Some(bookmark.title),
                    body: format!("{}\n目录：{}", bookmark.url, bookmark.folder),
                });
            }
        }
        if let Some(memo_id) = crate::plugins::memo::memo_id_from_item_id(item_id) {
            if let Some(memo) = self.memos.find(memo_id) {
                return Some(Preview::Text {
                    title: Some(memo.title),
                    body: memo.body,
                });
            }
        }
        // 剪贴板历史：预览按**当前**存储内容返回，列表快照可能是上一次查询的。
        if let Some(event_id) = crate::clipboard::event_id_from_item_id(item_id) {
            if let Ok(Some(event)) = self.clipboard.store().find(event_id) {
                return Some(Preview::Text {
                    title: Some(event.summary),
                    body: event
                        .text
                        .unwrap_or_else(|| "（该条目没有可显示的文字内容）".to_string()),
                });
            }
        }
        self.item_by_id(item_id).map(|item| item.preview)
    }

    /// 备忘录写入的前置条件：必须关联了配置工作区，且备忘录插件处于启用状态。
    fn memo_workspace(&self) -> Result<Workspace, MemoError> {
        if !self.memo_plugin_enabled() {
            return Err(MemoError::PluginDisabled);
        }
        match &lock(&self.inner).workspace {
            Some(workspace) => Ok(workspace.clone()),
            None => Err(MemoError::NoWorkspace),
        }
    }

    /// 把一条备忘录写进生效内容（工作区文件已经写好）。
    fn upsert_memo(&self, memo: Memo) {
        let mut snapshot = self.memos.snapshot();
        snapshot.memos.retain(|existing| existing.id != memo.id);
        // 同名的坏文件（无法解析的旧内容）问题记录随之消失。
        let memo_id = memo.id.clone();
        snapshot.problems.retain(|problem| {
            problem
                .path
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .as_deref()
                != Some(memo_id.as_str())
        });
        snapshot.memos.push(memo);
        snapshot.memos.sort_by(|a, b| a.id.cmp(&b.id));
        self.memos.replace(snapshot);
    }

    /// 从工作区读取备忘录并更新生效内容。返回生效内容是否发生变化。
    ///
    /// 读取的每一个文件都记进监听账本（[`Self::record_own_read`]）：macOS 的 FSEvents
    /// 会把这次读上报成修改事件（见 `crate::watch` 的模块文档）。
    fn load_memos(&self, workspace: &Workspace) -> bool {
        let before = self.memos.snapshot();
        let snapshot = memo::read_dir_with(&workspace.memos_dir(), &|path, bytes| {
            self.record_own_read(path, bytes)
        });
        if snapshot == before {
            return false;
        }
        self.memos.replace(snapshot);
        true
    }

    // -----------------------------------------------------------------------
    // 剪贴板历史（ticket 09）
    // -----------------------------------------------------------------------

    /// 剪贴板历史插件是否存在于清单中且处于启用状态。
    ///
    /// 停用插件后既不贡献结果也不做后台捕获：这一条同时是搜索与捕获的判据。
    pub fn clipboard_plugin_enabled(&self) -> bool {
        lock(&self.inner)
            .manifest
            .get(crate::plugins::CLIPBOARD_PLUGIN_ID)
            .map(|entry| entry.kind == PluginKind::Feature && entry.enabled)
            .unwrap_or(false)
            && self
                .deps
                .plugins
                .is_enabled(crate::plugins::CLIPBOARD_PLUGIN_ID)
    }

    /// 读取剪贴板是否已获授权。
    ///
    /// 原生边界上的权限校验（ADR §6）：编译进来的插件实现与清单里的记录都必须声明
    /// `clipboard.read`。手写清单删掉这条能力后捕获会停止，而不是继续默默读取剪贴板。
    fn clipboard_read_authorized(&self) -> bool {
        let compiled = self
            .deps
            .plugins
            .manifests()
            .into_iter()
            .find(|(manifest, _)| manifest.id == crate::plugins::CLIPBOARD_PLUGIN_ID)
            .map(|(manifest, _)| manifest);
        let Some(compiled) = compiled else {
            return false;
        };
        if !compiled.requires(CAP_CLIPBOARD_READ) {
            return false;
        }
        lock(&self.inner)
            .manifest
            .get(crate::plugins::CLIPBOARD_PLUGIN_ID)
            .map(|entry| entry.to_feature_manifest().requires(CAP_CLIPBOARD_READ))
            .unwrap_or(false)
    }

    /// 把设置里的记录范围同步给运行时，并按插件启用状态启停后台捕获。
    ///
    /// 停用插件会**停止后台线程**，而不只是让它空转（spec「停用插件同时停止搜索贡献
    /// 和后台活动」）。本方法幂等，可以在任何配置变化后安全重放。
    pub fn sync_clipboard_runtime(&self) {
        let (paused, retention_days, capacity) = {
            let inner = lock(&self.inner);
            let clipboard = &inner.settings.clipboard;
            (
                clipboard.paused,
                clipboard.retention_days,
                clipboard.capacity,
            )
        };
        self.clipboard.configure(paused, retention_days, capacity);
        if self.clipboard_plugin_enabled() && self.clipboard_read_authorized() {
            self.clipboard.start();
        } else {
            self.clipboard.stop();
        }
    }

    /// 本机剪贴板历史存储（诊断与测试用）。
    ///
    /// 暴露它是为了让「保留期限」这类**与时钟有关**的行为可以被验证：宿主不提供
    /// 加速时间的能力，测试只能用同一个回收入口把「现在」推到期限之外，再从查询入口
    /// 断言结果。
    pub fn clipboard_store(&self) -> &Arc<ClipboardStore> {
        self.clipboard.store()
    }

    /// 后台捕获线程是否正在运行。
    pub fn clipboard_capture_active(&self) -> bool {
        self.clipboard.is_running()
    }

    /// 显式开始后台捕获（插件未启用时不会启动）。外壳启动时调用。
    pub fn start_clipboard_capture(&self) {
        self.sync_clipboard_runtime();
    }

    /// 显式停止后台捕获（不改变插件启用状态）。
    ///
    /// 测试用它取得确定性：停止线程后再手动调用 [`Host::capture_clipboard_once`]，
    /// 结果就完全由测试驱动。
    pub fn stop_clipboard_capture(&self) {
        self.clipboard.stop();
    }

    /// 同步地捕获一次剪贴板。
    ///
    /// 这是后台轮询线程与集成测试共用的**唯一**捕获入口：
    ///
    /// 1. 插件未启用 → [`ClipboardCaptureOutcome::Disabled`]，且**不读取**剪贴板；
    /// 2. 未声明 `clipboard.read` → 如实失败；
    /// 3. 其余交给运行时：暂停、去重、自身写入抑制、容量与回收都在那里。
    pub fn capture_clipboard_once(&self) -> ClipboardCaptureOutcome {
        if !self.clipboard_plugin_enabled() {
            return ClipboardCaptureOutcome::Disabled;
        }
        if !self.clipboard_read_authorized() {
            return ClipboardCaptureOutcome::Failed {
                message: "剪贴板插件没有声明 clipboard.read 能力，宿主不会读取剪贴板".to_string(),
            };
        }
        self.clipboard.capture_once()
    }

    /// 剪贴板历史的完整状态（面向 UI 与诊断）。
    ///
    /// 只读入口，不会启动后台线程；配置变化由
    /// [`Host::sync_clipboard_runtime`] 的各个调用点负责同步。
    pub fn clipboard_state(&self) -> ClipboardState {
        let store = self.clipboard.store();
        let stats = store.stats().unwrap_or_default();
        let snapshot = self.clipboard.snapshot();
        let settings = lock(&self.inner).settings.clipboard.clone();
        let storage_error = store.storage_error();
        ClipboardState {
            enabled: self.clipboard_plugin_enabled(),
            paused: settings.paused,
            capture_active: self.clipboard.is_running(),
            storage_ok: storage_error.is_none(),
            last_error: snapshot
                .last_error
                .clone()
                .or_else(|| storage_error.clone()),
            storage_error,
            storage_path: store.db_path(),
            entries: stats.total,
            pinned: stats.pinned,
            attachments: stats.attachments,
            capacity: settings.capacity,
            retention_days: settings.retention_days,
            capacity_reached: snapshot.capacity_reached,
            last_capture_ms: snapshot.last_capture_ms,
            suppressed: snapshot.suppressed,
        }
    }

    /// 当前历史里的条目（置顶在前，然后按时间倒序）。`query` 为空时列出全部。
    pub fn clipboard_entries(&self, query: Option<&str>) -> Vec<crate::clipboard::ClipboardEvent> {
        self.clipboard.store().list(query, 200).unwrap_or_default()
    }

    /// 置顶 / 取消置顶一条历史。找不到时返回中文原因。
    pub fn pin_clipboard_entry(&self, id: &str, pinned: bool) -> Result<(), ClipboardActionError> {
        match self.clipboard.store().set_pinned(id, pinned) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ClipboardActionError::NotFound(id.to_string())),
            Err(error) => Err(ClipboardActionError::Storage(error.to_string())),
        }
    }

    /// 删除一条历史（同时回收不再被引用的附件）。
    pub fn delete_clipboard_entry(&self, id: &str) -> Result<(), ClipboardActionError> {
        match self.clipboard.store().delete(id) {
            Ok(true) => Ok(()),
            Ok(false) => Err(ClipboardActionError::NotFound(id.to_string())),
            Err(error) => Err(ClipboardActionError::Storage(error.to_string())),
        }
    }

    /// 清空历史（用户在设置里的显式操作，置顶条目也会被清掉）。
    pub fn clear_clipboard_history(&self) -> Result<usize, ClipboardActionError> {
        self.clipboard
            .store()
            .clear()
            .map_err(|error| ClipboardActionError::Storage(error.to_string()))
    }

    /// 暂停 / 恢复记录。设置会随其它偏好写进配置工作区。
    pub fn set_clipboard_paused(&self, paused: bool) -> Result<Settings, SettingsError> {
        let mut settings = self.settings();
        settings.clipboard.paused = paused;
        self.update_settings(settings)
    }

    /// 设置保留期限与容量，并立即按新范围回收一次。
    pub fn set_clipboard_limits(
        &self,
        retention_days: u32,
        capacity: usize,
    ) -> Result<Settings, SettingsError> {
        let mut settings = self.settings();
        settings.clipboard.retention_days = retention_days;
        settings.clipboard.capacity = capacity;
        let applied = self.update_settings(settings)?;
        let now = crate::clipboard::now_ms();
        // 立即回收：用户把容量改小之后不该等到下一次复制才生效。
        let _ = self.clipboard.store().reclaim(
            applied.clipboard.retention_days,
            applied.clipboard.capacity,
            now,
        );
        Ok(applied)
    }
    // -----------------------------------------------------------------------
    // Chrome 书签（ticket 13）
    // -----------------------------------------------------------------------

    /// 当前 Chrome 状态：发现结果、profile 列表、关联状态与书签索引状态。
    ///
    /// 这是 UI 轮询「书签文件有没有变化」的入口：每次调用都会按 mtime + size 检查
    /// 书签文件，变化时重建索引（ADR §3 的补充入口）。慢操作（读文件、发现 Chrome）
    /// 全部在锁外完成，只有汇总状态时才短暂持有内部锁。
    pub fn chrome_state(&self) -> ChromeState {
        self.bookmarks.refresh();
        let association = self.association();
        let discovered = self.deps.chrome.discover();

        let snapshot = self.bookmarks.snapshot();
        let mut state = ChromeState {
            bookmarks_label: snapshot.status.label_zh(),
            bookmarks: snapshot,
            associated: association.as_ref().map(|item| item.profile_dir.clone()),
            associated_name: association.as_ref().map(|item| item.display_name.clone()),
            ..ChromeState::not_associated()
        };

        let mut live_error: Option<String> = None;
        match discovered {
            Ok(environment) => {
                state.available = true;
                state.brand_label = Some(environment.brand.label_zh().to_string());
                state.custom_user_data_dir = environment.pass_user_data_dir;
                state.binary = Some(environment.binary.clone());
                state.user_data_dir = Some(environment.user_data_dir.clone());
                state.warnings = environment.warnings.clone();
                state.profiles = environment
                    .profiles
                    .iter()
                    .map(|profile| ChromeProfileView {
                        dir: profile.dir.clone(),
                        name: profile.name.clone(),
                        user_name: profile.user_name.clone(),
                        managed: profile.managed,
                        has_bookmarks: profile.has_bookmarks,
                        bookmarks_readable: profile.bookmarks_readable,
                        unreadable_reason: profile.unreadable_reason.clone(),
                        associated: association
                            .as_ref()
                            .map(|item| item.profile_dir == profile.dir)
                            .unwrap_or(false),
                    })
                    .collect();
                if let Some(item) = &association {
                    if !item.profile_dir_exists() {
                        live_error = Some(format!(
                            "关联的 Chrome profile「{}」目录已不存在，请在设置里重新选择",
                            item.display_name
                        ));
                    }
                }
            }
            Err(failure) => {
                live_error = Some(failure.to_string());
            }
        }

        // 设备本地记录本身有问题时优先展示（它只能靠重新关联修复）。
        let record_error = lock(&self.inner).chrome_error.clone();
        state.error = record_error.or(live_error);
        state
    }

    /// 关联一个已发现的 Chrome profile。
    ///
    /// 只接受**发现结果里存在**的目录名：Chrome 在 `--profile-directory` 指向不存在的
    /// 目录时会静默新建一个空 profile，因此这里先校验（研究笔记 §4）。关联记录写进
    /// 设备本地存储，不进入配置工作区。
    pub fn associate_chrome_profile(
        &self,
        profile_dir: &str,
    ) -> Result<ChromeState, ChromeBookmarkError> {
        let environment = self
            .deps
            .chrome
            .discover()
            .map_err(ChromeBookmarkError::from)?;
        let profile = environment
            .profile(profile_dir)
            .ok_or_else(|| {
                ChromeBookmarkError::ProfileMissing(format!(
                    "{profile_dir}（已发现的 profile：{}）",
                    environment
                        .profiles
                        .iter()
                        .map(|profile| profile.dir.as_str())
                        .collect::<Vec<_>>()
                        .join("、")
                ))
            })?
            .clone();

        let association = ChromeAssociation {
            profile_dir: profile.dir.clone(),
            display_name: profile.name.clone(),
            user_data_dir: environment.user_data_dir.clone(),
            binary: environment.binary.clone(),
            pass_user_data_dir: environment.pass_user_data_dir,
        };
        self.persist_chrome_association(&association)?;
        self.apply_chrome_association(Some(&association));
        {
            // 记录已经有效，清掉「设备本地记录损坏」之类的旧问题。
            lock(&self.inner).chrome_error = None;
        }
        Ok(self.chrome_state())
    }

    /// 显式重新读取书签文件（丢弃指纹，一定重读）。
    pub fn refresh_chrome_bookmarks(&self) -> ChromeState {
        self.bookmarks.invalidate();
        self.chrome_state()
    }

    /// 书签索引快照（插件与设置界面都只读它）。
    pub fn chrome_bookmarks(&self) -> crate::chrome::BookmarkSnapshot {
        self.bookmarks.snapshot()
    }

    /// Chrome 书签插件是否存在于清单中且处于启用状态。
    pub fn chrome_plugin_enabled(&self) -> bool {
        lock(&self.inner)
            .manifest
            .get(crate::plugins::CHROME_PLUGIN_ID)
            .map(|entry| entry.kind == PluginKind::Feature && entry.enabled)
            .unwrap_or(false)
            && self
                .deps
                .plugins
                .is_enabled(crate::plugins::CHROME_PLUGIN_ID)
    }

    /// 当前关联的 profile（设备本地数据）。
    fn association(&self) -> Option<ChromeAssociation> {
        lock(&self.inner).chrome_association.clone()
    }

    /// 启动时从设备本地存储恢复关联，让重启后可以直接检索与打开。
    fn restore_chrome_association(&self) {
        let raw = match self.device.get(KEY_CHROME_ASSOCIATION) {
            Ok(raw) => raw,
            Err(error) => {
                lock(&self.inner).chrome_error =
                    Some(format!("无法读取设备本地的 Chrome 关联：{error}"));
                return;
            }
        };
        let Some(raw) = raw else { return };
        match serde_json::from_str::<ChromeAssociation>(&raw) {
            Ok(association) => self.apply_chrome_association(Some(&association)),
            Err(error) => {
                lock(&self.inner).chrome_error = Some(format!(
                    "设备本地的 Chrome 关联记录无法解析（{error}）；请在设置里重新选择 profile"
                ));
            }
        }
    }

    /// 写入设备本地的关联记录。
    fn persist_chrome_association(
        &self,
        association: &ChromeAssociation,
    ) -> Result<(), ChromeBookmarkError> {
        let text = serde_json::to_string_pretty(association)
            .map_err(|error| ChromeBookmarkError::Device(error.to_string()))?;
        self.device
            .put(KEY_CHROME_ASSOCIATION, &text)
            .map_err(|error| ChromeBookmarkError::Device(error.to_string()))
    }

    /// 让生效状态跟着关联走：索引指向新的书签文件并立刻重建。
    fn apply_chrome_association(&self, association: Option<&ChromeAssociation>) {
        {
            let mut inner = lock(&self.inner);
            inner.chrome_association = association.cloned();
        }
        self.bookmarks
            .set_path(association.map(ChromeAssociation::bookmarks_path));
        self.bookmarks.refresh();
    }

    /// 书签的默认操作：在关联的 Chrome profile 里打开。
    ///
    /// 权限校验在原生边界：来源插件必须在清单里、已启用，并声明 `chrome.open`。
    /// 打开前**立刻重读书签文件**（列表可能是上一次查询的快照），再校验 profile 目录
    /// 确实存在、URL 可以安全地作为 argv 交给 Chrome。启动只 `spawn`，不等待也不以
    /// 退出码判断页面是否打开。
    fn execute_bookmark(&self, item: &SearchItem) -> ActionOutcome {
        if let Err(outcome) =
            self.feature_source(item, crate::plugins::CAP_CHROME_OPEN, "在 Chrome 打开")
        {
            return outcome;
        }
        let Some(association) = self.association() else {
            return ActionOutcome::failed(ChromeBookmarkError::NoAssociation.to_string());
        };
        let Some(bookmark_id) = BookmarkEntry::id_from_item_id(&item.id) else {
            return ActionOutcome::failed(format!("无法识别的书签条目：{}", item.id));
        };
        // 先校验 profile 目录确实存在：`--profile-directory` 指向不存在的目录时
        // Chrome 会静默新建一个空 profile（研究笔记 §4），这一步必须在启动之前。
        if !association.profile_dir_exists() {
            return ActionOutcome::failed(
                ChromeBookmarkError::ProfileMissing(association.profile_dir.clone()).to_string(),
            );
        }
        // 打开前立即重读：书签可能刚在 Chrome 里被删掉或改过。
        self.bookmarks.refresh();
        let Some(bookmark) = self.bookmarks.find(bookmark_id) else {
            return ActionOutcome::failed(
                ChromeBookmarkError::BookmarkMissing(item.title.clone()).to_string(),
            );
        };
        let url = match validate_open_url(&bookmark.url) {
            Ok(url) => url.to_string(),
            Err(error) => return ActionOutcome::failed(error.to_string()),
        };
        let user_data_dir = association
            .pass_user_data_dir
            .then_some(association.user_data_dir.as_path());
        let request = ChromeLaunchRequest::new(
            association.binary.clone(),
            build_open_args(&association.profile_dir, user_data_dir, &url),
        );
        match self.deps.chrome.launch(&request) {
            Ok(_receipt) => ActionOutcome::done(Some(format!(
                "已请求 Chrome 用 profile「{}」打开：{}；Chrome 已运行时由现有进程接管，\
                 Flashcast 不等待进程退出，也无法据此确认页面是否已加载",
                association.display_name, url
            ))),
            Err(error) => ActionOutcome::failed(ChromeBookmarkError::from(error).to_string()),
        }
    }

    /// 命令入口的通用前置校验：来源插件必须在清单里、已启用，并声明所需能力。
    fn feature_source(
        &self,
        item: &SearchItem,
        capability: &str,
        native_action: &str,
    ) -> Result<(), ActionOutcome> {
        let entry = lock(&self.inner).manifest.get(&item.source).cloned();
        let Some(entry) = entry.filter(|entry| entry.kind == PluginKind::Feature) else {
            return Err(ActionOutcome::failed(format!(
                "结果来源「{}」不在插件清单里，已拒绝执行",
                item.source
            )));
        };
        if !entry.enabled || !self.deps.plugins.is_enabled(&item.source) {
            return Err(ActionOutcome::failed(format!(
                "插件「{}」已停用，已拒绝执行",
                entry.name
            )));
        }
        if !entry.to_feature_manifest().requires(capability) {
            return Err(ActionOutcome::failed(format!(
                "插件「{}」没有声明 {} 能力，宿主不会替它{}",
                entry.name, capability, native_action
            )));
        }
        Ok(())
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
        let workspace = self.workspace()?.ok_or(ThemeError::NoWorkspace("安装"))?;
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
        self.persist_manifest(&manifest)
            .map_err(|error| ThemeError::Workspace(error.to_string()))?;
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
        let workspace = self.workspace()?.ok_or(ThemeError::NoWorkspace("移除"))?;
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
        self.persist_manifest(&manifest)
            .map_err(|error| ThemeError::Workspace(error.to_string()))?;
        let notice = fallback
            .as_ref()
            .map(|_| format!("主题「{}」已移除，已切换回「浅色」", entry.name));
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
    /// 停用当前选中的主题会退回内置浅色主题，并给出中文原因。停用一个功能插件时
    /// 同时丢掉它的范围对象：如果用户正停在该插件的范围里，直接回到首屏，并按当前
    /// 输入重算一次结果——列表里不会留着已停用插件的结果。
    pub fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<(), ManifestError> {
        let (manifest, selection, notice, feature_changed) = {
            let mut inner = lock(&self.inner);
            let entry =
                inner.manifest.get(id).cloned().ok_or_else(|| {
                    ManifestError::invalid(format!("插件清单里没有这个标识：{id}"))
                })?;
            inner.manifest.set_enabled(id, enabled);
            let mut feature_changed = false;
            if entry.kind == PluginKind::Feature {
                self.deps.plugins.set_enabled(id, enabled);
                feature_changed = true;
                // 范围对象属于「已进入某个插件的范围」这一状态：插件停了就不能再留着。
                inner.plugin_scopes.remove(id);
                if !enabled
                    && matches!(&inner.scope, QueryScope::Plugin { id: current, .. } if current == id)
                {
                    inner.scope = QueryScope::Home;
                }
            }
            let mut notice = None;
            let selection =
                if entry.kind == PluginKind::Theme && !enabled && inner.selected_theme == id {
                    inner.selected_theme = THEME_LIGHT.to_string();
                    inner.theme_error = None;
                    notice = Some(format!("主题「{}」已停用，已切换回「浅色」", entry.name));
                    Some(ThemeSelection::new(THEME_LIGHT))
                } else {
                    None
                };
            (inner.manifest.clone(), selection, notice, feature_changed)
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
        if feature_changed {
            // 启停也可能改变后台活动：启用剪贴板历史即开始捕获，停用即停止线程。
            self.sync_clipboard_runtime();
            // 启停改变了「哪些来源参与搜索」，按当前输入重算，让界面立刻反映新状态。
            let input = lock(&self.inner).input.clone();
            let seq = self.next_seq();
            let mut inner = lock(&self.inner);
            self.search(&mut inner, &input, SearchMode::UserInput, None, seq);
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
            Some(entry) if !entry.enabled => {
                error = Some(format!(
                    "主题「{}」已停用，继续使用上一次可用外观",
                    entry.name
                ))
            }
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
                        inner.theme_appearance =
                            document.resolved_appearance(inner.system_appearance);
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
                    appearance: entry
                        .appearance
                        .unwrap_or(crate::theme::ThemeAppearance::Light),
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
            ItemKind::Memo => self.execute_memo(item),
            ItemKind::ClipboardEntry => self.execute_clipboard_entry(item),
            ItemKind::Bookmark => self.execute_bookmark(item),
        }
    }

    // -----------------------------------------------------------------------
    // 自动粘贴（spec「粘贴是宿主级操作」，ticket 08）
    // -----------------------------------------------------------------------

    /// 外壳在**唤起时**（显示窗口之前）把唤起前的前台应用交给宿主。
    ///
    /// 每次都覆盖，并作废上一次未完成的粘贴计划：这样「上一次唤起的应用」不可能被
    /// 这一轮粘贴用到。`None` 表示本次拿不到（例如 Wayland 不允许读取全局焦点），
    /// 此时自动粘贴会自动降级为「已复制，请手动粘贴」，绝不会猜一个目标。
    pub fn set_paste_target(&self, target: Option<FocusedApp>) {
        let mut paste = lock(&self.paste);
        paste.target = target;
        paste.plan = None;
    }

    /// 当前记录的粘贴目标（唤起前的应用）。诊断与测试用。
    pub fn paste_target(&self) -> Option<FocusedApp> {
        lock(&self.paste).target.clone()
    }

    /// 外壳在**没有**执行粘贴的情况下关闭浮窗时调用（Escape、失焦、托盘切换）。
    ///
    /// 丢掉待完成的计划，这样随后哪怕有一次迟到的 `complete_paste` 也不会注入按键：
    /// 「快速关闭」不可能粘贴到上一次的选择。
    pub fn cancel_paste(&self) {
        lock(&self.paste).plan = None;
    }

    /// 是否有待外壳完成的粘贴计划。
    pub fn has_pending_paste(&self) -> bool {
        lock(&self.paste).plan.is_some()
    }

    /// 剪贴板**已经写入** `text` 之后，决定能否自动粘贴，并给出准确的结果。
    ///
    /// 这是粘贴流程里可复用的接缝（ticket 09 的剪贴板历史走同一条路）：调用方先把内容
    /// 写进剪贴板，再调用本方法，就会得到两种结果之一：
    ///
    /// - [`ActionOutcome::paste_pending`]：外壳必须关闭浮窗、等焦点交出，然后调用
    ///   [`Host::complete_paste`] 完成恢复 + 注入；
    /// - [`ActionOutcome::copied_needs_manual_paste`]：内容已在剪贴板，中文反馈里说清
    ///   为什么没有自动粘贴以及用户该怎么做。
    ///
    /// 决策只依据两件事：平台能力报告（Wayland / 缺权限 / 未覆盖都算不能）与
    /// 唤起时捕获到的目标应用。任何一项不成立都**不**尝试注入。
    pub fn finish_copy_for_paste(&self, label: &str, text: &str) -> ActionOutcome {
        self.finish_copy_for_paste_with_note(label, text, None)
    }

    /// 同 [`Host::finish_copy_for_paste`]，但把一段**格式说明**带进反馈。
    ///
    /// ticket 11 用它如实报告降级：剪贴板里实际提供了哪些格式、哪些没有以及为什么。
    /// 富文本没能同时提供时（Linux 的 `wl-copy`、macOS 的 `pbcopy`），用户必须在反馈里
    /// 看到这一点，而不是以为样式也一起过去了。
    pub fn finish_copy_for_paste_with_note(
        &self,
        label: &str,
        text: &str,
        note: Option<String>,
    ) -> ActionOutcome {
        let capabilities = self.capabilities();
        let with_note = |message: String| match note.as_deref() {
            Some(note) if !note.is_empty() => format!("{message}；{note}"),
            _ => message,
        };
        let target = { lock(&self.paste).target.clone() };
        let Some(target) = target else {
            return ActionOutcome::copied_needs_manual_paste(with_note(manual_paste_message(
                label,
                capabilities.os,
                Some("没有记录到唤起前的应用，无法确定粘贴目标"),
            )));
        };
        if let Some(blocker) = auto_paste_blocker(&capabilities) {
            return ActionOutcome::copied_needs_manual_paste(with_note(manual_paste_message(
                label,
                capabilities.os,
                Some(&blocker),
            )));
        }
        let epoch = self.paste_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        let target_name = target.name.clone();
        let plan = PastePlan {
            target,
            label: label.to_string(),
            epoch,
            text_bytes: text.len(),
            formats_note: note,
        };
        {
            // 覆盖旧计划：只有最新一次执行会被粘贴。
            lock(&self.paste).plan = Some(plan.clone());
        }
        ActionOutcome::paste_pending(
            plan,
            format!("已复制「{label}」，正在粘贴到「{target_name}」…"),
        )
    }

    /// 外壳关闭浮窗之后调用：恢复目标应用、核对前台、注入系统粘贴。
    ///
    /// 顺序与判据都不可省略：
    ///
    /// 1. 取出**最新**的粘贴计划；没有计划或计划已过期（期间又执行了别的条目）就放弃；
    /// 2. 把焦点还给唤起前的应用；
    /// 3. 回读前台，确认它**就是**捕获的那个应用——不是则绝不注入（spec 明确禁止把
    ///    内容粘贴到别的应用）；
    /// 4. 注入系统粘贴。
    ///
    /// 2–4 任何一步失败都退回「已复制，请手动粘贴」：内容已经在剪贴板里，用户按一次
    /// 粘贴键即可，不会既没粘贴又没有提示。
    pub fn complete_paste(&self) -> ActionOutcome {
        let capabilities = self.capabilities();
        let plan = { lock(&self.paste).plan.take() };
        let Some(plan) = plan else {
            return ActionOutcome::failed("没有待完成的粘贴：窗口可能已被关闭或该操作已被取消");
        };
        if plan.epoch != self.paste_epoch.load(Ordering::SeqCst) {
            return ActionOutcome::failed(format!(
                "粘贴计划已过期（期间执行了其它条目），已丢弃「{}」的粘贴",
                plan.label
            ));
        }
        let manual = |blocker: String| {
            let message = manual_paste_message(&plan.label, capabilities.os, Some(&blocker));
            ActionOutcome::copied_needs_manual_paste(match plan.formats_note.as_deref() {
                Some(note) if !note.is_empty() => format!("{message}；{note}"),
                _ => message,
            })
        };
        if let Err(error) = self.deps.focus.restore(&plan.target) {
            return manual(format!("无法把焦点还给「{}」：{error}", plan.target.name));
        }
        // 恢复焦点后必须回读核对：把内容粘贴到用户没有预期的窗口是明确的错误。
        match self.deps.focus.capture() {
            Ok(current) if same_app(&current, &plan.target) => {}
            Ok(current) => {
                return manual(format!(
                    "焦点恢复后前台是「{}」，不是唤起前的「{}」，已取消自动粘贴",
                    current.name, plan.target.name
                ))
            }
            Err(error) => {
                return manual(format!(
                    "无法确认焦点已回到「{}」：{error}",
                    plan.target.name
                ))
            }
        }
        match self.deps.paster.paste() {
            Ok(()) => ActionOutcome::done(Some(match plan.formats_note.as_deref() {
                Some(note) if !note.is_empty() => {
                    format!("已粘贴「{}」到「{}」；{note}", plan.label, plan.target.name)
                }
                _ => format!("已粘贴「{}」到「{}」", plan.label, plan.target.name),
            })),
            Err(error) => manual(format!("自动粘贴没有成功：{error}")),
        }
    }

    /// 备忘录的默认操作：把**当前**内容写进剪贴板，然后尽力粘贴回唤起前的应用。
    ///
    /// 权限校验放在**原生边界**：来源插件必须在清单里、已启用，并且声明了
    /// `clipboard.write`；随后才调用平台剪贴板适配层。
    ///
    /// 内容以工作区里**当前**的内容为准，而不是列表快照里的：列表可能是上一次查询的
    /// 结果，用户可能在期间改过这条备忘录。写进剪贴板的永远是「刚刚执行的那条内容」。
    fn execute_memo(&self, item: &SearchItem) -> ActionOutcome {
        let Some(memo_id) = crate::plugins::memo::memo_id_from_item_id(&item.id) else {
            return ActionOutcome::failed(format!("无法识别的备忘录条目：{}", item.id));
        };
        let entry = lock(&self.inner).manifest.get(&item.source).cloned();
        let Some(entry) = entry.filter(|entry| entry.kind == PluginKind::Feature) else {
            return ActionOutcome::failed(format!(
                "结果来源「{}」不在插件清单里，已拒绝执行",
                item.source
            ));
        };
        if !entry.enabled || !self.deps.plugins.is_enabled(&item.source) {
            return ActionOutcome::failed(format!("插件「{}」已停用，已拒绝执行", entry.name));
        }
        if !entry.to_feature_manifest().requires(CAP_CLIPBOARD_WRITE) {
            return ActionOutcome::failed(format!(
                "插件「{}」没有声明 {} 能力，宿主不会替它写入剪贴板",
                entry.name, CAP_CLIPBOARD_WRITE
            ));
        }
        if let Support::Unsupported { reason } = self.capabilities().clipboard {
            return ActionOutcome::failed(format!("系统剪贴板不可用：{reason}"));
        }
        let Some(memo) = self.memos.find(memo_id) else {
            return ActionOutcome::failed(format!(
                "找不到「{}」对应的备忘录，可能已被删除或改名，请重新查询",
                item.title
            ));
        };
        let (label, text) = (memo.title.clone(), memo.body.clone());
        if let Err(error) = self.write_clipboard_text(&text) {
            return ActionOutcome::failed(format!("无法复制「{label}」：{error}"));
        }
        self.finish_copy_for_paste(&label, &text)
    }

    /// 把内容写进系统剪贴板，并登记「这是 Flashcast 自己的写入」。
    ///
    /// 所有宿主写剪贴板的路径都必须经过这里：没有登记的写入会被自己的后台捕获重新收成
    /// 一条新历史，而「粘贴历史条目 → 又被捕获 → 再粘贴」正是 spec 明确禁止的循环。
    fn write_clipboard_text(&self, text: &str) -> Result<(), flashcast_platform::ClipboardError> {
        self.write_clipboard_content(&ClipboardContent::text(text))
            .map(|_| ())
    }

    /// 按平台公开格式把同一次复制的内容写进系统剪贴板（文本 + HTML/RTF）。
    ///
    /// 与 [`Host::write_clipboard_text`] 一样要登记自身写入（按纯文本指纹，两层抑制各自
    /// 成立）。返回的 [`ClipboardWriteReport`] 如实说明**实际提供了哪些格式**：平台做
    /// 不到一次提供多种格式时（Linux 的 `wl-copy`、macOS 的 `pbcopy`），调用方据此给用户
    /// 准确的中文反馈，而不是声称富文本样式已经保留。
    fn write_clipboard_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, flashcast_platform::ClipboardError> {
        let report = self.deps.clipboard.write_content(content)?;
        self.clipboard.note_own_write(&content.text);
        Ok(report)
    }

    /// 剪贴板历史条目的默认操作：把**当前**内容写进剪贴板，然后尽力粘贴回唤起前的应用。
    ///
    /// 权限校验放在**原生边界**：来源插件必须在清单里、已启用，并且声明了
    /// `clipboard.write`；随后才调用平台剪贴板适配层。恢复与复制回退全部复用
    /// ticket 08 的 [`Host::finish_copy_for_paste`]。
    ///
    /// ticket 11：写回的是**同一次复制的全部公开格式**——纯文本加上历史里保存的
    /// HTML/RTF 载荷。目标应用自己挑：富文本目标取格式化版本，纯文本目标取文本。
    /// 平台没能提供某些格式时，结果反馈里如实写明提供了哪些、哪些没有以及原因。
    fn execute_clipboard_entry(&self, item: &SearchItem) -> ActionOutcome {
        let Some(event_id) = crate::clipboard::event_id_from_item_id(&item.id) else {
            return ActionOutcome::failed(format!("无法识别的剪贴板条目：{}", item.id));
        };
        let entry = lock(&self.inner).manifest.get(&item.source).cloned();
        let Some(entry) = entry.filter(|entry| entry.kind == PluginKind::Feature) else {
            return ActionOutcome::failed(format!(
                "结果来源「{}」不在插件清单里，已拒绝执行",
                item.source
            ));
        };
        if !entry.enabled || !self.deps.plugins.is_enabled(&item.source) {
            return ActionOutcome::failed(format!("插件「{}」已停用，已拒绝执行", entry.name));
        }
        if !entry.to_feature_manifest().requires(CAP_CLIPBOARD_WRITE) {
            return ActionOutcome::failed(format!(
                "插件「{}」没有声明 {} 能力，宿主不会替它写入剪贴板",
                entry.name, CAP_CLIPBOARD_WRITE
            ));
        }
        if let Support::Unsupported { reason } = self.capabilities().clipboard {
            return ActionOutcome::failed(format!("系统剪贴板不可用：{reason}"));
        }
        let event = match self.clipboard.store().find(event_id) {
            Ok(Some(event)) => event,
            Ok(None) => {
                return ActionOutcome::failed(format!(
                    "找不到「{}」对应的剪贴板历史，可能已被删除或过期回收，请重新查询",
                    item.title
                ))
            }
            Err(error) => return ActionOutcome::failed(error.to_string()),
        };
        let Some(text) = event.text.clone() else {
            // 「超出容量、格式不支持或文件失效时看到明确状态」：图片与文件历史的恢复在
            // tickets 10–12，这里如实说明，而不是假装粘贴成功。
            return ActionOutcome::failed(format!(
                "「{}」没有可直接粘贴的文字内容：图片与文件历史的恢复将在后续版本提供",
                event.summary
            ));
        };
        let content = clipboard_content_for(&event, text.clone());
        let report = match self.write_clipboard_content(&content) {
            Ok(report) => report,
            Err(error) => {
                return ActionOutcome::failed(format!("无法复制「{}」：{error}", event.summary))
            }
        };
        // 只有真的降级时才带上说明：全部格式都写进去了就不必打扰用户。
        let note = report.degraded().then(|| report.describe_zh());
        self.finish_copy_for_paste_with_note(&event.summary, &text, note)
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
            other => match other.strip_prefix(PLUGIN_ENTRY_PREFIX) {
                // 首屏的插件入口条目：等价于用户直接输入该插件的关键词。
                Some(plugin_id) => self.enter_plugin_scope(plugin_id),
                None => ActionOutcome::failed(format!("未知命令：{other}")),
            },
        }
    }

    /// 进入插件范围（首屏插件入口条目的执行路径）。
    ///
    /// 与「用户直接把关键词打全」走同一段逻辑，唯一区别是跳过关键词与标签的冲突保留：
    /// 用户已经明确选择了插件入口，此时必须真的进入范围，而不是又留在首屏。
    fn enter_plugin_scope(&self, plugin_id: &str) -> ActionOutcome {
        let entry = lock(&self.inner).manifest.get(plugin_id).cloned();
        let Some(entry) = entry.filter(|entry| entry.kind == PluginKind::Feature) else {
            return ActionOutcome::failed(format!("插件「{plugin_id}」不在清单里，无法进入"));
        };
        if !entry.enabled || !self.deps.plugins.is_enabled(plugin_id) {
            return ActionOutcome::failed(format!("插件「{}」已停用，无法进入", entry.name));
        }
        let Some(keyword) = entry.keywords.first().cloned() else {
            return ActionOutcome::failed(format!("插件「{}」没有可用的关键词", entry.name));
        };
        let seq = self.next_seq();
        let mut inner = lock(&self.inner);
        let input = inner.input.clone();
        self.search(
            &mut inner,
            &input,
            SearchMode::ExplicitPluginEntry,
            None,
            seq,
        );
        ActionOutcome::done(Some(format!(
            "已进入「{}」范围（关键词 {keyword}）",
            entry.name
        )))
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
        let same_input = previous_input == input;
        let normalized = input.trim().to_lowercase();

        // 查询范围切换：输入完整匹配插件关键词时进入该插件范围。
        //
        // 例外是**关键词与标签冲突**（ADR §4）：如果首屏还有「标签精确匹配」的备忘录候选，
        // 就留在首屏，同时给出明确的插件入口条目。任何一方都不静默消失。
        let keyword_scope = self.deps.plugins.take_scope(input);
        // 冲突判定用的关键词：插件清单里记录的那个别名，与用户输入等价。
        let collision_keyword = keyword_scope.as_ref().and_then(|(manifest, _)| {
            manifest
                .matches_keyword(&normalized)
                .or_else(|| Some(normalized.clone()))
        });
        // 已经在同一个插件的范围里时不算冲突：那是用户在范围里继续输入（关键词本身就
        // 是范围的入口），此时必须留在范围内，否则「执行入口进入范围」会被下一次查询弹回首屏。
        let keyword_plugin = keyword_scope
            .as_ref()
            .map(|(manifest, _)| manifest.id.clone());
        let already_in_keyword_scope = match (&inner.scope, &keyword_plugin) {
            (QueryScope::Plugin { id, .. }, Some(keyword_id)) => id == keyword_id,
            _ => false,
        };
        let collision = match (&collision_keyword, mode) {
            (Some(keyword), SearchMode::UserInput) if !already_in_keyword_scope => {
                self.home_tag_hits(inner, keyword)
            }
            _ => Vec::new(),
        };
        // 入口条目用的清单信息：必须在移动 `scope` 之前克隆出来。
        let keyword_entry = keyword_scope
            .as_ref()
            .filter(|_| !collision.is_empty())
            .map(|(manifest, _)| manifest.clone());
        match keyword_scope {
            // 有冲突：留在首屏（可能是从插件范围退回来的，因此显式置回首屏）。
            Some(_) if !collision.is_empty() => {
                inner.scope = QueryScope::Home;
                inner.plugin_scopes.clear();
            }
            Some((manifest, scope)) => {
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
                }
                // 同一插件用**另一个别名**再次进入（例如把输入从「备忘录」改成「memo」）时
                // 不记录新的历史，但必须更新记下的关键词并换上新的范围对象——范围标签与
                // 「剥掉关键词前缀」都以它对依据，否则会显示旧别名、也搜不到东西。
                inner
                    .plugin_scopes
                    .insert(manifest.id.clone(), Arc::from(scope));
                inner.scope = QueryScope::Plugin {
                    id: manifest.id.clone(),
                    keyword: normalized.clone(),
                };
            }
            None if !inner.scope.is_home() && normalized.is_empty() => {
                // 清空输入即离开插件范围，回到首屏。
                inner.scope = QueryScope::Home;
                inner.plugin_scopes.clear();
            }
            None => {}
        }

        inner.input = input.to_string();
        let ctx = SearchContext::new(input, inner.scope.clone(), 50);

        let (mut ranked, failures) = match inner.scope.clone() {
            QueryScope::Home => self.search_home(inner, &ctx),
            QueryScope::Plugin { id, .. } => self.search_plugin_scope(inner, &ctx, &id),
        };
        if let (Some(manifest), Some(keyword)) = (keyword_entry, collision_keyword.as_ref()) {
            // 插件入口排在最前（匹配层级相同，相关度最高）：直接回车仍然进入插件范围，
            // 往下选择则可以使用标签命中的备忘录。
            ranked.push(RankedItem {
                item: plugin_entry_item(&manifest, keyword),
                source_order: 0,
                source_priority: 0,
            });
        }
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

    /// 首屏上与 `keyword` **标签精确相等**的候选（用于关键词/标签冲突判定）。
    ///
    /// 判定依据是宿主定义的结果模型，而不是插件的内部实现：任何参与首屏搜索的插件
    /// 贡献出的 `Memo` 条目，只要匹配层级是「关键词或标签精确」，就算一次标签命中。
    /// 插件搜索仍然走「独立线程 + 超时 + panic 隔离」，冲突判定不会把宿主拖住。
    fn home_tag_hits(&self, inner: &HostInner, keyword: &str) -> Vec<SearchItem> {
        let ctx = SearchContext::new(keyword, QueryScope::Home, 50);
        self.deps
            .plugins
            .search_home(&ctx, inner.settings.plugin_timeout())
            .results
            .into_iter()
            .flat_map(|(_source, items)| items)
            .filter(|item| {
                item.kind == ItemKind::Memo && item.score.tier == MatchTier::KeywordOrTagExact
            })
            .collect()
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
        let Some(scope) = inner.plugin_scopes.get(plugin_id).cloned() else {
            return (Vec::new(), Vec::new());
        };
        // 范围搜索与首屏搜索共用同一条隔离边界：独立线程 + 超时 + panic 捕获。
        match self
            .deps
            .plugins
            .search_scope(plugin_id, scope, ctx, inner.settings.plugin_timeout())
        {
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
            Err(failure) => (Vec::new(), vec![failure]),
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

/// 首屏「插件入口」条目的标识前缀：`flashcast.plugin.<插件 id>`。
pub const PLUGIN_ENTRY_PREFIX: &str = "flashcast.plugin.";

/// 关键词与标签冲突时，首屏给出的插件入口条目（ADR §4）。
///
/// 相关度给到最高、来源优先级为宿主自身，因此它排在标签命中的备忘录之前：用户按输入
/// 关键词时的第一反应（回车进入插件）保持不变，同时标签命中的备忘录就在下面，不会被
/// 静默丢弃。
pub fn plugin_entry_item(manifest: &crate::plugin::PluginManifest, keyword: &str) -> SearchItem {
    SearchItem {
        id: format!("{PLUGIN_ENTRY_PREFIX}{}", manifest.id),
        title: manifest.name.clone(),
        subtitle: Some(format!("插件 · 回车进入「{keyword}」范围")),
        icon: None,
        source: HOST_SOURCE.to_string(),
        kind: ItemKind::Command,
        default_action: DefaultAction::Open,
        preview: Preview::None,
        score: Score::new(MatchTier::KeywordOrTagExact, u8::MAX),
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

/// 校验备忘录标题。标题是识别条目的主要依据，不允许为空。
fn validate_title(title: &str) -> Result<String, MemoError> {
    let title = title.trim();
    if title.is_empty() {
        return Err(MemoError::Invalid("备忘录标题不能为空".to_string()));
    }
    if title.chars().count() > 200 {
        return Err(MemoError::Invalid(
            "备忘录标题过长（最多 200 个字符）".to_string(),
        ));
    }
    Ok(title.to_string())
}

/// 校验并规范化正文。
fn validate_body(body: &str) -> Result<String, MemoError> {
    if body.trim().is_empty() {
        return Err(MemoError::Invalid("备忘录正文不能为空".to_string()));
    }
    Ok(body.trim_end().to_string())
}

/// 规范化标签：去空白、去空项、去重（保持顺序）。
fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized: Vec<String> = Vec::new();
    for tag in tags {
        let tag = tag.trim();
        if tag.is_empty() || normalized.iter().any(|existing| existing == tag) {
            continue;
        }
        normalized.push(tag.to_string());
    }
    normalized
}
