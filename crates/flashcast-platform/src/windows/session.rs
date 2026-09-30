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

/// 当前进程是否能看到一个可交互的桌面会话。
pub fn interactive_desktop_available() -> bool {
    unsafe {
        if !GetForegroundWindow().is_invalid() {
            return true;
        }
        !GetShellWindow().is_invalid()
    }
}
