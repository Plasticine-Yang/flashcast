//! 确定性测试替身。**本模块是仓库中唯一允许存放平台替身的位置**，由 feature `fake`
//! 提供，只有 `flashcast-core` 的 dev-dependencies 会启用它。
//!
//! 替身通过的检查不能证明平台适配通过：真实平台行为必须由各平台的真实检查覆盖。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};
use crate::catalog::{AppCatalog, AppEntry, CatalogError};
use crate::chrome::{
    discover_from_paths, BinaryCandidate, ChromeEnvironment, ChromeError, ChromeLaunch,
    ChromeLaunchRequest, ChromeProvider, UserDataCandidate,
};
use crate::clipboard::{ClipboardAccess, ClipboardError};
use crate::focus::{FocusError, FocusTracker, FocusedApp};
use crate::hotkey::HotkeySpec;
use crate::launch::{AppLauncher, LaunchError, LaunchReceipt};
use crate::launch_request::LaunchRequest;
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 可控的软件目录替身。`scan` 依次返回预先设定的结果序列，
/// 用尽后重复最后一个结果，便于验证 `rescan` 确实重新读取了目录。
#[derive(Default)]
pub struct FakeAppCatalog {
    scans: Mutex<Vec<Result<Vec<AppEntry>, CatalogError>>>,
    call_count: AtomicUsize,
}

impl FakeAppCatalog {
    /// 每次都返回同一批软件。
    pub fn with_apps(apps: Vec<AppEntry>) -> Self {
        Self {
            scans: Mutex::new(vec![Ok(apps)]),
            call_count: AtomicUsize::new(0),
        }
    }

    /// 每次扫描返回固定的错误。
    pub fn failing(error: CatalogError) -> Self {
        Self {
            scans: Mutex::new(vec![Err(error)]),
            call_count: AtomicUsize::new(0),
        }
    }

    /// 按顺序返回多次扫描结果；index 超出后重复最后一个。
    pub fn with_scan_results(results: Vec<Result<Vec<AppEntry>, CatalogError>>) -> Self {
        assert!(!results.is_empty(), "至少需要一次扫描结果");
        Self {
            scans: Mutex::new(results),
            call_count: AtomicUsize::new(0),
        }
    }

    pub fn scan_count(&self) -> usize {
        self.call_count.load(Ordering::SeqCst)
    }
}

impl AppCatalog for FakeAppCatalog {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError> {
        let index = self.call_count.fetch_add(1, Ordering::SeqCst);
        let scans = lock(&self.scans);
        let index = index.min(scans.len() - 1);
        scans[index].clone()
    }
}

/// 记录启动请求的替身启动器。
#[derive(Default)]
pub struct FakeLauncher {
    requests: Mutex<Vec<LaunchRequest>>,
    receipts: Mutex<Vec<Result<LaunchReceipt, LaunchError>>>,
}

impl FakeLauncher {
    /// 所有启动都成功。
    pub fn always_succeeds() -> Self {
        Self::default()
    }

    /// 所有启动都返回同一个错误。
    pub fn always_fails(error: LaunchError) -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            receipts: Mutex::new(vec![Err(error)]),
        }
    }

    /// 按顺序返回结果；index 超出后重复最后一个。
    pub fn with_results(results: Vec<Result<LaunchReceipt, LaunchError>>) -> Self {
        assert!(!results.is_empty(), "至少需要一次启动结果");
        Self {
            requests: Mutex::new(Vec::new()),
            receipts: Mutex::new(results),
        }
    }

    /// 已收到的启动请求，按顺序。
    pub fn requests(&self) -> Vec<LaunchRequest> {
        lock(&self.requests).clone()
    }

    pub fn launch_count(&self) -> usize {
        lock(&self.requests).len()
    }
}

