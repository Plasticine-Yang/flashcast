//! Linux 能力探测：如实报告会话类型与各项能力的真实状态。

use crate::capability::{Capabilities, CapabilityProbe, OsKind, SessionType, Support};

use super::{detect_session_type, WAYLAND_FOCUS_REASON};

pub struct LinuxCapabilityProbe {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxCapabilityProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxCapabilityProbe {
    pub fn new() -> Self {
        Self {
            session: detect_session_type(),
            force_x11: super::force_x11_backend(),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self { session, force_x11 }
    }
}

impl CapabilityProbe for LinuxCapabilityProbe {
    fn probe(&self) -> Capabilities {
        let session = self.session;
        let has_x11_display = env_non_empty("DISPLAY");
        let has_wayland_display = env_non_empty("WAYLAND_DISPLAY");
        let desktop_available = matches!(session, SessionType::X11 | SessionType::Wayland)
            && (has_x11_display || has_wayland_display);

        let mut notes = Vec::new();
        if let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") {
            if !desktop.is_empty() {
                notes.push(format!("桌面环境：{desktop}"));
            }
        }
        if session == SessionType::Wayland {
            notes.push(WAYLAND_FOCUS_REASON.to_string());
            if has_x11_display {
                notes.push(
                    "检测到 XWayland 的 DISPLAY，但 X11 检查结果不能推断 Wayland 下的行为。"
                        .to_string(),
                );
            }
        }
        if std::env::var("DBUS_SESSION_BUS_ADDRESS")
            .map(|v| v.is_empty())
            .unwrap_or(true)
        {
            notes.push("未检测到 D-Bus 会话总线，托盘与通知可能不可用。".to_string());
        }

        let hotkey = match session {
            SessionType::X11 => Support::Supported,
            SessionType::Wayland if self.force_x11 => Support::Unknown {
                reason: format!(
                    "已通过 FLASHCAST_FORCE_X11_BACKEND 强制使用 X11 后端；{}",
                    "XWayland 下抓取通常无法收到按键，结果不可靠"
                ),
            },
            SessionType::Wayland => Support::Unsupported {
                reason: "Wayland 会话不支持全局快捷键抓取（global-hotkey 仅实现 X11 的 XGrabKey，\
                         GNOME 未提供 XDG GlobalShortcuts 门户的一等实现）；请使用托盘入口打开 Flashcast"
                    .to_string(),
            },
            SessionType::Headless => Support::Unsupported {
                reason: "当前没有桌面会话".to_string(),
            },
            SessionType::Unknown | SessionType::NotApplicable => Support::Unknown {
                reason: "无法确定会话类型，未覆盖全局快捷键检查".to_string(),
            },
        };

        Capabilities {
            os: OsKind::Linux,
            os_version: os_version(),
            arch: crate::capability::target_arch(),
            session,
            desktop_available,
            hotkey,
            clipboard: clipboard_support(session),
            auto_paste: auto_paste_support(session, self.force_x11),
            notes,
        }
    }
}

/// 剪贴板支持。
///
/// 只报告**环境前提**是否具备（文字、图片与富文本的适配已由 ticket 09/10/11 实现，
/// 但真实读写能否成功取决于当前会话有没有可用的选区），因此状态保持「未覆盖」，
/// 不得写成支持。
fn clipboard_support(session: SessionType) -> Support {
    let wayland_tool = which("wl-copy");
    let x11_tool = which("xclip").or_else(|| which("xsel"));
    match session {
        SessionType::Wayland => match wayland_tool {
            Some(tool) => Support::Unknown {
                reason: format!(
                    "文字、HTML/RTF 与图片已实现（ticket 09/10/11：读按 MIME 类型，写只提供纯文本，图片按 image/png 写）；文件列表尚未实现（ticket 12 覆盖）；环境已具备 {tool}"
                ),
            },
            None => Support::Unsupported {
                reason: "未找到 wl-copy，Wayland 下无法读写剪贴板".to_string(),
            },
        },
        _ => match x11_tool {
            Some(tool) => Support::Unknown {
                reason: format!(
                    "文字、HTML/RTF 与图片已实现（ticket 09/10/11：读按 MIME 类型，写只提供纯文本，图片按 image/png 写）；文件列表尚未实现（ticket 12 覆盖）；环境已具备 {tool}"
                ),
            },
            None => Support::Unsupported {
                reason: "未找到 xclip 或 xsel，X11 下无法读写剪贴板".to_string(),
            },
        },
    }
}

/// 自动粘贴支持。
///
/// X11：能不能注入取决于服务器是否提供 XTEST，因此这里真的去问一次服务器；问不到
/// 就如实报「不支持」，不写成未覆盖。
/// Wayland：普通应用无法把焦点转给其他应用后注入按键（研究 §3.4），直接不支持。
fn auto_paste_support(session: SessionType, force_x11: bool) -> Support {
    match session {
        SessionType::X11 => x11_paste_support(),
        SessionType::Wayland if force_x11 => match x11_paste_support() {
            Support::Supported => Support::Unknown {
                reason: "已通过 FLASHCAST_FORCE_X11_BACKEND 强制使用 X11 后端：只有 \
                         XWayland 里的 X11 客户端能收到合成按键，原生 Wayland 客户端收不到，\
                         因此不能算作 Wayland 支持"
                    .to_string(),
            },
            other => other,
        },
        SessionType::Wayland => Support::Unsupported {
            reason: super::WAYLAND_PASTE_REASON.to_string(),
        },
        SessionType::Headless => Support::Unsupported {
            reason: "当前没有桌面会话，无法注入粘贴".to_string(),
        },
        SessionType::Unknown | SessionType::NotApplicable => Support::Unknown {
            reason: "无法确定会话类型，未覆盖自动粘贴检查".to_string(),
        },
    }
}

/// X11 下的自动粘贴支持：以服务器是否提供 XTEST 扩展为准。
pub fn x11_paste_support() -> Support {
    if super::x11::xtest_available() {
        Support::Supported
    } else {
        Support::Unsupported {
            reason: "X11 服务器未提供 XTEST 扩展（或无法连接 DISPLAY），无法注入粘贴按键"
                .to_string(),
        }
    }
}

fn env_non_empty(key: &str) -> bool {
    std::env::var(key).map(|v| !v.is_empty()).unwrap_or(false)
}

/// 在 PATH 中查找可执行文件，返回其名称。
fn which(program: &str) -> Option<String> {
    let path = std::env::var("PATH").ok()?;
    path.split(':')
        .filter(|segment| !segment.is_empty())
        .map(|segment| std::path::Path::new(segment).join(program))
        .find(|candidate| candidate.is_file())
        .map(|_| program.to_string())
}

/// 从 `/etc/os-release` 读取发行版描述。
fn os_version() -> Option<String> {
    let content = std::fs::read_to_string("/etc/os-release").ok()?;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("PRETTY_NAME=") {
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}
