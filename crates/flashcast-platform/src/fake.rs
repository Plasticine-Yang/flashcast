//! 确定性测试替身。**本模块是仓库中唯一允许存放平台替身的位置**，由 feature `fake`
//! 提供，只有 `flashcast-core` 的 dev-dependencies 会启用它。
//!
//! 替身通过的检查不能证明平台适配通过：真实平台行为必须由各平台的真实检查覆盖。

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};
use crate::catalog::{AppCatalog, AppEntry, CatalogError};
use crate::chrome::{
    discover_from_paths, BinaryCandidate, ChromeEnvironment, ChromeError, ChromeLaunch,
    ChromeLaunchRequest, ChromeProvider, UserDataCandidate,
};
use crate::clipboard::{
    ClipboardAccess, ClipboardCapture, ClipboardContent, ClipboardError, ClipboardFileEntry,
    ClipboardFormatKind, ClipboardPoll, ClipboardSourceApp, ClipboardWatcher,
    ClipboardWriteReport,
};
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
    /// 每次注入时剪贴板里的**文件列表**（ticket 12）。
    files_at_paste: Mutex<Vec<Option<Vec<PathBuf>>>>,
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
            files_at_paste: Mutex::new(Vec::new()),
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

    /// 每次注入时剪贴板里的文件列表（`None` 表示当时剪贴板没有文件列表）。
    pub fn files_at_paste(&self) -> Vec<Option<Vec<PathBuf>>> {
        lock(&self.files_at_paste).clone()
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
        let (text, files) = match clipboard {
            Some(clipboard) => {
                let files = clipboard.last_write_files();
                let text = if files.is_some() {
                    None
                } else {
                    clipboard.last_write()
                };
                (text, files)
            }
            None => (None, None),
        };
        *lock(&self.pastes) += 1;
        lock(&self.text_at_paste).push(text);
        lock(&self.files_at_paste).push(files);
        Ok(())
    }
}

/// 记录写入内容的剪贴板替身。
///
/// `failures` 非空时按顺序返回失败（用尽后重复最后一个），用于验证「复制失败必须给出
/// 准确反馈」；默认总是成功。`read_text` 返回最后一次写入的内容，因此它同时充当
/// 「系统剪贴板里现在是什么」的可观察视图（ticket 09）。
///
/// 富文本（ticket 11）：每次写入的**完整内容**（文本 + HTML + RTF）都被记下来，
/// 测试因此能断言「恢复时到底把哪些格式放进了剪贴板」，而不是只看「调用过恢复」。
/// `rich_unsupported` 模拟只能提供纯文本的平台（Linux 的 `wl-copy`、macOS 的 `pbcopy`）：
/// 此时 `write_content` 只写文本，并按给定的原因如实报告其它格式被跳过。
#[derive(Default)]
pub struct FakeClipboard {
    writes: Mutex<Vec<String>>,
    /// 按顺序记录的**文件列表**写入（ticket 12）。
    file_writes: Mutex<Vec<Vec<PathBuf>>>,
    contents: Mutex<Vec<ClipboardContent>>,
    reports: Mutex<Vec<ClipboardWriteReport>>,
    failures: Mutex<Vec<ClipboardError>>,
    read_failures: Mutex<Vec<ClipboardError>>,
    rich_unsupported: Mutex<Option<String>>,
}

impl FakeClipboard {
    /// 总是成功。
    pub fn new() -> Self {
        Self::default()
    }

    /// 总是返回同一个失败原因。
    pub fn always_fails(error: ClipboardError) -> Self {
        Self {
            failures: Mutex::new(vec![error]),
            ..Self::default()
        }
    }

    /// 读取总是返回同一个失败原因（写入仍然可用）。
    pub fn fails_read(error: ClipboardError) -> Self {
        Self {
            read_failures: Mutex::new(vec![error]),
            ..Self::default()
        }
    }

    /// 只能提供纯文本的平台样子：富文本格式如实报告为未提供。
    pub fn text_only(reason: &str) -> Self {
        Self {
            rich_unsupported: Mutex::new(Some(reason.to_string())),
            ..Self::default()
        }
    }

    /// 直接设置当前剪贴板内容，模拟「外部应用写入了剪贴板」。
    pub fn set_text(&self, text: impl Into<String>) {
        lock(&self.writes).push(text.into());
    }

