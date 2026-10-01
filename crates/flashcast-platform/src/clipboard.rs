//! 剪贴板适配（ADR §5 的 `ClipboardAccess` 与 `ClipboardWatcher`）。
//!
//! ticket 07 只需要「把文本写进系统剪贴板」这一项：备忘录的默认操作是粘贴，而
//! 自动粘贴（ticket 08）尚未实现，因此先复制并如实提示手动粘贴。图片、HTML/RTF 与
//! 文件列表属于剪贴板历史（ticket 09/10），会在同一 trait 上继续增加方法。
//!
//! ticket 09 增加「监听」这一半：剪贴板历史需要在用户复制后把内容捕获下来（ADR §5 的
//! `ClipboardWatcher`）。可用的 Rust 剪贴板库（`arboard`）**不提供变化事件**，因此这里
//! 采用轮询（见 research `app-discovery-and-focus.md` 剪贴板一节）；各平台按自己的方式
//! 判断「变化」：
//!
//! - Windows：`GetClipboardSequenceNumber`（系统维护的单调序号，最可靠）；
//! - Linux / macOS：外部工具（`wl-paste` / `xclip` / `pbpaste`）没有序号可读，
//!   因此用**内容指纹**判断变化。代价是同一段文字被复制两次只算一次变化；这与
//!   「重复内容去重」的结果一致，不影响用户体验，但**不能**用真实适配器验证「去重」
//!   ——那一条由宿主的 `content_hash` 保证，并用替身在集成测试里覆盖。
//!
//! ## 自身写入抑制
//!
//! Flashcast 自己也会写剪贴板（粘贴备忘录或历史条目）。这些写入**不能**再被自己捕获：
//! 那会形成「粘贴 → 捕获 → 又出现一条 → 再粘贴」的循环，历史会被自己的内容填满
//! （spec 明确要求）。适配层因此提供 [`ClipboardWatcher::note_own_write`]：宿主在写
//! 剪贴板时登记这次写入，`poll` 不会再把它报告成一次新的复制事件。宿主另有一层基于
//! 内容指纹的抑制作为兜底（见 `flashcast-core` 的捕获管线），两层各自成立。
//!
//! ## 为什么走系统自带工具
//!
//! Linux 的能力探测（[`crate::linux::cap`]）本来就用 `wl-copy` / `xclip` / `xsel`
//! 判断剪贴板是否可用，这一层沿用同一组工具，就不需要为三种平台各引入一套原生
//! 绑定；工具缺失时如实返回「未找到」，而不是假装复制成功。Wayland 下 `wl-copy`、
//! X11 下 `xclip`/`xsel`、macOS 下 `pbcopy` 都是对应桌面环境的标准组件。
//!
//! Windows 没有等价的可靠命令行工具（`clip.exe` 按控制台代码页解析输入，中文会乱码），
//! 因此 Windows 使用 `Win32` 剪贴板 API。

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// 把文本写入系统剪贴板。属性读取与写入都需要原生能力，因此只在平台层实现。
pub trait ClipboardAccess: Send + Sync {
    /// 写入文本。失败原因为面向用户的中文描述。
    fn write_text(&self, text: &str) -> Result<(), ClipboardError>;

    /// 读取当前剪贴板里的文本。剪贴板为空（或格式不是文本）时返回 `Ok(None)`。
    ///
    /// 读取失败与「没有文本」是两回事：前者返回 `Err`，宿主据此给出准确的失败状态，
    /// 而不是把它当成「这次没有内容」而静默跳过。
    fn read_text(&self) -> Result<Option<String>, ClipboardError>;
}

/// 一次复制事件里存在的格式。v0.1.0 只捕获文本，其余取值是 ticket 10–12 的扩展点：
/// 捕获结构体会带上对应内容，而宿主的存储 schema 已经能容纳它们。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClipboardFormatKind {
    Text,
    Html,
    Rtf,
    Image,
    Files,
}

