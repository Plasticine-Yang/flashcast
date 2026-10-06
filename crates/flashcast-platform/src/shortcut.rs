//! 全局快捷键注册。注册失败（例如快捷键已被其他应用占用，或会话不支持）必须
//! 作为可展示的错误返回给 UI，而不是静默忽略。

use std::sync::Arc;

use crate::hotkey::HotkeySpec;

/// 一次快捷键激活携带的平台凭据。
#[derive(Debug, Clone, Default)]
pub struct HotkeyActivation {
    /// Wayland 合成器签发的窗口激活凭据；仅供这次唤起使用。
    pub token: Option<String>,
}

/// 回调在非主线程执行；窗口操作须转发到主线程。
pub type PressCallback = Arc<dyn Fn(HotkeyActivation) + Send + Sync + 'static>;

/// 已注册快捷键的句柄。`id` 由实现分配，`spec` 保留注册时的规格以便注销。
#[derive(Clone)]
pub struct HotkeyHandle {
    pub id: u64,
    pub spec: HotkeySpec,
    /// 门户实际绑定的按键说明，可能与首选组合不同。
    pub trigger_description: Option<String>,
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
        Self {
            id,
            spec,
            callback,
            trigger_description: None,
        }
    }

    /// 触发该快捷键的回调。供测试替身与诊断使用。
    pub fn fire(&self) {
        (self.callback)(HotkeyActivation::default());
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("快捷键无效：{0}")]
    InvalidSpec(#[from] crate::hotkey::HotkeySpecError),
    #[error("快捷键 {spec} 已被 Flashcast 注册")]
    AlreadyRegistered { spec: String },
    #[error("快捷键 {spec} 已被占用，无法确认占用者，请在设置中改用其他组合")]
    Conflict { spec: String },
    #[error("当前环境无法注册全局快捷键：{reason}")]
    BackendUnavailable { reason: String },
    #[error("快捷键操作失败：{reason}")]
    Other { reason: String },
}

/// 系统占用检测。Unknown 与不适用分开，避免把无法检测当作没有冲突。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyConflictReport {
    pub status: String,
    pub can_resolve: bool,
    pub can_undo: bool,
    pub effective: Option<String>,
    pub message: Option<String>,
}
impl Default for HotkeyConflictReport {
    fn default() -> Self {
        Self {
            status: "not-applicable".into(),
            can_resolve: false,
            can_undo: false,
            effective: None,
            message: None,
        }
    }
}

/// 注册、注销、更新全局快捷键。
pub trait HotkeyManager: Send + Sync {
    /// 只读检测，不申请授权或修改桌面设置。撤销记录只保存在本机。
    fn inspect_conflict(
        &self,
        _spec: &HotkeySpec,
        _backup: &std::path::Path,
    ) -> HotkeyConflictReport {
        HotkeyConflictReport::default()
    }
    /// 仅在用户确认后调用；失败须回滚并给出手动处理原因。
    fn resolve_conflict(
        &self,
        _spec: &HotkeySpec,
        _backup: &std::path::Path,
        _undo: bool,
    ) -> Result<HotkeyConflictReport, String> {
        Err("当前桌面不支持自动处理，请在系统快捷键设置中修改。".into())
    }

    /// 注册 `spec`，成功时返回句柄，失败时返回可展示的原因。
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError>;

    /// 命令身份用于系统门户区分多个插件入口。
    fn register_command(
        &self,
        spec: &HotkeySpec,
        _id: &str,
        _title: &str,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        self.register(spec, on_press)
    }

    /// 用新规格替换已注册的快捷键，返回新的句柄。
    fn update(&self, handle: &HotkeyHandle, spec: &HotkeySpec)
        -> Result<HotkeyHandle, HotkeyError>;

    /// 注销已注册的快捷键。重复注销不视为错误。
    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError>;
}
