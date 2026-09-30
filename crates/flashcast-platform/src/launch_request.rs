//! 启动请求。与具体软件条目解耦，便于在测试中直接构造。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// 一次启动请求。`program` 与 `args` 已按平台规则分词，调用方不得再经过 shell。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchRequest {
    pub program: String,
    pub args: Vec<String>,
    /// 目标软件要求终端时，平台实现需要把它放进终端模拟器运行。
    pub terminal: bool,
    pub working_dir: Option<PathBuf>,
}

impl LaunchRequest {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            terminal: false,
            working_dir: None,
        }
    }

    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// 完整 argv：`program` 在首位，随后是参数。
    pub fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.program.clone());
        argv.extend(self.args.iter().cloned());
        argv
    }
}