impl ClipboardFormatKind {
    /// 存储里的稳定标签（`clipboard_formats.kind`）。
    pub fn tag(self) -> &'static str {
        match self {
            ClipboardFormatKind::Text => "text",
            ClipboardFormatKind::Html => "html",
            ClipboardFormatKind::Rtf => "rtf",
            ClipboardFormatKind::Image => "image",
            ClipboardFormatKind::Files => "files",
        }
    }
}

/// 剪贴板内容的来源应用（平台能提供时）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardSourceApp {
    /// 稳定标识：X11 的 `WM_CLASS` 实例名 / Windows 的进程名 / macOS 的 bundle id。
    pub app_id: String,
    /// 面向用户的名称；无法获得时为 `app_id`。
    pub title: Option<String>,
}

/// 轮询到的一次新复制事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipboardCapture {
    /// 本次事件里存在的格式集合。
    pub formats: Vec<ClipboardFormatKind>,
    /// 文本内容（ticket 09 的唯一载荷）。ticket 10–12 会在同一结构体上增加
    /// HTML/RTF/图片/文件字段。
    pub text: Option<String>,
    /// 来源应用（平台可提供时）。
    pub source: Option<ClipboardSourceApp>,
}

impl ClipboardCapture {
    /// 只有文本的一次复制事件。
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            formats: vec![ClipboardFormatKind::Text],
            text: Some(text.into()),
            source: None,
        }
    }
}

/// 一次轮询的结果。区分「没有变化」与「变化了但内容不可捕获」，宿主才能给出准确状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClipboardPoll {
    /// 自上次轮询以来没有新的复制事件。
    Unchanged,
    /// 有新的复制事件。
    Changed(ClipboardCapture),
}

/// 剪贴板变化监听（ADR §5）。
///
/// 实现必须满足两条：
///
/// 1. `poll` **有界返回**：拿不到选区（自动化会话、Wayland 缺 `wl-clipboard`）时要如实
///    报错，不能挂住调用方；
/// 2. `note_own_write` 之后再 `poll` **不能**把那次写入报告成新的复制事件。
pub trait ClipboardWatcher: Send + Sync {
    /// 轮询一次剪贴板。
    fn poll(&self) -> Result<ClipboardPoll, ClipboardError>;

    /// 登记一次由 Flashcast 自己发起的剪贴板写入（自身写入抑制）。
    fn note_own_write(&self, text: &str);
}

/// 内容指纹：没有平台序号可读时用它判断剪贴板是否变化。
///
/// 只用标准库的 `DefaultHasher`，不引入额外依赖，也不需要密码学强度——它只用来
/// 判断「和上次读到的是不是同一段文字」。
pub fn fingerprint(text: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

/// 剪贴板操作的失败原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClipboardError {
    #[error("当前会话不支持写入剪贴板：{reason}")]
    Unsupported { reason: String },
    #[error("未找到可用的剪贴板工具：{reason}")]
    ToolMissing { reason: String },
    #[error("写入剪贴板失败：{0}")]
    Failed(String),
}

/// 文本写入的最大长度。剪贴板工具对超长文本没有明确上限，这里只做防呆。
pub const MAX_TEXT_BYTES: usize = 4 * 1024 * 1024;

/// 校验待写入的文本。
pub fn check_text(text: &str) -> Result<(), ClipboardError> {
    if text.trim().is_empty() {
        return Err(ClipboardError::Failed(
            "内容为空，没有可复制的东西".to_string(),
        ));
    }
    if text.len() > MAX_TEXT_BYTES {
        return Err(ClipboardError::Failed(format!(
            "内容超过 {} MB，已拒绝写入剪贴板",
            MAX_TEXT_BYTES / (1024 * 1024)
        )));
    }
    Ok(())
}

