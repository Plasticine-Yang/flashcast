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

/// 两个身份是否指向**同一个应用**。
///
/// 自动粘贴前必须用它核对「恢复后的前台就是我们捕获的那个应用」：只要不相等就绝不
/// 注入按键，否则内容会被粘贴到用户没有预期的窗口里（spec 明确禁止）。
///
/// 判定顺序按可靠性排列，且只用两边都有的字段：
///
/// 1. 平台原生窗口句柄（X11 窗口 id / Win32 `HWND`）——最可靠，同一个进程可能有多个窗口；
/// 2. 进程号（macOS / Windows 都能拿到）；
/// 3. 稳定标识（X11 的 `WM_CLASS` 实例名、macOS 的 bundle id）。
///
/// 两边都没有可比对字段时返回 `false`：**拿不准就不注入**，由宿主退回手动粘贴。
pub fn same_app(a: &FocusedApp, b: &FocusedApp) -> bool {
    if let (Some(left), Some(right)) = (a.window, b.window) {
        return left == right;
    }
    if let (Some(left), Some(right)) = (a.pid, b.pid) {
        return left == right;
    }
    !a.id.is_empty() && a.id == b.id
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, pid: Option<u32>, window: Option<u64>) -> FocusedApp {
        FocusedApp {
            id: id.to_string(),
            name: id.to_string(),
            wm_class: None,
            pid,
            window,
        }
    }

    #[test]
    fn window_handle_wins_over_pid() {
        // 同一个进程的另一个窗口：句柄不同即不是同一个前台，不能注入。
        let captured = app("editor", Some(7), Some(11));
        let other_window = app("editor", Some(7), Some(12));
        assert!(!same_app(&captured, &other_window));
        assert!(same_app(&captured, &captured.clone()));
    }

    #[test]
    fn falls_back_to_pid_then_id() {
        let captured = app("editor", Some(7), None);
        assert!(same_app(&captured, &app("editor", Some(7), None)));
        assert!(!same_app(&captured, &app("editor", Some(8), None)));
        // 两边都没有 pid 时才比稳定标识。
        assert!(same_app(
            &app("editor", None, None),
            &app("editor", None, None)
        ));
        assert!(!same_app(
            &app("editor", None, None),
            &app("browser", None, None)
        ));
    }

    #[test]
    fn refuses_when_nothing_comparable() {
        // 只有一边有 pid：pid 无法比对，退到稳定标识；标识不同就拒绝。
        assert!(!same_app(
            &app("editor", Some(7), None),
            &app("browser", None, None)
        ));
        // 窗口句柄都有但不同：即使 pid 相同也拒绝（同一进程的另一个窗口）。
        assert!(!same_app(
            &app("editor", Some(7), Some(3)),
            &app("editor", Some(7), Some(4))
        ));
        // 两边都没有任何可比对的身份（连稳定标识都是空的）：拒绝——拿不准就不注入。
        assert!(!same_app(&app("", None, None), &app("", None, None)));
    }
}
