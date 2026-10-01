//! Windows 剪贴板读写与变化监听。
//!
//! Windows 没有可靠的命令行剪贴板工具（`clip.exe` 按控制台输入代码页解析标准输入，
//! 非 ASCII 会乱码），因此直接调用 `Win32` 剪贴板 API：`CF_UNICODETEXT` + 全局内存块，
//! 与宿主其它 Windows 适配层一样只做 FFI，不含业务判断。
//!
//! 变化监听用 `GetClipboardSequenceNumber`：系统维护的单调序号，剪贴板每次变化都会自增，
//! 因此可以精确地区分「同一段文字被复制了两次」与「没有变化」。自身写入抑制也建立在
//! 它之上：登记一次自身写入时记下当前序号，`poll` 见到该序号就直接跳过。
//!
//! ## 富文本格式
//!
//! Windows 是唯一三种格式齐全的平台：读写在**一次**剪贴板打开里完成
//! `CF_UNICODETEXT` + `HTML Format`（CF_HTML） + `Rich Text Format`。文本与富文本格式
//! 因此天然属于同一次复制事件，既不会拆成多条历史，恢复时也是三种格式一起交给系统，
//! 由目标应用自己挑。CF_HTML 的头部编解码是纯逻辑，放在 `crate::clipboard` 里由 Linux
//! 上的单元测试覆盖（见那里的 `cf_html_tests`）。
//!
//! 私有格式（应用自定义的格式名）不在保存范围内：Flashcast 只保存并恢复上面这三种
//! 平台公开格式，不声称保留了任意应用的私有格式。
//!
//! 这个模块只在 `cfg(target_os = "windows")` 下编译；本地 Linux 开发机无法执行它，
//! 由 Windows runner 上的真实检查覆盖（见 `flashcast-platform-check`）。

use std::path::PathBuf;
use std::sync::Mutex;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, POINT};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_HDROP, CF_UNICODETEXT};
use windows::Win32::UI::Shell::{DragQueryFileW, DROPFILES, HDROP};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

use crate::clipboard::{
    cf_html_bytes, cf_html_fragment, check_files, check_text, file_entries, ClipboardAccess,
    ClipboardCapture, ClipboardContent, ClipboardError, ClipboardFormatKind, ClipboardPoll,
    ClipboardSkippedFormat, ClipboardSourceApp, ClipboardWatcher, ClipboardWriteReport,
};

/// Windows 上 HTML 载荷的剪贴板格式注册名（CF_HTML）。
const HTML_FORMAT_NAME: &str = "HTML Format";
/// Windows 上 RTF 载荷的剪贴板格式注册名。
const RTF_FORMAT_NAME: &str = "Rich Text Format";

/// 把格式注册名换成格式 id；0 表示注册失败。
fn register_format(name: &str) -> u32 {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    unsafe { RegisterClipboardFormatW(PCWSTR(wide.as_ptr())) }
}

/// Windows 的文本剪贴板后端。
pub struct WindowsClipboard;

impl Default for WindowsClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsClipboard {
    pub fn new() -> Self {
        Self
    }

    /// 在一次剪贴板打开里读出文本 + HTML + RTF。
    ///
    /// **一次复制事件的格式必须一起读**：分三次打开剪贴板读，期间选区可能已经被换掉，
    /// 结果会把两次复制的内容拼成一条历史。三种格式都拿不到（或没有文本）时返回
    /// `Ok(None)`。
    fn read_all(&self) -> Result<Option<(String, Option<String>, Option<String>)>, ClipboardError> {
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = read_all_locked();
            let _ = CloseClipboard();
            result
        }
    }
}

impl ClipboardAccess for WindowsClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        check_text(text)?;
        // 剪贴板是全局资源：打开失败说明被其它进程占用，如实报告而不是静默失败。
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = write_locked(text);
            let _ = CloseClipboard();
            result
        }
    }

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        Ok(self.read_all()?.map(|(text, _, _)| text))
    }

    /// 恢复剪贴板历史：文本 + HTML + RTF **一次全部**交回系统。
    ///
    /// Windows 允许在一次剪贴板打开里登记多个格式：`CF_UNICODETEXT` 给纯文本目标，
    /// `HTML Format`（CF_HTML）与 `Rich Text Format` 给富文本目标。任何富文本格式写入
    /// 失败都不会影响其它格式，失败原因如实进入 [`ClipboardWriteReport`]。
    fn write_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, ClipboardError> {
        check_text(&content.text)?;
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = write_content_locked(content);
            let _ = CloseClipboard();
            result
        }
    }

    fn write_files(&self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        check_files(paths)?;
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = write_files_locked(paths);
            let _ = CloseClipboard();
            result
        }
    }

    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = read_files_locked();
            let _ = CloseClipboard();
            result
        }
    }
}

