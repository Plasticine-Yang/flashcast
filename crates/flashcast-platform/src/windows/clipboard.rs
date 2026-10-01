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
//! 这个模块只在 `cfg(target_os = "windows")` 下编译；本地 Linux 开发机无法执行它，
//! 由 Windows runner 上的真实检查覆盖（见 `flashcast-platform-check`）。

use std::sync::Mutex;

use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardOwner,
    GetClipboardSequenceNumber, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};
use windows::Win32::System::Ole::{CF_DIB, CF_UNICODETEXT};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowThreadProcessId, RegisterClipboardFormatW};

use crate::clipboard::{
    check_image_write, check_text, dib_to_png, fingerprint_bytes, image_from_bytes, png_to_dib,
    ClipboardAccess, ClipboardCapture, ClipboardError, ClipboardFormatKind, ClipboardImage,
    ClipboardPoll, ClipboardSourceApp, ClipboardWatcher, IMAGE_MIME_PNG, MAX_IMAGE_BYTES,
};

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

    /// 读取当前剪贴板里的图片，并把「有图片但无法保存」的原因一并带回。
    ///
    /// 顺序与真实剪贴板一致：先看现代应用普遍提供的注册格式 `PNG`（字节已经是 PNG，
    /// 不需要解码重编码），再回退到 `CF_DIB`（旧应用与 Explorer 常见）。
    pub fn read_image_detailed(
        &self,
    ) -> Result<(Option<ClipboardImage>, Option<String>), ClipboardError> {
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = read_image_locked();
            let _ = CloseClipboard();
            result
        }
    }
}

/// 注册格式 `PNG` 的 id；拿不到时返回 0（此时只写 `CF_DIB`）。
unsafe fn png_format() -> u32 {
    let name: Vec<u16> = "PNG".encode_utf16().chain(Some(0)).collect();
    RegisterClipboardFormatW(windows::core::PCWSTR(name.as_ptr()))
}

/// 剪贴板已打开：读注册格式 `PNG`，失败再读 `CF_DIB`。
unsafe fn read_image_locked() -> Result<(Option<ClipboardImage>, Option<String>), ClipboardError> {
    let format = png_format();
    if format != 0 {
        if let Some(bytes) = clipboard_bytes(format)? {
            let (image, problem) = image_from_bytes(IMAGE_MIME_PNG, Some(bytes));
            if image.is_some() {
                return Ok((image, None));
            }
            if let Some(reason) = problem {
                return Ok((None, Some(reason)));
            }
        }
    }
    match clipboard_bytes(CF_DIB.0 as u32)? {
        None => Ok((None, None)),
        Some(dib) => match dib_to_png(&dib) {
            Ok(png) => Ok(image_from_bytes(IMAGE_MIME_PNG, Some(png))),
            // 位图存在但无法解码：这是「无法恢复的格式」，必须如实上报。
            Err(reason) => Ok((None, Some(format!("位图格式无法恢复：{reason}")))),
        },
    }
}

/// 剪贴板已打开：把某个格式的数据读成字节。没有该格式时返回 `Ok(None)`。
unsafe fn clipboard_bytes(format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
    let handle = match GetClipboardData(format) {
        Ok(handle) => handle,
        Err(_) => return Ok(None),
    };
    if handle.is_invalid() {
        return Ok(None);
    }
    let global = HGLOBAL(handle.0);
    let pointer = GlobalLock(global) as *const u8;
    if pointer.is_null() {
        return Err(ClipboardError::Failed("无法锁定剪贴板内存".to_string()));
    }
    let size = GlobalSize(global);
    if size == 0 {
        let _ = GlobalUnlock(global);
        return Ok(None);
    }
    // 上限之外只多读 1 字节：既不会把巨量数据读进内存，又能让上层区分
    // 「刚好等于上限」与「超过上限」。
    let size = size.min(MAX_IMAGE_BYTES + 1);
    let bytes = std::slice::from_raw_parts(pointer, size).to_vec();
    let _ = GlobalUnlock(global);
    Ok(Some(bytes))
}