impl AppLauncher for FakeLauncher {
    fn launch(&self, request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError> {
        let index = {
            let mut requests = lock(&self.requests);
            requests.push(request.clone());
            requests.len() - 1
        };
        let receipts = lock(&self.receipts);
        if receipts.is_empty() {
            return Ok(LaunchReceipt {
                pid: Some(4242),
                argv: request.argv(),
            });
        }
        let index = index.min(receipts.len() - 1);
        receipts[index].clone()
    }
}

/// 可控的焦点追踪替身。
#[derive(Default)]
pub struct FakeFocusTracker {
    active: Mutex<Option<FocusedApp>>,
    error: Mutex<Option<FocusError>>,
    restored: Mutex<Vec<FocusedApp>>,
}

impl FakeFocusTracker {
    /// 当前前台应用为 `app`。
    pub fn with_active(app: FocusedApp) -> Self {
        Self {
            active: Mutex::new(Some(app)),
            error: Mutex::new(None),
            restored: Mutex::new(Vec::new()),
        }
    }

    /// 模拟「当前会话不支持读取焦点」的情形（例如 Wayland）。
    pub fn unsupported(reason: &str) -> Self {
        Self {
            active: Mutex::new(None),
            error: Mutex::new(Some(FocusError::Unsupported {
                reason: reason.to_string(),
            })),
            restored: Mutex::new(Vec::new()),
        }
    }

    pub fn set_active(&self, app: Option<FocusedApp>) {
        *lock(&self.active) = app;
    }

    pub fn restored(&self) -> Vec<FocusedApp> {
        lock(&self.restored).clone()
    }
}

impl FocusTracker for FakeFocusTracker {
    fn capture(&self) -> Result<FocusedApp, FocusError> {
        if let Some(error) = lock(&self.error).clone() {
            return Err(error);
        }
        lock(&self.active).clone().ok_or(FocusError::NoActiveWindow)
    }

    fn restore(&self, app: &FocusedApp) -> Result<(), FocusError> {
        if let Some(error) = lock(&self.error).clone() {
            return Err(error);
        }
        lock(&self.restored).push(app.clone());
        Ok(())
    }
}

/// 记录写入内容的剪贴板替身。
///
/// `failures` 非空时按顺序返回失败（用尽后重复最后一个），用于验证「复制失败必须给出
/// 准确反馈」；默认总是成功。
#[derive(Default)]
pub struct FakeClipboard {
    writes: Mutex<Vec<String>>,
    failures: Mutex<Vec<ClipboardError>>,
}

impl FakeClipboard {
    /// 总是成功。
    pub fn new() -> Self {
        Self::default()
    }

    /// 总是返回同一个失败原因。
    pub fn always_fails(error: ClipboardError) -> Self {
        Self {
            writes: Mutex::new(Vec::new()),
            failures: Mutex::new(vec![error]),
        }
    }

    /// 已写入的文本，按顺序。
    pub fn writes(&self) -> Vec<String> {
        lock(&self.writes).clone()
    }

    /// 最近一次写入的文本。
    pub fn last_write(&self) -> Option<String> {
        lock(&self.writes).last().cloned()
    }

    pub fn write_count(&self) -> usize {
        lock(&self.writes).len()
    }
}

impl ClipboardAccess for FakeClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        crate::clipboard::check_text(text)?;
        let failures = lock(&self.failures);
        if !failures.is_empty() {
            let index = lock(&self.writes).len().min(failures.len() - 1);
            return Err(failures[index].clone());
        }
        drop(failures);
        lock(&self.writes).push(text.to_string());
        Ok(())
    }
}

/// 可控的全局快捷键替身。
///
/// `taken` 中的规格视为已被其他应用占用，注册时返回
/// [`HotkeyError::Conflict`]，用于验证 UI 能展示冲突反馈。
#[derive(Default)]
pub struct FakeHotkeyManager {
    taken: Mutex<Vec<String>>,
    registered: Mutex<Vec<HotkeyHandle>>,
    unsupported: Mutex<Option<String>>,
    next_id: AtomicUsize,
}

