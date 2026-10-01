//! 合成系统粘贴（ADR §5 的 `Paster`）。
//!
//! spec「粘贴是宿主级操作」把粘贴拆成四步：保存唤起前的目标应用、准备剪贴板、
//! 关闭浮窗、恢复目标应用并发起系统粘贴。本模块只做**最后一步**：在已经恢复到前台的
//! 窗口上注入一次 `Ctrl+V` / `Cmd+V`。
//!
//! 边界写在这里，因为「谁负责什么」是这个能力最容易出错的地方：
//!
//! - 恢复焦点是 [`crate::focus::FocusTracker`] 的职责；
//! - 「恢复后的前台必须仍然是我们捕获的那个应用，否则绝不注入」是宿主的职责
//!   （宿主在调用本 trait 之前核对，见 `Host::complete_paste`）；
//! - 本模块只回答「能不能注入」以及「注入失败了为什么」，并且必须如实回答。
//!
//! ## 各平台的注入方式与门槛
//!
//! | 平台 | 机制 | 门槛 |
//! | --- | --- | --- |
//! | Linux X11 | XTEST（`enigo` 的 `x11rb` 后端） | 服务器需提供 XTEST 扩展；无门槛 |
//! | Linux Wayland | **无**（GNOME 不允许转移焦点后注入） | 只有 XDG RemoteDesktop 门户授权后才行，本版本不申请该权限 |
//! | Windows | `SendInput`，`dwExtraInfo` 打标 | UIPI：无法注入到更高完整性级别（管理员）的前台窗口 |
//! | macOS | `CGEvent`，`EVENT_SOURCE_USER_DATA` 打标 | 必须已获得「辅助功能」权限 |
//!
//! 三个平台都**不**通过拼接 shell 命令来发送按键：CLI 工具（`xdotool`、`wtype`、
//! `ydotool`）要么只覆盖 X11、要么需要 wlroots 专有协议，要么要求 root 或 `input`
//! 组成员（研究 §4.2）。因此这里使用进程内的合成输入库。

/// 在**当前前台窗口**上发起一次系统粘贴。
pub trait Paster: Send + Sync {
    /// 注入粘贴快捷键。调用方必须已经确认当前前台就是目标应用。
    fn paste(&self) -> Result<(), PasteError>;
}

/// 发起粘贴的失败原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PasteError {
    #[error("当前桌面会话不支持自动粘贴：{reason}")]
    Unsupported { reason: String },
    #[error("缺少自动粘贴所需的权限：{reason}")]
    PermissionMissing { reason: String },
    #[error("发起自动粘贴失败：{reason}")]
    Failed { reason: String },
}

/// 合成按键的标记值。
///
/// Windows 写进 `dwExtraInfo`、macOS 写进 `EVENT_SOURCE_USER_DATA`：宿主自己的
/// 全局快捷键监听据此忽略「我们刚注入的那次按键」，否则一次自动粘贴可能再次唤起
/// 启动器（研究 §4.3）。取值只是任意非零常量，语义是「Flashcast 合成」。
pub const SYNTHETIC_INPUT_MARKER: i64 = 0x4643_4153; // "FCAS"

/// 三个桌面平台共用的合成粘贴动作：按下修饰键 → 点一次 `V` → 松开修饰键。
///
/// `enigo` 的 `Settings::release_keys_when_dropped` 默认为 `true`，因此即使中途失败，
/// 修饰键也会在 `Enigo` 被丢弃时释放，不会把 Ctrl / Cmd 留在按下状态；这里仍然显式
/// 松开，保证成功路径上的按键顺序完整。
#[cfg(any(target_os = "linux", target_os = "windows", target_os = "macos"))]
pub(crate) fn send_paste_chord(
    enigo: &mut enigo::Enigo,
    modifier: enigo::Key,
) -> Result<(), PasteError> {
    use enigo::{Direction, Key, Keyboard};

    let failed = |error: enigo::InputError| PasteError::Failed {
        reason: format!("注入粘贴按键失败：{error}"),
    };
    enigo.key(modifier, Direction::Press).map_err(failed)?;
    let typed = enigo.key(Key::Unicode('v'), Direction::Click);
    let released = enigo.key(modifier, Direction::Release);
    typed.and(released).map_err(failed)
}

/// 面向用户的手动粘贴提示。自动粘贴不可用或失败时都给出这一句。
pub fn manual_paste_hint(os: crate::capability::OsKind) -> &'static str {
    match os {
        crate::capability::OsKind::Macos => "请切换到目标应用后按 Cmd+V 手动粘贴",
        _ => "请切换到目标应用后按 Ctrl+V 手动粘贴",
    }
}

/// 把「打算自动粘贴但没有条件」整理成一段准确的中文说明。
///
/// 宿主用它拼装 `CopiedNeedsManualPaste` 的反馈：先说清已复制，再说清为什么没有
/// 自动粘贴，最后给出用户接下来该做什么。任何一项缺失都会让用户以为粘贴成功了。
pub fn manual_paste_message(
    label: &str,
    os: crate::capability::OsKind,
    blocker: Option<&str>,
) -> String {
    match blocker {
        Some(reason) if !reason.is_empty() => format!(
            "已复制「{label}」到剪贴板；{reason}；{}",
            manual_paste_hint(os)
        ),
        _ => format!("已复制「{label}」到剪贴板；{}", manual_paste_hint(os)),
    }
}