/// 剪贴板已打开：清空后写入注册格式 `PNG` 与 `CF_DIB`。
unsafe fn write_image_locked(png: &[u8], format: u32) -> Result<(), ClipboardError> {
    EmptyClipboard().map_err(|error| ClipboardError::Failed(format!("无法清空剪贴板：{error}")))?;
    // 同时提供两种格式：现代应用（浏览器、聊天工具）读 PNG，旧应用读位图。
    // 位图转换失败时**不写**任何东西，由调用方如实报错——半写入的剪贴板会让人以为
    // 复制成功了。
    let dib = png_to_dib(png)
        .map_err(|reason| ClipboardError::Failed(format!("无法把图片转换为位图格式：{reason}")))?;
    set_clipboard_bytes(CF_DIB.0 as u32, &dib)?;
    if format != 0 {
        set_clipboard_bytes(format, png)?;
    }
    Ok(())
}

/// 剪贴板已打开：把一个字节块交给系统（所有权随之转移）。
unsafe fn set_clipboard_bytes(format: u32, bytes: &[u8]) -> Result<(), ClipboardError> {
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
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = read_locked();
            let _ = CloseClipboard();
            result
        }
    }

    /// 写入图片（ticket 10）：清空剪贴板后写入 `CF_DIB`（旧应用）与注册格式 `PNG`。
    ///
    /// `CF_DIB` 由 PNG 转换而来（[`png_to_dib`]），转换失败时在写入之前就失败，
    /// 不会留下一个半写入的剪贴板。
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        check_image_write(image)?;
        let format = unsafe { png_format() };
        unsafe {
            OpenClipboard(None)
                .map_err(|error| ClipboardError::Failed(format!("无法打开剪贴板：{error}")))?;
            let result = write_image_locked(&image.bytes, format);
            let _ = CloseClipboard();
            result
        }
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Ok(self.read_image_detailed()?.0)
    }
}

/// 剪贴板已打开：清空后写入 UTF-16 文本。所有权交给系统，失败路径自己释放。
unsafe fn write_locked(text: &str) -> Result<(), ClipboardError> {
    EmptyClipboard().map_err(|error| ClipboardError::Failed(format!("无法清空剪贴板：{error}")))?;

    let mut utf16: Vec<u16> = text.encode_utf16().collect();
    utf16.push(0);
    let bytes = std::mem::size_of_val(utf16.as_slice());

    let handle: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes)
        .map_err(|error| ClipboardError::Failed(format!("无法分配剪贴板内存：{error}")))?;
    let pointer = GlobalLock(handle);
    if pointer.is_null() {
        let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
        return Err(ClipboardError::Failed("无法锁定剪贴板内存".to_string()));
    }
    std::ptr::copy_nonoverlapping(utf16.as_ptr() as *const u8, pointer as *mut u8, bytes);
    let _ = GlobalUnlock(handle);

    // 成功之后内存块归系统所有，不能再释放；失败时要自己释放，避免泄漏。
    match SetClipboardData(CF_UNICODETEXT.0 as u32, Some(HANDLE(handle.0))) {
        Ok(_) => Ok(()),
        Err(error) => {
            let _ = windows::Win32::Foundation::GlobalFree(Some(handle));
            Err(ClipboardError::Failed(format!("无法写入剪贴板：{error}")))
        }
    }
}

/// 剪贴板已打开：读取 `CF_UNICODETEXT`。没有文本格式时返回 `Ok(None)`。
unsafe fn read_locked() -> Result<Option<String>, ClipboardError> {
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
        let text = WindowsClipboard::new()
            .read_text()?
            .filter(|text| !text.is_empty());
        let (image, problem) = WindowsClipboard::new().read_image_detailed()?;
        if text.is_none() && image.is_none() && problem.is_none() {
            return Ok(ClipboardPoll::Unchanged);
        }
        let source = unsafe { source_app() };
        let mut formats = Vec::new();
        if text.is_some() {
            formats.push(ClipboardFormatKind::Text);
        }
        if image.is_some() {
            formats.push(ClipboardFormatKind::Image);
        }
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats,
            text,
            image,
            image_problem: problem,
            source,
        }))
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
}