/// 剪贴板已打开：把一块全局内存交给系统。成功之后内存归系统所有，不能再释放。
unsafe fn set_global_data(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
    if bytes.is_empty() {
        return Err(ClipboardError::Failed(
            "载荷为空，没有可写入的内容".to_string(),
        ));
    }
    let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes.len())
        .map_err(|error| ClipboardError::Failed(format!("无法分配剪贴板内存：{error}")))?;
    let pointer = GlobalLock(handle);
    if pointer.is_null() {
        let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
        return Err(ClipboardError::Failed("无法锁定剪贴板内存".to_string()));
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer as *mut u8, bytes.len());
    let _ = GlobalUnlock(handle);
    match SetClipboardData(format, Some(HANDLE(handle.0))) {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
            Err(ClipboardError::Failed(format!("无法写入剪贴板：{error}")))
        }
    }
}

/// 剪贴板已打开：清空后写入 UTF-16 文本。所有权交给系统，失败路径自己释放。
unsafe fn write_locked(text: &str) -> Result<(), ClipboardError> {
    EmptyClipboard().map_err(|error| ClipboardError::Failed(format!("无法清空剪贴板：{error}")))?;
    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    set_global_data(
        CF_UNICODETEXT.0 as u32,
        std::slice::from_raw_parts(
            utf16.as_ptr() as *const u8,
            std::mem::size_of_val(utf16.as_slice()),
        ),
    )
}

/// 剪贴板已打开：清空后把文本 + HTML + RTF 一起写进系统剪贴板。
///
/// 文本是必须成功的那个（任何目标都能接收）；富文本格式失败不中断，如实记进报告。
unsafe fn write_content_locked(
    content: &ClipboardContent,
) -> Result<ClipboardWriteReport, ClipboardError> {
    EmptyClipboard().map_err(|error| ClipboardError::Failed(format!("无法清空剪贴板：{error}")))?;
    let mut report = ClipboardWriteReport::default();

    let mut utf16: Vec<u16> = content.text.encode_utf16().collect();
    utf16.push(0);
    set_global_data(
        CF_UNICODETEXT.0 as u32,
        std::slice::from_raw_parts(
            utf16.as_ptr() as *const u8,
            std::mem::size_of_val(utf16.as_slice()),
        ),
    )?;
    report.formats.push(ClipboardFormatKind::Text);

    for (kind, name, bytes) in [
        (
            ClipboardFormatKind::Html,
            HTML_FORMAT_NAME,
            content.html.as_deref().map(|html| cf_html_bytes(html)),
        ),
        (
            ClipboardFormatKind::Rtf,
            RTF_FORMAT_NAME,
            content.rtf.as_deref().map(|rtf| rtf.as_bytes().to_vec()),
        ),
    ] {
        let Some(bytes) = bytes else { continue };
        let format = register_format(name);
        if format == 0 {
            report.skipped.push(ClipboardSkippedFormat {
                kind,
                reason: format!("无法注册剪贴板格式 {name}"),
            });
            continue;
        }
        match set_global_data(format, &bytes) {
            Ok(()) => report.formats.push(kind),
            Err(error) => report.skipped.push(ClipboardSkippedFormat {
                kind,
                reason: error.to_string(),
            }),
        }
    }
    Ok(report)
}

/// 剪贴板已打开：读取文本 + HTML + RTF。没有文本格式时返回 `Ok(None)`。
unsafe fn read_all_locked(
) -> Result<Option<(String, Option<String>, Option<String>)>, ClipboardError> {
    let Some(text) = read_utf16_locked()? else {
        // 剪贴板里没有文本（例如只复制了一张图片）：这不是错误。
        return Ok(None);
    };
    let html = read_format_bytes_locked(HTML_FORMAT_NAME)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|payload| cf_html_fragment(&payload));
    let rtf = read_format_bytes_locked(RTF_FORMAT_NAME)
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .filter(|value| !value.trim().is_empty());
    Ok(Some((text, html, rtf)))
}