impl FakeHotkeyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 模拟会话不支持全局快捷键（例如 Wayland）。
    pub fn unsupported(reason: &str) -> Self {
        Self {
            unsupported: Mutex::new(Some(reason.to_string())),
            ..Self::default()
        }
    }

    /// 标记某个规格已被其他应用占用。
    pub fn mark_taken(&self, canonical: &str) {
        lock(&self.taken).push(canonical.to_string());
    }

    /// 当前已注册的句柄。
    pub fn registered(&self) -> Vec<HotkeyHandle> {
        lock(&self.registered).clone()
    }

    /// 触发一个已注册快捷键的回调，模拟用户按下。
    pub fn press(&self, id: u64) -> bool {
        if let Some(handle) = lock(&self.registered).iter().find(|h| h.id == id) {
            handle.fire();
            return true;
        }
        false
    }

    /// 触发全部已注册快捷键的回调。
    pub fn press_all(&self) {
        for handle in lock(&self.registered).clone() {
            handle.fire();
        }
    }
}

impl HotkeyManager for FakeHotkeyManager {
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        if let Some(reason) = lock(&self.unsupported).clone() {
            return Err(HotkeyError::BackendUnavailable { reason });
        }
        let canonical = spec.canonical();
        if lock(&self.taken).contains(&canonical) {
            return Err(HotkeyError::Conflict { spec: canonical });
        }
        let mut registered = lock(&self.registered);
        if registered.iter().any(|h| h.spec.canonical() == canonical) {
            return Err(HotkeyError::AlreadyRegistered { spec: canonical });
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst) as u64 + 1;
        let handle = HotkeyHandle::new(id, spec.clone(), on_press);
        registered.push(handle.clone());
        Ok(handle)
    }

    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        self.unregister(handle)?;
        match self.register(spec, handle.callback.clone()) {
            Ok(new_handle) => Ok(new_handle),
            Err(error) => {
                // 更新失败时恢复原快捷键，避免用户失去入口。
                let _ = self.register(&handle.spec, handle.callback.clone());
                Err(error)
            }
        }
    }

    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        lock(&self.registered).retain(|h| h.id != handle.id);
        Ok(())
    }
}

/// 固定返回同一份能力快照的替身。
pub struct FakeCapabilityProbe {
    capabilities: Capabilities,
}

impl FakeCapabilityProbe {
    pub fn new(capabilities: Capabilities) -> Self {
        Self { capabilities }
    }

    /// 一个常见的 Linux + X11 快照。
    pub fn linux_x11() -> Self {
        Self::new(Capabilities {
            os: OsKind::Linux,
            os_version: Some("Fake Linux".to_string()),
            arch: "x86_64".to_string(),
            session: SessionType::X11,
            desktop_available: true,
            hotkey: Support::Supported,
            clipboard: Support::Supported,
            auto_paste: Support::Supported,
            notes: Vec::new(),
        })
    }

    /// 一个 Linux + Wayland 快照：读取焦点与全局快捷键均不可用。
    pub fn linux_wayland() -> Self {
        let reason = "Wayland 会话不提供全局焦点与快捷键抓取".to_string();
        Self::new(Capabilities {
            os: OsKind::Linux,
            os_version: Some("Fake Linux".to_string()),
            arch: "x86_64".to_string(),
            session: SessionType::Wayland,
            desktop_available: true,
            hotkey: Support::Unsupported {
                reason: reason.clone(),
            },
            clipboard: Support::Supported,
            auto_paste: Support::Unsupported {
                reason: reason.clone(),
            },
            notes: vec![reason],
        })
    }
}

impl CapabilityProbe for FakeCapabilityProbe {
    fn probe(&self) -> Capabilities {
        self.capabilities.clone()
    }
}

/// 一个可用的示例软件条目，供替身与测试复用。
pub fn sample_app(id: &str, name: &str) -> AppEntry {
    AppEntry {
        id: id.to_string(),
        name: name.to_string(),
        comment: None,
        icon: None,
        exec: vec![format!("/usr/bin/{id}")],
        desktop_file: None,
        working_dir: None,
        wm_class: None,
        terminal: false,
        keywords: Vec::new(),
        source: crate::catalog::AppSource::Desktop,
    }
}

