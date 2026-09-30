//! macOS 剪贴板文本写入。
//!
//! `pbcopy` 是 macOS 自带的标准剪贴板工具（`/usr/bin/pbcopy`），从标准输入读取文本并
//! 写入通用剪贴板，因此这一层与 Linux 侧共用同一套「外部工具」实现，不需要额外的
//! Objective-C 绑定。`find_program` 会额外查 `/usr/bin`，覆盖从 Finder 启动时 PATH
//! 极简的情况。

use crate::clipboard::{
    check_text, find_program, write_with_tool, ClipboardAccess, ClipboardError,
};

/// macOS 的文本剪贴板后端。
pub struct MacosClipboard;

impl Default for MacosClipboard {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosClipboard {
    pub fn new() -> Self {
        Self
    }

    /// 当前会使用的工具路径；不可用时给出原因。诊断用。
    pub fn backend_name(&self) -> Result<&'static str, ClipboardError> {
        match find_program("pbcopy") {
            Some(_) => Ok("pbcopy"),
            None => Err(ClipboardError::ToolMissing {
                reason: "未找到 /usr/bin/pbcopy".to_string(),
            }),
        }
    }
}

impl ClipboardAccess for MacosClipboard {
    fn write_text(&self, text: &str) -> Result<(), ClipboardError> {
        check_text(text)?;
        let program = find_program("pbcopy").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/pbcopy".to_string(),
        })?;
        write_with_tool(&program, &[], text)
    }
}
