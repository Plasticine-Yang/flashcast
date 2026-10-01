//! 确定性测试替身。**本模块是仓库中唯一允许存放平台替身的位置**，由 feature `fake`
//! 提供，只有 `flashcast-core` 的 dev-dependencies 会启用它。
//!
//! 替身通过的检查不能证明平台适配通过：真实平台行为必须由各平台的真实检查覆盖。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};
use crate::catalog::{AppCatalog, AppEntry, CatalogError};
use crate::clipboard::{ClipboardAccess, ClipboardError};
use crate::focus::{FocusError, FocusTracker, FocusedApp};
use crate::hotkey::HotkeySpec;
use crate::launch::{AppLauncher, LaunchError, LaunchReceipt};
use crate::launch_request::LaunchRequest;
use crate::paste::{PasteError, Paster};
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
    /// 只让 `restore` 失败（例如「唤起前的应用已退出」），`capture` 仍然可用。
    restore_error: Mutex<Option<FocusError>>,
    restored: Mutex<Vec<FocusedApp>>,
}

impl FakeFocusTracker {
    /// 当前前台应用为 `app`。
    pub fn with_active(app: FocusedApp) -> Self {
        Self {
            active: Mutex::new(Some(app)),
            error: Mutex::new(None),
            restore_error: Mutex::new(None),
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
            restore_error: Mutex::new(None),
            restored: Mutex::new(Vec::new()),
        }
    }

    /// 模拟「唤起前的应用已退出」：`capture` 正常，`restore` 失败。
    pub fn restore_fails(error: FocusError) -> Self {
        Self {
            active: Mutex::new(None),
            error: Mutex::new(None),
            restore_error: Mutex::new(Some(error)),
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
        if let Some(error) = lock(&self.restore_error).clone() {
            return Err(error);
        }
        lock(&self.restored).push(app.clone());
        Ok(())
    }
}

/// 记录合成粘贴的替身。
///
/// 除了「注入了几次」，它还记录**注入时剪贴板里是什么**：这是验证「不会粘贴过期选择」
/// 最直接的证据——宿主必须在同一轮执行里先把该条正文写进剪贴板，再注入粘贴。
#[derive(Default)]
pub struct FakePaster {
    pastes: Mutex<usize>,
    text_at_paste: Mutex<Vec<Option<String>>>,
    failures: Mutex<Vec<PasteError>>,
    clipboard: Mutex<Option<Arc<FakeClipboard>>>,
}

impl FakePaster {
    /// 总是成功，但不观察剪贴板。
    pub fn new() -> Self {
        Self::default()
    }

    /// 总是成功，并记录每次注入时的剪贴板内容。
    pub fn observing(clipboard: Arc<FakeClipboard>) -> Self {
        Self {
            pastes: Mutex::new(0),
            text_at_paste: Mutex::new(Vec::new()),
            failures: Mutex::new(Vec::new()),
            clipboard: Mutex::new(Some(clipboard)),
        }
    }

    /// 注入失败（按顺序返回，用尽后重复最后一个）。
    pub fn with_failures(failures: Vec<PasteError>) -> Self {
        assert!(!failures.is_empty(), "至少需要一次粘贴结果");
        Self {
            failures: Mutex::new(failures),
            ..Self::default()
        }
    }

    /// 已注入的次数。
    pub fn paste_count(&self) -> usize {
        *lock(&self.pastes)
    }

    /// 每次注入时剪贴板里的文本（`None` 表示当时剪贴板没有内容）。
    pub fn text_at_paste(&self) -> Vec<Option<String>> {
        lock(&self.text_at_paste).clone()
    }
}

impl Paster for FakePaster {
    fn paste(&self) -> Result<(), PasteError> {
        let failures = lock(&self.failures);
        if !failures.is_empty() {
            let index = (*lock(&self.pastes)).min(failures.len() - 1);
            *lock(&self.pastes) += 1;
            return Err(failures[index].clone());
        }
        drop(failures);
        // 先取到句柄再释放锁，避免与剪贴板替身形成嵌套锁。
        let clipboard = lock(&self.clipboard).clone();
        let text = clipboard.and_then(|clipboard| clipboard.last_write());
        *lock(&self.pastes) += 1;
        lock(&self.text_at_paste).push(text);
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
