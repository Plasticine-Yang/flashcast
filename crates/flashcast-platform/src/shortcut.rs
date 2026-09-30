//! 全局快捷键注册。注册失败（例如快捷键已被其他应用占用，或会话不支持）必须
//! 作为可展示的错误返回给 UI，而不是静默忽略。

use std::sync::Arc;

use crate::hotkey::HotkeySpec;

/// 快捷键被按下时调用的回调。
///
/// 回调在**非主线程**上执行；实现方不得在回调内直接操作窗口，应转发到主线程
/// 或事件循环。
pub type PressCallback = Arc<dyn Fn() + Send + Sync + 'static>;

/// 已注册快捷键的句柄。`id` 由实现分配，`spec` 保留注册时的规格以便注销。
#[derive(Clone)]
pub struct HotkeyHandle {
    pub id: u64,
    pub spec: HotkeySpec,
    pub(crate) callback: PressCallback,
}

impl std::fmt::Debug for HotkeyHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotkeyHandle")
            .field("id", &self.id)
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl HotkeyHandle {
    pub fn new(id: u64, spec: HotkeySpec, callback: PressCallback) -> Self {
        Self { id, spec, callback }
    }

    /// 触发该快捷键的回调。供测试替身与诊断使用。
    pub fn fire(&self) {
        (self.callback)();
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("快捷键无效：{0}")]
    InvalidSpec(#[from] crate::hotkey::HotkeySpecError),
    #[error("快捷键 {spec} 已被 Flashcast 注册")]
    AlreadyRegistered { spec: String },
    #[error("快捷键 {spec} 已被其他应用占用，请在设置中改用其他组合")]
    Conflict { spec: String },
    #[error("当前环境无法注册全局快捷键：{reason}")]
    BackendUnavailable { reason: String },
    #[error("快捷键操作失败：{reason}")]
    Other { reason: String },
}

/// 注册、注销、更新全局快捷键。
pub trait HotkeyManager: Send + Sync {
    /// 注册 `spec`，成功时返回句柄，失败时返回可展示的原因。
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError>;

    /// 用新规格替换已注册的快捷键，返回新的句柄。
    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError>;

    /// 注销已注册的快捷键。重复注销不视为错误。
    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError>;
}
