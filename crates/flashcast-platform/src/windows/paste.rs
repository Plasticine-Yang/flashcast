//! Windows 合成粘贴：`SendInput` 注入 `Ctrl+V`。
//!
//! 用 `enigo` 而不是自己拼 `INPUT` 数组，是为了拿到两个已经核实过的细节：
//! `dwExtraInfo` 打标（宿主自己的全局快捷键不会把这次合成按键当成用户输入）以及
//! 丢弃时自动释放已按下的键（不会把 Ctrl 留在按下状态）。
//!
//! 诚实的能力边界：`SendInput` 受 UIPI 限制，**无法**注入到更高完整性级别
//! （以管理员运行）的前台窗口。这种情况下 `SendInput` 会静默丢弃事件，因此能力
//! 报告里把这一点写成限制说明，而不是承诺「一定能粘贴」。

use enigo::{Enigo, Key, Settings};

use crate::paste::{PasteError, Paster, SYNTHETIC_INPUT_MARKER};

/// Windows 的合成粘贴后端。
pub struct WindowsPaster;

impl Default for WindowsPaster {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsPaster {
    pub fn new() -> Self {
        Self
    }
}

impl Paster for WindowsPaster {
    fn paste(&self) -> Result<(), PasteError> {
        let settings = Settings {
            windows_dw_extra_info: Some(SYNTHETIC_INPUT_MARKER as usize),
            ..Settings::default()
        };
        let mut enigo = Enigo::new(&settings).map_err(|error| PasteError::Failed {
            reason: format!("无法连接 Windows 合成输入后端（SendInput）：{error}"),
        })?;
        crate::paste::send_paste_chord(&mut enigo, Key::Control)
    }
}
