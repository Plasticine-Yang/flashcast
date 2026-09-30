//! Windows 会话探测。
//!
//! Windows 没有 X11/Wayland 之分，但对启动器而言关键事实是**是否存在可交互的
//! 桌面**：服务会话（session 0）与没有登录会话的 runner 上，窗口站是不可见的，
//! 此时全局快捷键注册与焦点读取都没有意义，必须如实报告而不是伪造成功。
//!
//! 判定用 `GetForegroundWindow()`：交互式桌面总会有一个前台窗口（包括「桌面」本身
//! 属于 Explorer 的情况）；服务/无头会话返回 NULL。没有前台窗口时再看 shell 窗口，
//! 覆盖「刚登录、还没有前台窗口」的短暂状态。

use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetShellWindow};

/// 桌面会话的原始可观测事实，供真实平台检查如实报告（而不是只给一个布尔值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopSession {
    /// `GetForegroundWindow()` 是否返回了有效窗口。
    pub foreground_window: bool,
    /// `GetShellWindow()` 是否返回了有效窗口（Explorer 的桌面窗口）。
    pub shell_window: bool,
}

impl DesktopSession {
    /// 是否存在可交互桌面：有前台窗口，或有 shell 窗口。
    pub fn interactive(self) -> bool {
        self.foreground_window || self.shell_window
    }

    /// 面向用户的中文描述。
    pub fn label_zh(self) -> &'static str {
        match (self.foreground_window, self.shell_window) {
            (true, true) => "交互式桌面（有前台窗口与 shell 窗口）",
            (true, false) => "交互式桌面（有前台窗口，无 shell 窗口）",
            (false, true) => "只有 shell 窗口，暂时没有前台窗口",
            (false, false) => "没有可交互桌面（服务会话或无头 runner）",
        }
    }
}

/// 读取当前桌面会话状态。
pub fn desktop_session() -> DesktopSession {
    unsafe {
        DesktopSession {
            foreground_window: !GetForegroundWindow().is_invalid(),
            shell_window: !GetShellWindow().is_invalid(),
        }
    }
}

/// 当前进程是否能看到一个可交互的桌面会话。
pub fn interactive_desktop_available() -> bool {
    desktop_session().interactive()
}
