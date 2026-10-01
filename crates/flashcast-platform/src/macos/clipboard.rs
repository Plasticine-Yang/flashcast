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

use std::path::PathBuf;
use std::sync::Mutex;

use crate::clipboard::{
    check_image_write, check_text, find_program, fingerprint, fingerprint_bytes, image_from_bytes,
    read_file_bounded, read_with_tool, run_tool, write_with_tool, ClipboardAccess,
    ClipboardCapture, ClipboardError, ClipboardFormatKind, ClipboardImage, ClipboardPoll,
    ClipboardWatcher, IMAGE_MIME_PNG, MAX_IMAGE_BYTES,
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

    /// 读取当前剪贴板里的图片，并把「有图片但无法保存」的原因一并带回。
    ///
    /// `pbpaste` **不支持**图片，因此这里走 `osascript`：`NSPasteboard` 只有
    /// Objective-C / AppleScript 可达（spec「保留与恢复按平台公开的可序列化格式
    /// 实现」）。脚本只在剪贴板里确有 `PNGf` 时成功，否则报错——也就是
    /// 「没有图片」，不是读取失败。
    pub fn read_image_detailed(
        &self,
    ) -> Result<(Option<ClipboardImage>, Option<String>), ClipboardError> {
        let Some(program) = find_program("osascript") else {
            return Err(ClipboardError::ToolMissing {
                reason: "未找到 /usr/bin/osascript，无法读取图片剪贴板".to_string(),
            });
        };
        let path = temp_image_path();
        let script = format!(
            "set theFile to (POSIX file \"{}\")\n\
             try\n\
             \tset theData to (the clipboard as «class PNGf»)\n\
             on error\n\
             \treturn \"none\"\n\
             end try\n\
             set theRef to (open for access theFile with write permission)\n\
             set eof theRef to 0\n\
             write theData to theRef\n\
             close access theRef\n\
             return \"ok\"",
            path.display()
        );
        let result = run_tool(&program, &["-e", &script]);
        let read = read_file_bounded(&path, MAX_IMAGE_BYTES as u64 + 1);
        let _ = std::fs::remove_file(&path);
        match result {
            // 脚本报错表示剪贴板里没有 PNG 图片：这是正常状态，不是读取失败。
            Ok((false, _)) => Ok((None, None)),
            Err(error) => Err(error),
            Ok((true, output)) => {
                if output.trim() != "ok" {
                    return Ok((None, None));
                }
                match read {
                    Ok(bytes) => Ok(image_from_bytes(IMAGE_MIME_PNG, bytes)),
                    Err(error) => Ok((None, Some(error.to_string()))),
                }
            }
        }
    }
}

/// 临时图片文件路径（`osascript` 只能在文件与剪贴板之间搬运图片）。
fn temp_image_path() -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "flashcast-clipboard-{}-{nanos:x}.png",
        std::process::id()
    ))
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

    /// 写入图片（ticket 10）：先把 PNG 写到临时文件，再让 `osascript` 把它放进剪贴板。
    ///
    /// AppleScript 只能从文件读取 `PNGf` 数据，没有可用的标准输入路径；临时文件在
    /// 成功或失败后都会被删除。
    fn write_image(&self, image: &ClipboardImage) -> Result<(), ClipboardError> {
        check_image_write(image)?;
        let program = find_program("osascript").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/osascript，无法写入图片剪贴板".to_string(),
        })?;
        let path = temp_image_path();
        std::fs::write(&path, &image.bytes).map_err(|error| {
            ClipboardError::Failed(format!("无法写入临时图片 {}：{error}", path.display()))
        })?;
        let script = format!(
            "set the clipboard to (read (POSIX file \"{}\") as «class PNGf»)",
            path.display()
        );
        let result = run_tool(&program, &["-e", &script]);
        let _ = std::fs::remove_file(&path);
        match result {
            Ok((true, _)) => Ok(()),
            Ok((false, message)) => Err(ClipboardError::Failed(format!(
                "osascript 无法把图片放进剪贴板：{}",
                message.trim()
            ))),
            Err(error) => Err(error),
        }
    }

    fn read_image(&self) -> Result<Option<ClipboardImage>, ClipboardError> {
        Ok(self.read_image_detailed()?.0)
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
        let text = MacosClipboard::new()
            .read_text()?
            .filter(|text| !text.is_empty());
        let (image, problem) = MacosClipboard::new().read_image_detailed()?;
        if text.is_none() && image.is_none() && problem.is_none() {
            return Ok(ClipboardPoll::Unchanged);
        }
        let print = match (&text, &image) {
            (Some(text), _) => fingerprint(text),
            (None, Some(image)) => fingerprint_bytes(&image.bytes),
            (None, None) => fingerprint(problem.as_deref().unwrap_or_default()),
        };
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
        let mut formats = Vec::new();
        if text.is_some() {
            formats.push(ClipboardFormatKind::Text);
        }
        if image.is_some() {
            formats.push(ClipboardFormatKind::Image);
        }
        Ok(ClipboardPoll::Changed(ClipboardCapture {
            formats,
            text,
            image,
            image_problem: problem,
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
