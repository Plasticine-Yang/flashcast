//! 读取唤起前应用身份并恢复其焦点。
//!
//! Linux 上 X11 与 Wayland 的可得性完全不同：X11 可通过 EWMH
//! （`_NET_ACTIVE_WINDOW` / `_NET_CLIENT_LIST`）读取与恢复；Wayland 下普通应用
//! 拿不到全局焦点，此时必须返回 [`FocusError::Unsupported`]，不得伪造结果。

use serde::{Deserialize, Serialize};

/// 唤起前处于前台的应用程序身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FocusedApp {
    /// 稳定标识：X11 上为窗口类名（`WM_CLASS` 的实例名），否则为进程名。
    pub id: String,
    /// 面向用户的名称；无法获得时为 `id`。
    pub name: String,
    pub wm_class: Option<String>,
    pub pid: Option<u32>,
    /// 平台原生窗口句柄。X11 上为窗口 id，用于恢复焦点。
    pub window: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FocusError {
    #[error("当前桌面会话不支持读取焦点窗口：{reason}")]
    Unsupported { reason: String },
    #[error("当前没有可识别的焦点窗口")]
    NoActiveWindow,
    #[error("读取焦点窗口失败：{reason}")]
    Unavailable { reason: String },
}

/// 读取唤起点前台的应用程序，并在需要时恢复其焦点。
pub trait FocusTracker: Send + Sync {
    /// 读取当前处于前台的应用程序。
    fn capture(&self) -> Result<FocusedApp, FocusError>;

    /// 把焦点恢复给 `app`。`app.window` 为 `None` 时按 `wm_class` / `pid` 查找。
    fn restore(&self, app: &FocusedApp) -> Result<(), FocusError>;
}
