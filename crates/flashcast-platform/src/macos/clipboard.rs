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

use std::path::PathBuf;
use std::sync::Mutex;

use crate::clipboard::{
    check_files, check_image_write, check_text, file_entries, find_program, fingerprint,
    fingerprint_bytes, fingerprint_files, image_from_bytes, read_file_bounded, read_with_tool,
    run_tool, write_with_tool, ClipboardAccess, ClipboardCapture, ClipboardContent, ClipboardError,
    ClipboardFormatKind, ClipboardImage, ClipboardPoll, ClipboardWatcher, ClipboardWriteReport,
    IMAGE_MIME_PNG, MAX_IMAGE_BYTES,
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

/// 读取文件列表的 AppleScript：剪贴板里是文件 URL 列表时逐行输出 POSIX 路径。
///
/// 用 `osascript` 而不是 objc2 绑定：这一层已经是「外部工具」实现，而 `NSPasteboard`
/// 的 `readObjects(forClasses:)` 需要 `NSURL` 桥接与 autorelease 语义，为一个读取路径
/// 引入一整棵依赖不划算。路径作为**参数**（`argv`）传入 / 由脚本输出，从不拼进 shell
/// 命令，因此没有注入面。
const READ_FILES_SCRIPT: &str = r#"try
  set theItems to (the clipboard as «class furl»)
  set out to ""
  if class of theItems is list then
    repeat with item_ in theItems
      set out to out & (POSIX path of item_) & linefeed
    end repeat
  else
    set out to (POSIX path of theItems) & linefeed
  end if
  return out
on error
  return ""
end try"#;

/// 写入文件列表的 AppleScript：`argv` 就是文件的 POSIX 路径。
///
/// `set the clipboard to` 一组 `POSIX file` 会把文件 URL 列表放进剪贴板，Finder 与
/// 文件选择对话框都能粘贴（v0.1.0 的视频文件也走这一条）。
const WRITE_FILES_SCRIPT: &str = r#"on run argv
  set theFiles to {}
  repeat with path_ in argv
    set the end of theFiles to (POSIX file (path_ as text))
  end repeat
  set the clipboard to theFiles
end run"#;

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

    fn write_files(&self, paths: &[PathBuf]) -> Result<(), ClipboardError> {
        check_files(paths)?;
        let program = find_program("osascript").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/osascript".to_string(),
        })?;
        let mut command = std::process::Command::new(program);
        // `-e` 是脚本，其余参数按顺序成为 AppleScript 的 `argv`；路径原样传递，
        // 不经过 shell，也不拼进脚本正文。
        command.arg("-e").arg(WRITE_FILES_SCRIPT);
        for path in paths {
            command.arg(path);
        }
        run_bounded_command(command, crate::clipboard::WRITE_TIMEOUT)
    }

    fn read_files(&self) -> Result<Option<Vec<PathBuf>>, ClipboardError> {
        let program = find_program("osascript").ok_or_else(|| ClipboardError::ToolMissing {
            reason: "未找到 /usr/bin/osascript".to_string(),
        })?;
        let mut command = std::process::Command::new(program);
        command.arg("-e").arg(READ_FILES_SCRIPT);
        let text = read_bounded_command(command, crate::clipboard::READ_TIMEOUT)?;
        let paths: Vec<PathBuf> = text
            .lines()
            .map(str::trim_end)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect();
        if paths.is_empty() {
            return Ok(None);
        }
        Ok(Some(paths))
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

/// 有界运行一个命令，成功时返回标准输出（解码为 UTF-8）。
///
/// 与文本路径共用同一套「不设上限就可能永久挂住」的判断（见 [`crate::clipboard`]）。
fn read_bounded_command(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<String, ClipboardError> {
    use std::io::Read;
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| ClipboardError::ReadFailed(format!("无法启动 osascript：{error}")))?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stdout) = stdout {
            let _ = stdout.read_to_end(&mut buffer);
        }
        buffer
    });
    wait_bounded(&mut child, timeout, read_timeout)?;
    let bytes = reader.join().unwrap_or_default();
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// 有界运行一个命令，只关心成败。
fn run_bounded_command(
    mut command: std::process::Command,
    timeout: std::time::Duration,
) -> Result<(), ClipboardError> {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| ClipboardError::Failed(format!("无法启动 osascript：{error}")))?;
    wait_bounded(&mut child, timeout, write_timeout)
}

/// 读取侧的「没有返回」失败原因。
fn read_timeout(timeout: std::time::Duration) -> ClipboardError {
    ClipboardError::ReadFailed(format!(
        "osascript 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
        timeout.as_secs()
    ))
}

/// 写入侧的「没有返回」失败原因。
fn write_timeout(timeout: std::time::Duration) -> ClipboardError {
    ClipboardError::Failed(format!(
        "osascript 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
        timeout.as_secs()
    ))
}

/// 等待子进程，超时则杀掉并如实报错。
fn wait_bounded(
    child: &mut std::process::Child,
    timeout: std::time::Duration,
    error: fn(std::time::Duration) -> ClipboardError,
) -> Result<(), ClipboardError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return if status.success() {
                    Ok(())
                } else {
                    Err(ClipboardError::Failed(format!(
                        "osascript 以状态 {status} 退出"
                    )))
                }
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error(timeout));
            }
            Err(error) => {
                return Err(ClipboardError::Failed(format!(
                    "等待 osascript 失败：{error}"
                )))
            }
        }
    }
}

/// macOS 的剪贴板变化监听（内容指纹 + 自身写入抑制）。
pub struct MacosClipboardWatcher {
    last: Mutex<Option<u64>>,
    own: Mutex<Vec<u64>>,
    last_files: Mutex<Option<u64>>,
    own_files: Mutex<Vec<u64>>,
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
            last_files: Mutex::new(None),
            own_files: Mutex::new(Vec::new()),
        }
    }
}

impl ClipboardWatcher for MacosClipboardWatcher {
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError> {
        // 文件列表优先：Finder 复制文件时剪贴板里是文件 URL 列表，不是文本。
        let files = MacosClipboard::new().read_files()?;
        if let Some(paths) = files {
            let print = fingerprint_files(&paths);
            {
                let mut own = self.own_files.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(index) = own.iter().position(|item| *item == print) {
                    own.remove(index);
                    *self.last_files.lock().unwrap_or_else(|p| p.into_inner()) = Some(print);
                    return Ok(ClipboardPoll::Unchanged);
                }
            }
            {
                let mut last = self.last_files.lock().unwrap_or_else(|p| p.into_inner());
                if *last == Some(print) {
                    return Ok(ClipboardPoll::Unchanged);
                }
                *last = Some(print);
            }
            return Ok(ClipboardPoll::Changed(ClipboardCapture {
                formats: vec![ClipboardFormatKind::Files],
                text: None,
                image: None,
                image_problem: None,
                files: file_entries(&paths),
                html: None,
                rtf: None,
                source: None,
            }));
        }
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
            files: Vec::new(),
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

    fn note_own_write_files(&self, paths: &[PathBuf]) {
        if paths.is_empty() {
            return;
        }
        self.own_files
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(fingerprint_files(paths));
    }
}
