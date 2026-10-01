//! Linux 平台适配层实现。
//!
//! 会话类型是本模块最重要的输入：X11 与 Wayland 的可得性完全不同，
//! 每个实现都先判断会话类型，再决定是执行真实操作还是如实报告「不支持」。

pub mod cap;
pub mod catalog;
pub mod clipboard;
pub mod focus;
pub mod hotkeys;
pub mod launcher;
pub mod paste;
pub mod x11;

pub use cap::LinuxCapabilityProbe;
pub use catalog::LinuxAppCatalog;
pub use clipboard::LinuxClipboard;
pub use focus::LinuxFocusTracker;
pub use hotkeys::LinuxHotkeyManager;
pub use launcher::LinuxLauncher;
pub use paste::LinuxPaster;

use crate::capability::SessionType;

/// 判断当前桌面会话类型。
///
/// `XDG_SESSION_TYPE` 优先；缺失时回退到 `WAYLAND_DISPLAY` / `DISPLAY`。
/// 注意 Wayland 会话下通常也设置了 `DISPLAY`（XWayland），因此不能凭 `DISPLAY`
/// 判定为 X11。
pub fn detect_session_type() -> SessionType {
    detect_session_type_from(
        std::env::var("XDG_SESSION_TYPE").ok().as_deref(),
        std::env::var("WAYLAND_DISPLAY").ok().as_deref(),
        std::env::var("DISPLAY").ok().as_deref(),
    )
}

/// 使用显式取值判断会话类型，便于测试。
pub fn detect_session_type_from(
    xdg_session_type: Option<&str>,
    wayland_display: Option<&str>,
    display: Option<&str>,
) -> SessionType {
    let has_wayland = wayland_display.map(|v| !v.is_empty()).unwrap_or(false);
    let has_x11 = display.map(|v| !v.is_empty()).unwrap_or(false);
    match xdg_session_type.map(str::trim).map(str::to_ascii_lowercase) {
        Some(ref value) if value == "wayland" => SessionType::Wayland,
        Some(ref value) if value == "x11" => SessionType::X11,
        Some(ref value) if value == "tty" || value == "headless" => SessionType::Headless,
        // 明确给出的其他取值（mir 等）当前按未知处理，不用 X11 检查推断支持。
        Some(ref value) if !value.is_empty() => {
            if has_wayland {
                SessionType::Wayland
            } else if has_x11 {
                SessionType::Unknown
            } else {
                SessionType::Unknown
            }
        }
        _ => {
            if has_wayland {
                SessionType::Wayland
            } else if has_x11 {
                SessionType::X11
            } else {
                SessionType::Headless
            }
        }
    }
}

/// Wayland 下无法读取全局焦点的统一原因说明。
pub const WAYLAND_FOCUS_REASON: &str =
    "Wayland 会话不向普通应用暴露全局焦点窗口；GNOME 等合成器未提供可用的公开接口。\
     如需记录唤起前应用，请在 X11 会话下运行，或使用后续 ticket 提供的替代方案。";

/// Wayland 下无法自动粘贴的统一原因说明。
///
/// 这句话会出现在用户看到的能力报告与粘贴反馈里，因此必须说清「为什么不行」以及
/// 「怎么才行」：合成器不提供把焦点交给其他应用后再注入按键的公开接口，只有
/// XDG RemoteDesktop 门户（需要用户授权）才能做到，本版本不申请该权限。
pub const WAYLAND_PASTE_REASON: &str =
    "Wayland 不允许应用在把焦点交给其他应用后注入按键：GNOME 没有可用的公开接口，\
     只有经 XDG RemoteDesktop 门户授权后才能做到，本版本不申请该权限。\
     内容已复制到剪贴板，可手动粘贴。";

/// 是否允许在 Wayland 会话下强制尝试 X11 后端（仅用于诊断）。
pub fn force_x11_backend() -> bool {
    std::env::var("FLASHCAST_FORCE_X11_BACKEND")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}