/// 剪贴板已打开：读取 `CF_UNICODETEXT`。没有文本格式时返回 `Ok(None)`。
unsafe fn read_utf16_locked() -> Result<Option<String>, ClipboardError> {
    let handle = match GetClipboardData(CF_UNICODETEXT.0 as u32) {
        Ok(handle) => handle,
        // 剪贴板里没有文本（例如只复制了一张图片）：这不是错误。
        Err(_) => return Ok(None),
    };
    if handle.is_invalid() {
        return Ok(None);
    }
    let pointer = GlobalLock(HGLOBAL(handle.0)) as *const u16;
    if pointer.is_null() {
        return Err(ClipboardError::Failed("无法锁定剪贴板内存".to_string()));
    }
    // 系统给出的内存块以 NUL 结尾；长度未知，因此边走边找（有硬上限，避免坏数据死循环）。
    let mut length = 0usize;
    const MAX_UNITS: usize = 8 * 1024 * 1024;
    while length < MAX_UNITS && *pointer.add(length) != 0 {
        length += 1;
    }
    let units = std::slice::from_raw_parts(pointer, length);
    let text = String::from_utf16_lossy(units);
    let _ = GlobalUnlock(HGLOBAL(handle.0));
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}

/// 剪贴板已打开：写入 `CF_HDROP`（资源管理器与文件对话框粘贴文件列表的公开格式）。
///
/// 结构是「`DROPFILES` 头部 + 双 NUL 结尾的 UTF-16 路径列表」，`fWide = TRUE` 表示
/// 宽字符。视频文件与普通文件走同一条路径（v0.1.0 不区分视频语义）。
unsafe fn write_files_locked(paths: &[PathBuf]) -> Result<(), ClipboardError> {
    EmptyClipboard().map_err(|error| ClipboardError::Failed(format!("无法清空剪贴板：{error}")))?;

    let mut wide: Vec<u16> = Vec::new();
    for path in paths {
        wide.extend(path.to_string_lossy().encode_utf16());
        wide.push(0);
    }
    // 列表以**双 NUL** 结尾：最后一个条目之后再补一个 0。
    wide.push(0);

    let header = std::mem::size_of::<DROPFILES>();
    let bytes = header + wide.len() * std::mem::size_of::<u16>();
    let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)
        .map_err(|error| ClipboardError::Failed(format!("无法分配剪贴板内存：{error}")))?;
    let pointer = GlobalLock(handle);
    if pointer.is_null() {
        let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
        return Err(ClipboardError::Failed("无法锁定剪贴板内存".to_string()));
    }
    let dropfiles = DROPFILES {
        pFiles: header as u32,
        pt: POINT { x: 0, y: 0 },
        fNC: false.into(),
        fWide: true.into(),
    };
    std::ptr::copy_nonoverlapping(
        &dropfiles as *const DROPFILES as *const u8,
        pointer as *mut u8,
        header,
    );
    std::ptr::copy_nonoverlapping(
        wide.as_ptr() as *const u8,
        (pointer as *mut u8).add(header),
        wide.len() * std::mem::size_of::<u16>(),
    );
    let _ = GlobalUnlock(handle);

    match SetClipboardData(CF_HDROP.0 as u32, Some(HANDLE(handle.0))) {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
            Err(ClipboardError::Failed(format!("无法写入剪贴板：{error}")))
        }
    }
}

/// 剪贴板已打开：读取 `CF_HDROP` 的文件列表。没有文件格式时返回 `Ok(None)`。
unsafe fn read_files_locked() -> Result<Option<Vec<PathBuf>>, ClipboardError> {
    let handle = match GetClipboardData(CF_HDROP.0 as u32) {
        Ok(handle) => handle,
        // 剪贴板里没有文件列表（例如只复制了一段文字）：这不是错误。
        Err(_) => return Ok(None),
    };
    if handle.is_invalid() {
        return Ok(None);
    }
    let drop = HDROP(handle.0);
    // `ifile == 0xFFFF_FFFF` 时返回条目数（不是长度）。
    let count = DragQueryFileW(drop, u32::MAX, None);
    if count == 0 {
        return Ok(None);
    }
    let mut paths = Vec::new();
    for index in 0..count {
        let length = DragQueryFileW(drop, index, None) as usize;
        if length == 0 {
            continue;
        }
        let mut buffer = vec![0u16; length + 1];
        let written = DragQueryFileW(drop, index, Some(&mut buffer)) as usize;
        if written == 0 {
            continue;
        }
        buffer.truncate(written);
        paths.push(PathBuf::from(String::from_utf16_lossy(&buffer)));
    }
    if paths.is_empty() {
        return Ok(None);
    }
    Ok(Some(paths))
}

