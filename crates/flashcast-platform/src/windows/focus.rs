//! Windows 焦点追踪：`GetForegroundWindow` + `GetWindowThreadProcessId` → 可执行文件路径，
//! 恢复用 `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`，失败时再用
//! `AttachThreadInput` 拼接输入队列（研究笔记 §3.1）。
//!
//! 为什么调用顺序是对的：外壳在**显示自己的窗口之前**调用 `capture()`
//! （`src-tauri/src/summon.rs`），此时 `GetForegroundWindow()` 仍然是用户唤起前
//! 的那个应用；等到需要恢复时，Flashcast 自己是前台进程，正好满足
//! `SetForegroundWindow` 文档里「调用者是前台进程」这一条，因此恢复通常一次成功。
//!
//! `AttachThreadInput` 只是尽力而为的兜底：微软从未把它列为绕过前台锁的受支持手段，
//! 因此恢复后必须回读 `GetForegroundWindow()` 验证，失败就如实报错，
//! 让 UI 退回「手动粘贴」的交互。

use crate::focus::{FocusError, FocusTracker, FocusedApp};
use crate::windows::identity::normalize_path;

pub struct WindowsFocusTracker;

impl Default for WindowsFocusTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsFocusTracker {
    pub fn new() -> Self {
        Self
    }
}

impl FocusTracker for WindowsFocusTracker {
    fn capture(&self) -> Result<FocusedApp, FocusError> {
        use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

        if !super::session::interactive_desktop_available() {
            return Err(FocusError::Unsupported {
                reason: "当前没有可交互的桌面会话（没有前台窗口），无法读取唤起前的应用；\
                         请使用托盘入口打开 Flashcast"
                    .to_string(),
            });
        }

        unsafe {
            let window = GetForegroundWindow();
            if window.is_invalid() {
                return Err(FocusError::NoActiveWindow);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(window, Some(&mut pid));
            if pid == 0 {
                return Err(FocusError::Unavailable {
                    reason: "无法取得前台窗口的进程 id".to_string(),
                });
            }
            // 唤起前的前台窗口属于 Flashcast 自己：说明调用顺序错了（窗口已经显示），
            // 这时必须报出来，而不是把 Flashcast 自己记成「唤起前的应用」。
            if pid == std::process::id() {
                return Err(FocusError::Unavailable {
                    reason: "当前前台窗口属于 Flashcast 自身；capture 必须在显示窗口之前调用"
                        .to_string(),
                });
            }
            let exe = process_image_path(pid).ok_or_else(|| FocusError::Unavailable {
                reason: format!("无法读取进程 {pid} 的可执行文件路径（可能已退出或权限不足）"),
            })?;
            let title = window_title(window);
            let name = title
                .clone()
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    std::path::Path::new(&exe)
                        .file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                })
                .unwrap_or_else(|| exe.clone());
            Ok(FocusedApp {
                id: normalize_path(&exe),
                name,
                // Windows 没有 X11 的 WM_CLASS；留空比伪造一个更有用。
                wm_class: None,
                pid: Some(pid),
                window: Some(window.0 as u64),
            })
        }
    }

    fn restore(&self, app: &FocusedApp) -> Result<(), FocusError> {
        let Some(window) = app.window else {
            return Err(FocusError::Unavailable {
                reason: "没有可用的窗口句柄（Windows 上必须用 HWND 恢复焦点，无法只靠进程名）"
                    .to_string(),
            });
        };
        if !super::session::interactive_desktop_available() {
            return Err(FocusError::Unsupported {
                reason: "当前没有可交互的桌面会话，无法恢复焦点".to_string(),
            });
        }
        let target = windows::Win32::Foundation::HWND(window as *mut core::ffi::c_void);
        if force_foreground(target) {
            Ok(())
        } else {
            Err(FocusError::Unavailable {
                reason: format!(
                    "Windows 拒绝了把焦点还给「{}」（前台锁未放开）；请手动切换回该应用",
                    app.name
                ),
            })
        }
    }
}

/// 把 `target` 带到前台；返回是否确实成功（以回读 `GetForegroundWindow` 为准）。
fn force_foreground(target: windows::Win32::Foundation::HWND) -> bool {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetActiveWindow, SetActiveWindow, SetFocus};
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsIconic, SetForegroundWindow, ShowWindow,
        SW_RESTORE,
    };

    unsafe {
        if target.is_invalid() {
            return false;
        }
        if IsIconic(target).as_bool() {
            let _ = ShowWindow(target, SW_RESTORE);
        }
        if SetForegroundWindow(target).as_bool() && GetForegroundWindow() == target {
            return true;
        }

        // 兜底：拼接当前线程与目标线程的输入队列，暂时解除前台激活限制。
        let target_thread = GetWindowThreadProcessId(target, None);
        let our_thread = windows::Win32::System::Threading::GetCurrentThreadId();
        if target_thread == 0 || target_thread == our_thread {
            return GetForegroundWindow() == target;
        }
        let attached = windows::Win32::System::Threading::AttachThreadInput(
            our_thread,
            target_thread,
            true,
        );
        if !attached.as_bool() {
            return GetForegroundWindow() == target;
        }
        let previous = GetActiveWindow();
        let _ = SetActiveWindow(target);
        let _ = SetFocus(Some(target));
        let _ = SetForegroundWindow(target);
        if !previous.is_invalid() {
            let _ = SetActiveWindow(previous);
        }
        let _ = windows::Win32::System::Threading::AttachThreadInput(
            our_thread,
            target_thread,
            false,
        );
        GetForegroundWindow() == target
    }
}

/// 读取进程的可执行文件路径。
///
/// 用 `PROCESS_QUERY_LIMITED_INFORMATION`：它跨完整性级别可用，且不需要
/// `PROCESS_VM_READ`，因此对提升权限的进程也能读（`GetModuleFileNameExW` 不行）。
fn process_image_path(pid: u32) -> Option<String> {
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 32_768];
        let mut length = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        );
        let _ = CloseHandle(handle);
        result.ok()?;
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        let trimmed = path.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}

/// 窗口标题；读取失败或为空时返回 `None`。
fn window_title(window: windows::Win32::Foundation::HWND) -> Option<String> {
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowTextLengthW, GetWindowTextW};

    unsafe {
        let length = GetWindowTextLengthW(window);
        if length <= 0 {
            return None;
        }
        let mut buffer = vec![0u16; length as usize + 1];
        let written = GetWindowTextW(window, &mut buffer);
        if written <= 0 {
            return None;
        }
        let title = String::from_utf16_lossy(&buffer[..written as usize]);
        let trimmed = title.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }
}