/// 便捷类型别名：共享的替身目录。
pub type SharedCatalog = Arc<FakeAppCatalog>;

/// 记录启动请求的 Chrome 替身。
///
/// 关键点：**发现**走的是真实的 [`discover_from_paths`]（真实的存在性检查与真实的
/// `Local State` 解析），只有**启动**被替换成记录 argv。因此宿主集成测试可以用临时夹具
/// 目录走完真实发现代码，同时精确断言交给 Chrome 的参数向量，而不会真的启动浏览器。
pub struct FakeChrome {
    discovery: FakeChromeDiscovery,
    launches: Mutex<Vec<ChromeLaunchRequest>>,
    launch_error: Mutex<Option<ChromeError>>,
    discovery_count: AtomicUsize,
}

/// 替身的发现来源。
enum FakeChromeDiscovery {
    /// 每次 `discover` 都按这些候选做**真实**发现：文件在夹具里变化后能被看见，
    /// 与真实适配器的「每次调用都重新发现」一致。
    Candidates {
        binaries: Vec<BinaryCandidate>,
        user_data: Vec<UserDataCandidate>,
    },
    /// 固定的发现结果（「Chrome 未安装」、profile 不可读等场景）。
    Fixed(Box<Result<ChromeEnvironment, ChromeError>>),
}

impl FakeChrome {
    /// 用给定的候选路径做真实发现。
    pub fn from_candidates(
        binaries: Vec<BinaryCandidate>,
        user_data: Vec<UserDataCandidate>,
    ) -> Self {
        Self {
            discovery: FakeChromeDiscovery::Candidates {
                binaries,
                user_data,
            },
            launches: Mutex::new(Vec::new()),
            launch_error: Mutex::new(None),
            discovery_count: AtomicUsize::new(0),
        }
    }

    /// 直接给出发现结果（例如「Chrome 未安装」或 profile 不可读的场景）。
    pub fn with_environment(environment: Result<ChromeEnvironment, ChromeError>) -> Self {
        Self {
            discovery: FakeChromeDiscovery::Fixed(Box::new(environment)),
            launches: Mutex::new(Vec::new()),
            launch_error: Mutex::new(None),
            discovery_count: AtomicUsize::new(0),
        }
    }

    /// 模拟「没有找到 Chrome 可执行文件」。
    pub fn not_installed(reason: &str) -> Self {
        Self::with_environment(Err(ChromeError::NotInstalled {
            searched: reason.to_string(),
        }))
    }

    /// 让之后的启动都返回同一个错误。
    pub fn always_fails_launch(self, error: ChromeError) -> Self {
        *lock(&self.launch_error) = Some(error);
        self
    }

    /// 已收到的启动请求，按顺序。
    pub fn launches(&self) -> Vec<ChromeLaunchRequest> {
        lock(&self.launches).clone()
    }

    /// 最近一次启动请求。
    pub fn last_launch(&self) -> Option<ChromeLaunchRequest> {
        lock(&self.launches).last().cloned()
    }

    pub fn launch_count(&self) -> usize {
        lock(&self.launches).len()
    }

    pub fn discovery_count(&self) -> usize {
        self.discovery_count.load(Ordering::SeqCst)
    }
}

impl ChromeProvider for FakeChrome {
    fn discover(&self) -> Result<ChromeEnvironment, ChromeError> {
        self.discovery_count.fetch_add(1, Ordering::SeqCst);
        match &self.discovery {
            FakeChromeDiscovery::Candidates {
                binaries,
                user_data,
            } => discover_from_paths(binaries, user_data),
            FakeChromeDiscovery::Fixed(result) => (**result).clone(),
        }
    }

    fn launch(&self, request: &ChromeLaunchRequest) -> Result<ChromeLaunch, ChromeError> {
        if let Some(error) = lock(&self.launch_error).clone() {
            return Err(error);
        }
        lock(&self.launches).push(request.clone());
        Ok(ChromeLaunch {
            pid: Some(4321),
            argv: request.argv(),
        })
    }
}