    /// 直接设置当前剪贴板里的文件列表，模拟「外部应用复制了文件」。
    pub fn set_files(&self, paths: &[PathBuf]) {
        lock(&self.file_writes).push(paths.to_vec());
    }

    /// 已写入的文本，按顺序。
    pub fn writes(&self) -> Vec<String> {
        lock(&self.writes).clone()
    }

    /// 已写入的完整内容（文本 + 富文本），按顺序。
    pub fn contents(&self) -> Vec<ClipboardContent> {
        lock(&self.contents).clone()
    }

    /// 最近一次写入的完整内容。
    pub fn last_content(&self) -> Option<ClipboardContent> {
        lock(&self.contents).last().cloned()
    }

    /// 每次写入返回的报告，按顺序。
    pub fn reports(&self) -> Vec<ClipboardWriteReport> {
        lock(&self.reports).clone()
    }

    /// 最近一次写入返回的报告。
    pub fn last_report(&self) -> Option<ClipboardWriteReport> {
        lock(&self.reports).last().cloned()
    }

    /// 最近一次写入的文本。
    pub fn last_write(&self) -> Option<String> {
        lock(&self.writes).last().cloned()
    }

    pub fn write_count(&self) -> usize {
        lock(&self.writes).len()
    }

    /// 已写入的文件列表，按顺序。
    pub fn file_writes(&self) -> Vec<Vec<PathBuf>> {
        lock(&self.file_writes).clone()
    }

    /// 最近一次写入的文件列表。
    pub fn last_write_files(&self) -> Option<Vec<PathBuf>> {
        lock(&self.file_writes).last().cloned()
    }
}

impl ClipboardAccess for FakeClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        self.write_content(&ClipboardContent::text(text))
            .map(|_| ())
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let failures = lock(&self.read_failures);
        if !failures.is_empty() {
            let index = lock(&self.writes).len().min(failures.len() - 1);
            return Err(failures[index].clone());
        }
        drop(failures);
        Ok(lock(&self.writes).last().cloned())
    }

    fn write_files(&self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        crate::clipboard::check_files(paths)?;
        // 与文本共用同一份失败队列：宿主对「写入失败」的处理路径只有一条。
        let failures = lock(&self.failures);
        if !failures.is_empty() {
            let index = lock(&self.file_writes)
                .len()
                .min(failures.len().saturating_sub(1));
            return Err(failures[index].clone());
        }
        drop(failures);
        lock(&self.file_writes).push(paths.to_vec());
        Ok(())
    }

    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        let failures = lock(&self.read_failures);
        if !failures.is_empty() {
            let index = lock(&self.file_writes)
                .len()
                .min(failures.len().saturating_sub(1));
            return Err(failures[index].clone());
        }
        drop(failures);
        Ok(lock(&self.file_writes).last().cloned())
    }

    fn write_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, ClipboardError> {
        crate::clipboard::check_text(&content.text)?;
        let failures = lock(&self.failures);
        if !failures.is_empty() {
            let index = lock(&self.writes).len().min(failures.len() - 1);
            return Err(failures[index].clone());
        }
        drop(failures);
        let report = match lock(&self.rich_unsupported).clone() {
            Some(reason) => ClipboardWriteReport::text_only(
                &reason,
                content
                    .requested_formats()
                    .into_iter()
                    .filter(|kind| *kind != ClipboardFormatKind::Text),
            ),
            None => ClipboardWriteReport {
                formats: content.requested_formats(),
                skipped: Vec::new(),
            },
        };
        lock(&self.writes).push(content.text.clone());
        lock(&self.contents).push(content.clone());
        lock(&self.reports).push(report.clone());
        Ok(report)
    }
}

/// 可控的剪贴板变化监听替身（ticket 09）。
///
/// 语义与真实适配层一致，但完全确定：`set_text` 模拟一次**外部**复制（序号自增，
/// 下一次 `poll` 报告一次变化，再下一次回到「没有变化」），`note_own_write` 模拟
/// Flashcast 自己的写入（序号自增，但下一次 `poll` 抑制掉，不报告变化）。
///
/// 「同一段文字被复制两次」会报告两次变化——真实适配层用内容指纹判断，做不到这一点，
/// 因此**去重**这条行为由宿主的 `content_hash` 保证，并用这个替身覆盖。
pub struct FakeClipboardWatcher {
    state: Mutex<FakeWatcherState>,
    polls: AtomicUsize,
    captures: AtomicUsize,
    own_writes: AtomicUsize,
}

