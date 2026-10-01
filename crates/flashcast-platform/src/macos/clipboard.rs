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
//!
//! 富文本：`pbcopy` / `pbpaste` 只处理纯文本，`-Prefer rtf` 在没有 RTF 风味时会退回它挑
//! 得到的内容、无法区分真伪，因此 macOS 上既不声称捕获、也不声称恢复 HTML/RTF。

use std::sync::Mutex;

use crate::clipboard::{
    check_text, find_program, fingerprint, read_with_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardContent, ClipboardError, ClipboardFormatKind, ClipboardPoll,
    ClipboardWatcher, ClipboardWriteReport,
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

    /// 恢复剪贴板历史：`pbcopy` 只接收纯文本，因此只提供文本并如实报告。
    ///
    /// `pbpaste -Prefer rtf` / `-Prefer ps` 存在，但对**写入**没有对应开关；读取侧也不
    /// 可靠：`pbpaste` 在没有该风味时会退回它挑得到的内容，无法区分「真的是 RTF」与
    /// 「拿到的其实是纯文本」。因此 macOS 上不声称保存或恢复了 HTML/RTF。
    fn write_content(
        &self,
        content: &ClipboardContent,
    ) -> Result<ClipboardWriteReport, ClipboardError> {
        check_text(&content.text)?;
        let program = find_program("pbcopy").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/pbcopy".to_string(),
        })?;
        write_with_tool(&program, &[], &content.text)?;
        Ok(ClipboardWriteReport::text_only(
            "pbcopy 只支持纯文本，富文本格式无法写回系统剪贴板",
            content
                .requested_formats()
                .into_iter()
                .filter(|kind| *kind != ClipboardFormatKind::Text),
        ))
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
            // pbpaste 不报告 HTML/RTF 风味究竟是不是真的，因此不声称保存了它们。
            html: None,
            rtf: None,
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