/// 剪贴板已打开：按注册名读取一种私有格式的原始字节。没有该格式时返回 `None`。
///
/// 先注册（`RegisterClipboardFormatW` 对同一个名字始终返回同一个 id），再取数据；
/// 拿不到、长度为 0 或无法锁定都按「没有这个格式」处理，不猜内容。
unsafe fn read_format_bytes_locked(name: &str) -> Option<Vec<u8>> {
    let format = register_format(name);
    if format == 0 {
        return None;
    }
    let handle = GetClipboardData(format).ok()?;
    if handle.is_invalid() {
        return None;
    }
    let size = GlobalSize(HGLOBAL(handle.0));
    if size == 0 {
        return None;
    }
    let pointer = GlobalLock(HGLOBAL(handle.0)) as *const u8;
    if pointer.is_null() {
        return None;
    }
    let bytes = std::slice::from_raw_parts(pointer, size).to_vec();
    let _ = GlobalUnlock(HGLOBAL(handle.0));
    Some(bytes)
}

/// 剪贴板持有者窗口对应的来源应用。
///
/// `GetClipboardOwner` 在多数情况下就是发起复制的应用的顶层窗口；拿不到进程信息时
/// 如实返回 `None`（来源是尽力而为的信息，缺失不影响捕获）。
unsafe fn source_app() -> Option<ClipboardSourceApp> {
    let owner: HWND = GetClipboardOwner().ok()?;
    if owner.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(owner, Some(&mut pid));
    if pid == 0 {
        return None;
    }
    let path = super::focus::process_image_path(pid)?;
    let executable = std::path::Path::new(&path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| path.clone());
    Some(ClipboardSourceApp {
        app_id: executable,
        title: super::focus::window_title(owner).or_else(|| Some(path)),
    })
}

/// Windows 的剪贴板变化监听。
pub struct WindowsClipboardWatcher {
    /// 上一次报告（或已抑制）的剪贴板序号。
    last: Mutex<u32>,
    /// 自身写入登记：这些序号不会被报告成新的复制事件。
    own: Mutex<Vec<u32>>,
}

impl Default for WindowsClipboardWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsClipboardWatcher {
    pub fn new() -> Self {
        Self {
            // 0 表示「系统不支持序号」，此时退化为每次都读（去重由宿主保证）。
            last: Mutex::new(0),
            own: Mutex::new(Vec::new()),
        }
    }

    /// 登记自身写入时用的当前序号（0 表示拿不到，此时靠宿主的内容指纹兜底）。
    fn sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }
}

impl ClipboardWatcher for WindowsClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        let sequence = Self::sequence();
        if sequence != 0 {
            {
                let mut own = self.own.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(index) = own.iter().position(|item| *item == sequence) {
                    own.remove(index);
                    *self.last.lock().unwrap_or_else(|p| p.into_inner()) = sequence;
                    return Ok(ClipboardPoll::Unchanged);
                }
            }
            let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
            if sequence == *last {
                return Ok(ClipboardPoll::Unchanged);
            }
            *last = sequence;
        }
        let source = unsafe { source_app() };
        // 文件列表优先：资源管理器复制文件时剪贴板里同时有 `CF_HDROP` 与文本
        // （路径本身）。先按文件捕获，才能把「复制文件」与「复制这段文字」分开。
        if let Some(files) = WindowsClipboard::new().read_files()? {
            return Ok(ClipboardPoll::Changed(ClipboardCapture {
                formats: vec![ClipboardFormatKind::Files],
                text: None,
                files: file_entries(&files),
                html: None,
                rtf: None,
                source,
            }));
        }
        let Some((text, html, rtf)) = WindowsClipboard::new().read_all()? else {
            return Ok(ClipboardPoll::Unchanged);
        };
        // 同一次复制的文本与富文本格式一起进同一条捕获，不拆成多条记录。
        let mut capture = ClipboardCapture::rich(text, html, rtf);
        capture.source = source;
        Ok(ClipboardPoll::Changed(capture))
    }

    fn note_own_write(&self, _text: &str) {
        let sequence = Self::sequence();
        if sequence == 0 {
            return;
        }
        let mut own = self.own.lock().unwrap_or_else(|p| p.into_inner());
        if !own.contains(&sequence) {
            own.push(sequence);
        }
    }

    fn note_own_write_files(&self, _paths: &[PathBuf]) {
        // 与文本共用同一份「自身写入序号」登记：Windows 的序号不区分格式，
        // 因此这里不需要另存一份文件指纹。
        self.note_own_write("");
    }
}
