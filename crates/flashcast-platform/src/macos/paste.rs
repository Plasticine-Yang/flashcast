//! macOS 合成粘贴：`CGEvent` 注入 `Cmd+V`。
//!
//! 权限门槛在这里显式检查，而不是让 `CGEventPost` 静默失败：
//! `AXIsProcessTrusted()` 为 false 时进程无法向其他应用注入事件，直接返回
//! [`PasteError::PermissionMissing`] 并给出中文设置路径。
//!
//! `open_prompt_to_get_permissions` 设为 `false`：权限提示由宿主在用户能看见的
//! 时机（设置页 / 能力报告）触发，不在粘贴流程里凭空弹窗（研究 §4.1）。
//! `event_source_user_data` 打标，让宿主自己的快捷键忽略这次合成按键。
//!
//! 与同目录的 `focus.rs` 一样：模块在任意目标上都能编译（常量与分支判断是纯逻辑），
//! 只有真正调用 Apple / enigo API 的 `MacosPaster` 按 `cfg(target_os = "macos")`
//! 条件编译。

#[cfg(target_os = "macos")]
use enigo::{Enigo, Key, Settings};

#[cfg(target_os = "macos")]
use crate::paste::{PasteError, Paster, SYNTHETIC_INPUT_MARKER};

/// 「辅助功能」权限缺失时给用户的中文原因（与能力探测共用同一句话）。
pub fn permission_reason() -> String {
    format!(
        "未获得「辅助功能」权限，无法向其他应用注入按键；请在 {} 中勾选 Flashcast",
        super::focus::ACCESSIBILITY_SETTINGS_LABEL
    )
}

/// macOS 的合成粘贴后端。
#[cfg(target_os = "macos")]
pub struct MacosPaster;

#[cfg(target_os = "macos")]
impl Default for MacosPaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(target_os = "macos")]
impl MacosPaster {
    pub fn new() -> Self {
        Self
    }

    /// 当前是否已获得「辅助功能」权限。能力探测复用同一次判断。
    pub fn accessible(&self) -> bool {
        super::focus::accessibility_granted()
    }
}

#[cfg(target_os = "macos")]
impl Paster for MacosPaster {
    fn paste(&self) -> Result<(), PasteError> {
        if !self.accessible() {
            return Err(PasteError::PermissionMissing {
                reason: permission_reason(),
            });
        }
        let settings = Settings {
            event_source_user_data: Some(SYNTHETIC_INPUT_MARKER),
            // 权限已在上面的分支里处理，这里不再弹系统提示。
            open_prompt_to_get_permissions: false,
            ..Settings::default()
        };
        let mut enigo = Enigo::new(&settings).map_err(|error| PasteError::Failed {
            reason: format!("无法连接 macOS 合成输入后端（CGEvent）：{error}"),
        })?;
        crate::paste::send_paste_chord(&mut enigo, Key::Meta)
    }
}
