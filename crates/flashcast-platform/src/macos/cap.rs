//! macOS 能力探测：如实报告系统、架构、桌面会话与各项权限状态。
//!
//! 「探测到的事实」与「由此得出的结论」分开成 [`MacosEnvironment`] 与
//! [`capabilities_for`]：结论部分因此可以在 Linux 上被真实测试，而不是只能靠
//! macOS runner 才能验证。
//!
//! 辅助功能权限没有独立的 `Capabilities` 字段，通过 [`Capabilities::notes`]
//! 与 `auto_paste` 状态暴露：UI 据此把用户引导到正确的设置面板，而不是让
//! 自动粘贴静默失败。

use crate::capability::{Capabilities, OsKind, SessionType, Support};

#[cfg(target_os = "macos")]
use crate::capability::CapabilityProbe;

use super::focus::ACCESSIBILITY_SETTINGS_LABEL;

/// 探测到的 macOS 环境事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MacosEnvironment {
    /// 是否存在可交互的桌面会话（能取到前台应用）。
    pub desktop_available: bool,
    /// 当前进程是否已获得「辅助功能」权限。
    pub accessibility_granted: bool,
}

impl Default for MacosEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosEnvironment {
    #[cfg(target_os = "macos")]
    pub fn new() -> Self {
        Self {
            desktop_available: desktop_available(),
            accessibility_granted: super::focus::accessibility_granted(),
        }
    }

    #[cfg(not(target_os = "macos"))]
    pub fn new() -> Self {
        Self {
            desktop_available: false,
            accessibility_granted: false,
        }
    }

    /// 辅助功能权限的中文状态说明。
    pub fn accessibility_label(&self) -> String {
        if self.accessibility_granted {
            "辅助功能权限：已授权".to_string()
        } else {
            format!(
                "辅助功能权限：未授权（自动粘贴不可用；设置入口：{ACCESSIBILITY_SETTINGS_LABEL}）"
            )
        }
    }
}

/// 由环境事实与系统版本得出能力快照。纯函数，任何平台都可验证。
pub fn capabilities_for(
    environment: MacosEnvironment,
    os_version: Option<String>,
    arch: String,
) -> Capabilities {
    let mut notes = vec![environment.accessibility_label()];
    notes.push(
        "软件发现扫描 /Applications、/System/Applications、/System/Applications/Utilities、\
         /Applications/Utilities 与 ~/Applications 下的 .app 包。"
            .to_string(),
    );
    notes.push(
        "应用图标由 NSWorkspace 渲染为 PNG 并缓存（现代应用只提供 Assets.car，没有可用的 .icns）。"
            .to_string(),
    );
    if environment.desktop_available {
        notes.push(
            "全局快捷键依赖应用的 Cocoa 事件循环，注册需在应用启动阶段（主线程）完成。".to_string(),
        );
    } else {
        notes.push("未检测到可交互的桌面会话（无 Aqua 会话），托盘与窗口可能不可用。".to_string());
    }

    Capabilities {
        os: OsKind::Macos,
        os_version,
        arch,
        // X11 / Wayland 的会话区分只存在于 Linux；macOS 固定为不适用。
        session: SessionType::NotApplicable,
        desktop_available: environment.desktop_available,
        hotkey: if environment.desktop_available {
            Support::Supported
        } else {
            Support::Unsupported {
                reason: "当前没有可交互的桌面会话（无 Aqua 会话）".to_string(),
            }
        },
        // 剪贴板适配：ticket 09/10/11 分别实现文字、图片与富文本的读；写只提供纯文本。
        clipboard: Support::Unknown {
            reason: "文字与图片已实现（ticket 09/10：pbpaste 读文本、图片经 osascript 读写）；\
                     HTML/RTF 不声称支持（ticket 11）；文件列表尚未实现（ticket 12 覆盖）；\
                     macOS 由 NSPasteboard 提供，无需外部工具"
                .to_string(),
        },
        auto_paste: if environment.accessibility_granted {
            // 已授权：CGEvent 可以注入（ticket 08 的 `MacosPaster`）。
            Support::Supported
        } else {
            Support::Unsupported {
                reason: format!(
                    "未获得「辅助功能」权限，无法向其他应用注入按键；请在 {ACCESSIBILITY_SETTINGS_LABEL} \
                     中勾选 Flashcast"
                ),
            }
        },
        notes,
    }
}

/// 是否存在可交互的桌面会话。
///
/// 判断依据是 `NSWorkspace` 能否报出前台应用：没有 Aqua 会话（纯 SSH、无登录
/// 桌面）时拿不到。请注意这不是「有没有 GUI 框架」的证明，只是最便宜且诚实的
/// 可用性信号。
#[cfg(target_os = "macos")]
pub fn desktop_available() -> bool {
    use objc2_app_kit::NSWorkspace;

    // NSWorkspace 的查询接口在 objc2 0.3 中是安全方法（不涉及裸指针）。
    let workspace = NSWorkspace::sharedWorkspace();
    workspace.frontmostApplication().is_some()
}

/// 系统版本描述，来自 `NSProcessInfo`（例如 `Version 14.5 (Build 23F79)`）。
#[cfg(target_os = "macos")]
pub fn os_version() -> Option<String> {
    use objc2_foundation::NSProcessInfo;

    let info = NSProcessInfo::processInfo();
    let version = info.operatingSystemVersionString().to_string();
    (!version.is_empty()).then_some(version)
}

#[cfg(target_os = "macos")]
pub struct MacosCapabilityProbe;

#[cfg(target_os = "macos")]
impl Default for MacosCapabilityProbe {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosCapabilityProbe {
    pub fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "macos")]
impl CapabilityProbe for MacosCapabilityProbe {
    fn probe(&self) -> Capabilities {
        capabilities_for(
            MacosEnvironment::new(),
            os_version(),
            crate::capability::target_arch(),
        )
    }
}