struct FakeWatcherState {
    text: Option<String>,
    /// 当前剪贴板里的文件列表（ticket 12）；非空时优先按文件报告。
    files: Vec<ClipboardFileEntry>,
    /// 同一次复制事件里的 HTML / RTF 载荷（ticket 11）。
    html: Option<String>,
    rtf: Option<String>,
    formats: Vec<ClipboardFormatKind>,
    source: Option<ClipboardSourceApp>,
    /// 外部写入序号：每次写入（含自身写入）自增。
    sequence: u64,
    /// 已经交付或抑制到的序号。
    delivered: u64,
    /// 自身写入的**文本**内容指纹，`poll` 见到就抑制。
    own: Vec<u64>,
    /// 自身写入的**文件列表**内容指纹，`poll` 见到就抑制。
    own_files: Vec<u64>,
    /// 是否在适配层抑制自身写入。置为 `false` 用于验证**宿主自己的兜底抑制**
    /// 独立成立（真实适配层失效时也不能形成自身写入循环）。
    suppress_own: bool,
    error: Option<ClipboardError>,
}

impl Default for FakeClipboardWatcher {
    fn default() -> Self {
        Self {
            state: Mutex::new(FakeWatcherState {
                text: None,
                files: Vec::new(),
                html: None,
                rtf: None,
                formats: Vec::new(),
                source: None,
                sequence: 0,
                delivered: 0,
                own: Vec::new(),
                own_files: Vec::new(),
                suppress_own: true,
                error: None,
            }),
            polls: AtomicUsize::new(0),
            captures: AtomicUsize::new(0),
            own_writes: AtomicUsize::new(0),
        }
    }
}

impl FakeClipboardWatcher {
    /// 空剪贴板。
    pub fn new() -> Self {
        Self::default()
    }

    /// 剪贴板初始就有内容（第一次 `poll` 会报告一次变化）。
    pub fn with_text(text: impl Into<String>) -> Self {
        let watcher = Self::default();
        watcher.set_text(text);
        watcher
    }

    /// 每次 `poll` 都返回同一个读取失败原因（模拟「拿不到剪贴板选区」）。
    pub fn failing(error: ClipboardError) -> Self {
        let watcher = Self::default();
        lock(&watcher.state).error = Some(error);
        watcher
    }

    /// 模拟一次外部复制：序号自增，下一次 `poll` 报告变化。
    pub fn set_text(&self, text: impl Into<String>) {
        let mut state = lock(&self.state);
        state.text = Some(text.into());
        state.files = Vec::new();
        // 纯文本复制没有富文本格式：残留的载荷必须被清掉，否则会把上一次的内容带进来。
        state.html = None;
        state.rtf = None;
        state.formats = vec![ClipboardFormatKind::Text];
        state.sequence += 1;
    }

    /// 模拟一次外部**文件列表**复制（ticket 12）：序号自增，下一次 `poll` 报告变化。
    ///
    /// 模拟真实平台的行为：文件列表在剪贴板里是文件格式，不是一段文字。
    pub fn set_files(&self, paths: &[PathBuf]) {
        let mut state = lock(&self.state);
        state.text = None;
        state.files = crate::clipboard::file_entries(paths);
        state.formats = vec![ClipboardFormatKind::Files];
        state.html = None;
        state.rtf = None;
        state.sequence += 1;
    }

    /// 模拟一次携带富文本的复制：文本、HTML、RTF 属于**同一次**事件，下一次 `poll`
    /// 只报告一条捕获（因此只会产生一条历史）。
    pub fn set_rich(&self, text: &str, html: Option<&str>, rtf: Option<&str>) {
        let capture = ClipboardCapture::rich(
            text.to_string(),
            html.map(str::to_string),
            rtf.map(str::to_string),
        );
        let mut state = lock(&self.state);
        state.text = capture.text.clone();
        state.html = capture.html.clone();
        state.rtf = capture.rtf.clone();
        state.formats = capture.formats.clone();
        state.sequence += 1;
    }

