//! Windows 能力探测：如实报告系统、架构、桌面会话与各项能力的真实状态。
//!
//! Windows 没有 X11/Wayland 之分，[`SessionType::NotApplicable`] 是刻意的：
//! 「会话类型」是 Linux 概念，在 Windows 上编一个值（例如硬说成 X11）会让
//! 平台报告失真。对启动器真正有意义的是 `desktop_available` —— 是否存在可交互的
//! 桌面（服务会话 session 0 与无头 runner 上没有），它决定快捷键与焦点读取是否有意义。

use crate::capability::{target_arch, Capabilities, CapabilityProbe, OsKind, SessionType, Support};

pub struct WindowsCapabilityProbe;

impl Default for WindowsCapabilityProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsCapabilityProbe {
    pub fn new() -> Self {
        Self
    }

    /// 当前是否存在可交互的桌面会话。
    pub fn desktop_available(&self) -> bool {
        super::session::interactive_desktop_available()
    }
}

impl CapabilityProbe for WindowsCapabilityProbe {
    fn probe(&self) -> Capabilities {
        let desktop_available = self.desktop_available();
        let mut notes = Vec::new();
        notes.push(
            "Windows 没有 X11/Wayland 之分；会话类型记为「不适用」，桌面可用性单独报告。"
                .to_string(),
        );
        if !desktop_available {
            notes.push(
                "当前没有可交互的桌面会话（无前台窗口与 shell 窗口）：全局快捷键、焦点读取\
                 与窗口唤起都没有意义，本报告中的相关检查为「未覆盖」。"
                    .to_string(),
            );
        }
        notes.push(
            "打包应用（UWP/MSIX）通过 Get-StartApps 枚举、以 AUMID 启动；\
             没有开始菜单快捷方式、也没有可推导 .exe 的注册表条目会被丢弃。"
                .to_string(),
        );

        Capabilities {
            os: OsKind::Windows,
            os_version: windows_version(),
            arch: target_arch(),
            session: SessionType::NotApplicable,
            desktop_available,
            hotkey: if desktop_available {
                // RegisterHotKey 在 Windows 上总是存在；组合是否被占用只能在真实注册时才知道。
                Support::Supported
            } else {
                Support::Unsupported {
                    reason: "当前没有可交互的桌面会话，RegisterHotKey 无法工作；\
                             请使用托盘入口打开 Flashcast"
                        .to_string(),
                }
            },
            clipboard: clipboard_support(desktop_available),
            auto_paste: auto_paste_support(desktop_available),
            notes,
        }
    }
}

/// 剪贴板支持。ticket 02 尚未实现剪贴板适配，因此只报告**环境前提**，
/// 状态保持「未覆盖」，不得写成支持。
fn clipboard_support(desktop_available: bool) -> Support {
    if !desktop_available {
        return Support::Unsupported {
            reason: "当前没有可交互的桌面会话，无法读写剪贴板".to_string(),
        };
    }
    Support::Unknown {
        reason: "剪贴板适配尚未实现（ticket 09/10 覆盖）；Windows 提供 OpenClipboard 与 \
                 WinRT Clipboard API，环境本身具备条件"
            .to_string(),
    }
}

/// 自动粘贴支持。ticket 02 尚未实现自动粘贴，因此同样保持「未覆盖」。
fn auto_paste_support(desktop_available: bool) -> Support {
    if !desktop_available {
        return Support::Unsupported {
            reason: "当前没有可交互的桌面会话，无法注入按键".to_string(),
        };
    }
    Support::Unknown {
        reason: "自动粘贴适配尚未实现（ticket 08 覆盖）；Windows 可用 SendInput 注入按键"
            .to_string(),
    }
}

/// 从注册表读取系统版本描述。
fn windows_version() -> Option<String> {
    let key = windows_registry::LOCAL_MACHINE
        .open(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion")
        .ok()?;
    let get = |name: &str| key.get_string(name).ok();
    super::version::format(
        get("ProductName").as_deref(),
        get("DisplayVersion").as_deref(),
        get("CurrentBuild").as_deref(),
        key.get_u32("UBR").ok(),
    )
}
