//! macOS 剪贴板读写与变化监听。
//!
//! `pbcopy` / `pbpaste` 是 macOS 自带的标准剪贴板工具（`/usr/bin/`），因此这一层与
//! Linux 侧共用同一套「外部工具」实现，不需要额外的 Objective-C 绑定。`find_program`
//! 会额外查 `/usr/bin`，覆盖从 Finder 启动时 PATH 极简的情况。
//!
//! 变化监听用**内容指纹**：`pbpaste` 不提供 `NSPasteboard` 的 `changeCount`，而为了一个
//! 序号引入 objc2 绑定并不划算（该模块在所有目标上编译，见 `macos/mod.rs`）。
//! 代价是同一段文字被复制两次只算一次变化；与去重结果一致，不影响体验。
//!
//! 来源应用在 macOS 上拿不到（`pbpaste` 不报告来源，`NSPasteboard` 也没有公开接口），
//! 因此如实留空，而不是猜一个。

use std::sync::Mutex;

use crate::clipboard::{
    check_text, find_program, fingerprint, read_with_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardError, ClipboardFormatKind, ClipboardPoll, ClipboardWatcher,
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

    /// 当前会使用的写入工具路径；不可用时给出原因。诊断用。
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

    fn read_text(&self) -> Result<Option<String>, ClipboardError> {
        let program = find_program("pbpaste").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/pbpaste".to_string(),
        })?;
        read_with_tool(&program, &[])
    }
}

/// macOS 的剪贴板变化监听（内容指纹 + 自身写入抑制）。
pub struct MacosClipboardWatcher {
    last: Mutex<Option<u64>>,
    own: Mutex<Vec<u64>>,
}

impl Default for MacosClipboardWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl MacosClipboardWatcher {
    pub fn new() -> Self {
        Self {
            last: Mutex::new(None),
            own: Mutex::new(Vec::new()),
        }
    }
}

impl ClipboardWatcher for MacosClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        let Some(text) = MacosClipboard::new().read_text()? else {
            return Ok(ClipboardPoll::Unchanged);
        };
        let print = fingerprint(&text);
        {
            let mut own = self.own.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(index) = own.iter().position(|item| *item == print) {
                own.remove(index);
                *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(print);
                return Ok(ClipboardPoll::Unchanged);
            }
        }
        {
            let mut last = self.last.lock().unwrap_or_else(|p| p.into_inner());
            if *last == Some(print) {
                return Ok(ClipboardPoll::Unchanged);
            }
            *last = Some(print);
        }
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats: vec![ClipboardFormatKind::Text],
            text: Some(text),
            // 来源应用在 macOS 上不可得：如实留空。
            source: None,
        }))
    }

    fn note_own_write(&self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.own
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fingerprint(text));
    }
}