/// 在 `PATH` 中查找一个可执行文件。
pub(crate) fn find_program(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    // macOS 的最小 PATH（例如从 Finder 启动）常常看不到 /usr/bin，因此再查一遍
    // 系统目录，避免把「工具存在但不在 PATH 里」误报成缺失。
    for dir in ["/usr/bin", "/bin", "/usr/local/bin"] {
        let candidate = Path::new(dir).join(program);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// 等待外部剪贴板工具返回的上限。
///
/// 这些工具在正常桌面会话里立刻返回（它们 fork 出后台进程持有选区）。但在拿不到选区的
/// 会话里（例如自动化会话没有可用的输入序列）`wl-copy` 会**永久阻塞**——实测观察到的
/// 就是 5 分钟不返回。宿主执行「复制」时不能因此挂住，所以超过这个上限就杀掉进程并如实
/// 报告「剪贴板工具没有返回」，由宿主的降级路径给用户中文反馈。
pub const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// 从外部剪贴板工具读取文本时的等待上限。
///
/// 与写入同理：Wayland 的选区由持有者进程提供，自动化会话里 `wl-paste` 可能永远等不到
/// 内容。宿主在后台按固定间隔轮询，一旦某次读取卡住，整个捕获线程就再也不会前进，
/// 因此读取同样必须**有界**。
pub const READ_TIMEOUT: Duration = Duration::from_secs(3);

/// 用一个外部剪贴板工具写入文本：文本走标准输入。
///
/// 这些工具（`wl-copy`、`xclip`、`pbcopy`）都会 fork 出后台进程持有选区，父进程随后
/// 退出，因此必须显式关闭标准输入再 `wait`，避免写入端被 SIGPIPE 打断。
///
/// 等待是**有界**的（[`WRITE_TIMEOUT`]）。标准错误由独立线程读取：工具 fork 出的守护
/// 进程会继承这个管道，父进程退出后管道仍未关闭，若在主线程里读到 EOF 就会再次挂住。
pub(crate) fn write_with_tool(
    program: &Path,
    args: &[&str],
    text: &str,
) -> Result<(), ClipboardError> {
    check_text(text)?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ClipboardError::Failed(format!("无法启动 {}：{error}", program.display()))
        })?;
    {
        let stdin = child
            .stdin
            .as_mut()
            .ok_or_else(|| ClipboardError::Failed("无法写入剪贴板工具的标准输入".to_string()))?;
        stdin
            .write_all(text.as_bytes())
            .map_err(|error| ClipboardError::Failed(format!("写入剪贴板失败：{error}")))?;
    }
    // 关闭标准输入（drop 掉句柄）后再等待，工具才知道内容已经结束。
    drop(child.stdin.take());
    let stderr = child.stderr.take();
    let stderr_reader = std::thread::spawn(move || {
        let mut buffer = String::new();
        if let Some(mut stderr) = stderr {
            let _ = std::io::Read::read_to_string(&mut stderr, &mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + WRITE_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // 不 join 读取线程：它的管道可能永远不关闭；进程退出时线程自然消失。
                return Err(ClipboardError::Failed(format!(
                    "剪贴板工具 {} 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
                    program.display(),
                    WRITE_TIMEOUT.as_secs()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(ClipboardError::Failed(format!(
                    "等待剪贴板工具失败：{error}"
                )));
            }
        }
    };
    let stderr = stderr_reader.join().unwrap_or_default();
    if !status.success() {
        let stderr = stderr.trim();
        return Err(ClipboardError::Failed(if stderr.is_empty() {
            format!("剪贴板工具 {} 以状态 {status} 退出", program.display())
        } else {
            format!("剪贴板工具 {} 失败：{stderr}", program.display())
        }));
    }
    Ok(())
}

/// 用一个外部剪贴板工具读取文本：内容走标准输出。
///
/// 与 [`write_with_tool`] 一样是**有界**的，并且用独立线程读取标准输出：读取端在主线程
/// 里等到 EOF 会再次挂住（选区持有者可能一直不关管道）。工具超时会被杀掉并如实报错。
pub(crate) fn read_with_tool(
    program: &Path,
    args: &[&str],
) -> Result<Option<String>, ClipboardError> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| {
            ClipboardError::Failed(format!("无法启动 {}：{error}", program.display()))
        })?;
    let stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut buffer = Vec::new();
        if let Some(mut stdout) = stdout {
            let _ = std::io::Read::read_to_end(&mut stdout, &mut buffer);
        }
        buffer
    });

    let deadline = Instant::now() + READ_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                // 不 join 读取线程：它的管道可能永远不关闭；进程退出时线程自然消失。
                return Err(ClipboardError::Failed(format!(
                    "剪贴板工具 {} 超过 {} 秒没有返回，已中止（当前会话可能无法取得剪贴板选区）",
                    program.display(),
                    READ_TIMEOUT.as_secs()
                )));
            }
            Err(error) => {
                let _ = child.kill();
                return Err(ClipboardError::Failed(format!(
                    "等待剪贴板工具失败：{error}"
                )));
            }
        }
    };
    let bytes = reader.join().unwrap_or_default();
    if !status.success() {
        // 工具以非 0 退出通常表示「当前剪贴板里没有这种格式」，按「没有文本」处理，
        // 而不是报错——空的剪贴板是正常状态。
        return Ok(None);
    }
    if bytes.is_empty() {
        return Ok(None);
    }
    Ok(Some(String::from_utf8_lossy(&bytes).into_owned()))
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// 工具卡住时必须**有界**返回，而不是让宿主的执行入口挂死。
    ///
    /// 这里用真实的 `sh -c 'sleep 30'` 当替身：它是真的会阻塞的进程，不依赖任何平台替身。
    #[test]
    fn blocking_tool_is_aborted_after_the_timeout() {
        let started = Instant::now();
        let result = write_with_tool(Path::new("/bin/sh"), &["-c", "sleep 30"], "正文");
        let elapsed = started.elapsed();
        let error = result.expect_err("阻塞的工具必须被判定为失败");
        assert!(
            error.to_string().contains("没有返回"),
            "失败原因必须说明工具没有返回：{error}"
        );
        assert!(
            elapsed < WRITE_TIMEOUT * 3,
            "必须在超时附近返回，实际耗时 {elapsed:?}"
        );
    }

    /// 读取同样必须有界：后台轮询线程一旦卡死，剪贴板历史就再也不会前进。
    #[test]
    fn blocking_read_tool_is_aborted_after_the_timeout() {
        let started = Instant::now();
        let result = read_with_tool(Path::new("/bin/sh"), &["-c", "sleep 30"]);
        let elapsed = started.elapsed();
        let error = result.expect_err("阻塞的读取必须被判定为失败");
        assert!(
            error.to_string().contains("没有返回"),
            "失败原因必须说明工具没有返回：{error}"
        );
        assert!(
            elapsed < READ_TIMEOUT * 3,
            "必须在超时附近返回，实际耗时 {elapsed:?}"
        );
    }

    /// 正常读取返回工具的标准输出；空输出按「剪贴板里没有文本」处理，不是错误。
    #[test]
    fn read_tool_returns_output_and_treats_empty_as_none() {
        let text = read_with_tool(Path::new("/bin/sh"), &["-c", "printf '来自剪贴板'"])
            .expect("正常读取不应失败");
        assert_eq!(text.as_deref(), Some("来自剪贴板"));
        let empty = read_with_tool(Path::new("/bin/sh"), &["-c", ":"]).expect("空输出不应失败");
        assert_eq!(empty, None);
        // 工具以非 0 退出表示「当前剪贴板没有这种格式」，同样不是错误。
        let missing =
            read_with_tool(Path::new("/bin/sh"), &["-c", "exit 1"]).expect("非 0 退出不应失败");
        assert_eq!(missing, None);
    }
}
