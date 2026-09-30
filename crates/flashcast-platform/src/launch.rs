//! 启动软件。实现必须按 argv 直接 `exec`，不允许经过 `sh -c`。

use serde::{Deserialize, Serialize};

use crate::launch_request::LaunchRequest;

/// 一次成功启动的结果。`pid` 在平台无法提供时为 `None`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchReceipt {
    pub pid: Option<u32>,
    /// 实际传给系统的完整 argv，便于诊断与日志。
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LaunchError {
    #[error("该软件没有可执行的启动命令")]
    EmptyCommand,
    #[error("找不到可执行文件 {program}")]
    ProgramNotFound { program: String },
    #[error("启动 {program} 失败：{reason}")]
    Spawn { program: String, reason: String },
    #[error("当前平台尚未实现软件启动")]
    Unsupported,
}

/// 启动目标软件并报告失败。
pub trait AppLauncher: Send + Sync {
    fn launch(&self, request: &LaunchRequest) -> Result<LaunchReceipt, LaunchError>;
}

/// 启动命令中是否含有会被 shell 解释的元字符。
///
/// 实现按 argv 直接执行，不使用 shell；该函数仅用于测试与诊断，
/// 证明含元字符的参数无需转义即可安全传递。
pub fn contains_shell_metacharacters(arg: &str) -> bool {
    arg.chars()
        .any(|c| matches!(c, ';' | '|' | '&' | '$' | '`' | '>' | '<' | '*' | '?' | '\n' | '(' | ')'))
}
