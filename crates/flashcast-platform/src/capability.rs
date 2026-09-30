//! 平台能力探测：系统、架构、桌面会话类型与各能力的真实支持状态。
//!
//! 「未覆盖」必须与「不支持」区分：`Support::Unknown` 表示当前环境无法判定，
//! `Support::Unsupported` 表示已判定为不可用并附有原因。

use serde::{Deserialize, Serialize};

/// 操作系统类别。v0.1.0 只覆盖以下三种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OsKind {
    Linux,
    Windows,
    Macos,
    Unknown,
}

impl OsKind {
    pub fn as_str(self) -> &'static str {
        match self {
            OsKind::Linux => "linux",
            OsKind::Windows => "windows",
            OsKind::Macos => "macos",
            OsKind::Unknown => "unknown",
        }
    }
}

/// Linux 会话类型。在其他平台恒为 `NotApplicable`。
///
/// `X11` 与 `Wayland` 必须分别记录：X11 检查通过不能推断 Wayland 通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionType {
    X11,
    Wayland,
    /// 无法判定（既没有 DISPLAY 也没有 WAYLAND_DISPLAY，或环境变量缺失）。
    Unknown,
    /// 没有可用的桌面会话（CI runner、纯 TTY）。
    Headless,
    /// 非 Linux 平台。
    NotApplicable,
}

impl SessionType {
    pub fn as_str(self) -> &'static str {
        match self {
            SessionType::X11 => "x11",
            SessionType::Wayland => "wayland",
            SessionType::Unknown => "unknown",
            SessionType::Headless => "headless",
            SessionType::NotApplicable => "not-applicable",
        }
    }

    /// 面向用户的中文说明。
    pub fn label_zh(self) -> &'static str {
        match self {
            SessionType::X11 => "X11",
            SessionType::Wayland => "Wayland",
            SessionType::Unknown => "未知会话类型",
            SessionType::Headless => "无桌面会话",
            SessionType::NotApplicable => "不适用",
        }
    }
}

/// 单项能力的支持状态。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum Support {
    /// 实测或由确定的平台事实判定为可用。
    Supported,
    /// 已判定不可用，`reason` 为面向用户的中文原因。
    Unsupported { reason: String },
    /// 当前环境无法判定。
    Unknown { reason: String },
}

impl Support {
    pub fn is_supported(&self) -> bool {
        matches!(self, Support::Supported)
    }

    pub fn reason(&self) -> Option<&str> {
        match self {
            Support::Supported => None,
            Support::Unsupported { reason } | Support::Unknown { reason } => Some(reason),
        }
    }

    /// 面向用户的中文标签。
    pub fn label_zh(&self) -> String {
        match self {
            Support::Supported => "支持".to_string(),
            Support::Unsupported { reason } => format!("不支持（{reason}）"),
            Support::Unknown { reason } => format!("未覆盖（{reason}）"),
        }
    }
}

/// 平台能力快照。由 [`CapabilityProbe::probe`] 返回。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub os: OsKind,
    /// 发行版或系统版本描述；无法获得时为 `None`。
    pub os_version: Option<String>,
    pub arch: String,
    pub session: SessionType,
    /// 是否存在可交互的桌面会话（窗口、托盘可用的前提）。
    pub desktop_available: bool,
    pub hotkey: Support,
    pub clipboard: Support,
    pub auto_paste: Support,
    /// 额外说明（例如 Wayland 下的具体限制）。
    pub notes: Vec<String>,
}

/// 报告平台能力。实现必须如实反映当前进程所在环境。
pub trait CapabilityProbe: Send + Sync {
    fn probe(&self) -> Capabilities;
}

/// 进程的目标架构字符串（编译期常量，不依赖运行时检测）。
pub fn target_arch() -> String {
    std::env::consts::ARCH.to_string()
}

/// 编译目标对应的 [`OsKind`]。
pub fn target_os() -> OsKind {
    match std::env::consts::OS {
        "linux" => OsKind::Linux,
        "windows" => OsKind::Windows,
        "macos" => OsKind::Macos,
        _ => OsKind::Unknown,
    }
}