    /// 模拟「来源应用」信息（平台能提供时）。
    pub fn set_source(&self, app_id: &str, title: &str) {
        lock(&self.state).source = Some(ClipboardSourceApp {
            app_id: app_id.to_string(),
            title: Some(title.to_string()),
        });
    }

    /// 已轮询次数。
    pub fn poll_count(&self) -> usize {
        self.polls.load(Ordering::SeqCst)
    }

    /// 已报告的变化次数（去重与抑制都发生在这之后）。
    pub fn capture_count(&self) -> usize {
        self.captures.load(Ordering::SeqCst)
    }

    /// 已登记的自身写入次数。
    pub fn own_write_count(&self) -> usize {
        self.own_writes.load(Ordering::SeqCst)
    }

    /// 让适配层**不再**抑制自身写入。
    ///
    /// 用于验证宿主侧的兜底抑制独立成立：即使适配层把自身写入报告成一次变化，
    /// 宿主也必须按内容指纹丢弃它，不能形成「粘贴 → 捕获 → 再粘贴」的循环。
    pub fn ignore_own_writes(&self) {
        lock(&self.state).suppress_own = false;
    }
}

impl ClipboardWatcher for FakeClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        let mut state = lock(&self.state);
        if let Some(error) = state.error.clone() {
            return Err(error);
        }
        if state.sequence == state.delivered {
            return Ok(ClipboardPoll::Unchanged);
        }
        state.delivered = state.sequence;
        // 文件列表优先（与真实适配层一致）：文件格式存在时就不是一次文字复制。
        if !state.files.is_empty() {
            let paths: Vec<PathBuf> = state.files.iter().map(|file| file.path.clone()).collect();
            let print = crate::clipboard::fingerprint_files(&paths);
            if state.suppress_own {
                if let Some(index) = state.own_files.iter().position(|item| *item == print) {
                    state.own_files.remove(index);
                    return Ok(ClipboardPoll::Unchanged);
                }
            }
            self.captures.fetch_add(1, Ordering::SeqCst);
            return Ok(ClipboardPoll::Changed(ClipboardCapture {
                formats: state.formats.clone(),
                text: None,
                files: state.files.clone(),
                html: None,
                rtf: None,
                source: state.source.clone(),
            }));
        }
        let text = state.text.clone();
        let Some(text) = text else {
            return Ok(ClipboardPoll::Unchanged);
        };
        let print = crate::clipboard::fingerprint(&text);
        if state.suppress_own {
            if let Some(index) = state.own.iter().position(|item| *item == print) {
                state.own.remove(index);
                return Ok(ClipboardPoll::Unchanged);
            }
        }
        self.captures.fetch_add(1, Ordering::SeqCst);
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats: state.formats.clone(),
            text: Some(text),
            files: Vec::new(),
            html: state.html.clone(),
            rtf: state.rtf.clone(),
            source: state.source.clone(),
        }))
    }

    fn note_own_write(&self, text: &str) {
        self.own_writes.fetch_add(1, Ordering::SeqCst);
        let mut state = lock(&self.state);
        if !text.is_empty() {
            state.own.push(crate::clipboard::fingerprint(text));
        }
        // 自身写入同样改变剪贴板（序号自增），只是不会被报告成复制事件。恢复一条历史时
        // 宿主写入的是「文本 + 富文本」，适配层的抑制以文本指纹为准（真实适配层同理：
        // 序号或文本指纹），因此自身写入不会形成循环。
        state.text = Some(text.to_string());
        state.files = Vec::new();
        state.html = None;
        state.rtf = None;
        state.sequence += 1;
    }

    fn note_own_write_files(&self, paths: &[PathBuf]) {
        self.own_writes.fetch_add(1, Ordering::SeqCst);
        if paths.is_empty() {
            return;
        }
        let mut state = lock(&self.state);
        state
            .own_files
            .push(crate::clipboard::fingerprint_files(paths));
        state.text = None;
        state.files = crate::clipboard::file_entries(paths);
        state.html = None;
        state.rtf = None;
        state.formats = vec![ClipboardFormatKind::Files];
        state.sequence += 1;
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
