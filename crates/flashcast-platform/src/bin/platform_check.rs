//! 真实 Linux 平台检查。
//!
//! 在真实机器上运行，输出结构化的能力与诊断报告，区分
//! **实测通过 / 实测失败 / 未覆盖**。该二进制不做任何模拟：
//! 它调用的是 `flashcast-platform` 的真实 Linux 实现。
//!
//! 用法：
//!   cargo run -p flashcast-platform --bin flashcast-platform-check
//!   cargo run -p flashcast-platform --bin flashcast-platform-check -- --json
//!   cargo run -p flashcast-platform --bin flashcast-platform-check -- --json --output report.json
//!
//! 注意：在 Wayland 会话下，全局快捷键与焦点读取会被判定为「未覆盖」，
//! 这是刻意的：X11 下（含 XWayland）的抓取成功不能证明 Wayland 下可用。

use flashcast_platform::capability::Capabilities;

// 以下都只用于 Linux 的真实检查；非 Linux 目标上这些检查一律报告「未覆盖」。
#[cfg(target_os = "linux")]
use flashcast_platform::capability::{CapabilityProbe, SessionType, Support};
#[cfg(target_os = "linux")]
use flashcast_platform::focus::FocusTracker;
#[cfg(target_os = "linux")]
use flashcast_platform::hotkey::HotkeySpec;
#[cfg(target_os = "linux")]
use flashcast_platform::linux::{
    LinuxAppCatalog, LinuxCapabilityProbe, LinuxFocusTracker, LinuxHotkeyManager,
};
#[cfg(target_os = "linux")]
use flashcast_platform::shortcut::HotkeyManager;
#[cfg(target_os = "linux")]
use std::sync::Arc;
#[cfg(target_os = "linux")]
use std::time::Duration;

// macOS 的真实检查全部经 `flashcast_platform::macos`，不经过任何替身。
#[cfg(target_os = "macos")]
use flashcast_platform::capability::{
    CapabilityProbe as MacosCapabilityProbeTrait, Support as MacosSupport,
};
#[cfg(target_os = "macos")]
use flashcast_platform::focus::FocusTracker as MacosFocusTrackerTrait;
#[cfg(target_os = "macos")]
use flashcast_platform::hotkey::HotkeySpec as MacosHotkeySpec;
#[cfg(target_os = "macos")]
use flashcast_platform::macos::catalog::MacosAppCatalog;
#[cfg(target_os = "macos")]
use flashcast_platform::macos::focus::{
    accessibility_granted, MacosFocusTracker, ACCESSIBILITY_SETTINGS_LABEL,
};
#[cfg(target_os = "macos")]
use flashcast_platform::macos::hotkeys::MacosHotkeyManager;
#[cfg(target_os = "macos")]
use flashcast_platform::macos::icons::{self as macos_icons, ICON_POINT_SIZE};
#[cfg(target_os = "macos")]
use flashcast_platform::shortcut::{
    HotkeyError as MacosHotkeyError, HotkeyManager as MacosHotkeyManagerTrait,
};
#[cfg(target_os = "macos")]
use std::sync::Arc as MacosArc;

// 以下是 Windows 真实检查所需的导入。
#[cfg(target_os = "windows")]
use flashcast_platform::capability::{CapabilityProbe, Support};
#[cfg(target_os = "windows")]
use flashcast_platform::focus::FocusTracker;
#[cfg(target_os = "windows")]
use flashcast_platform::hotkey::HotkeySpec;
#[cfg(target_os = "windows")]
use flashcast_platform::launch::AppLauncher;
#[cfg(target_os = "windows")]
use flashcast_platform::launch_request::LaunchRequest;
#[cfg(target_os = "windows")]
use flashcast_platform::shortcut::HotkeyManager;
#[cfg(target_os = "windows")]
use flashcast_platform::windows::{
    WindowsAppCatalog, WindowsCapabilityProbe, WindowsFocusTracker, WindowsHotkeyManager,
    WindowsLauncher,
};
#[cfg(target_os = "windows")]
use std::sync::Arc;

/// 只在 macOS 等尚未实现真实适配的平台上报告「未覆盖」时给出的复现命令提示。
#[cfg(target_os = "macos")]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform --target x86_64-apple-darwin";
#[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform";

/// 探测当前环境能力。Linux、Windows 与 macOS 用各自的真实探测器，其余平台用
/// 「不支持」桩实现。
fn probe_capabilities() -> Capabilities {
    #[cfg(target_os = "linux")]
    {
        LinuxCapabilityProbe::new().probe()
    }
    #[cfg(target_os = "macos")]
    {
        flashcast_platform::macos::MacosCapabilityProbe::new().probe()
    }
    #[cfg(target_os = "windows")]
    {
        WindowsCapabilityProbe::new().probe()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        flashcast_platform::current().capabilities.probe()
    }
}

/// 检查结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    MeasuredPass,
    MeasuredFail,
    NotCovered,
}

impl Status {
    fn key(self) -> &'static str {
        match self {
            Status::MeasuredPass => "measured_pass",
            Status::MeasuredFail => "measured_fail",
            Status::NotCovered => "not_covered",
        }
    }

    fn label_zh(self) -> &'static str {
        match self {
            Status::MeasuredPass => "实测通过",
            Status::MeasuredFail => "实测失败",
            Status::NotCovered => "未覆盖",
        }
    }
}

struct CheckResult {
    id: &'static str,
    title: &'static str,
    status: Status,
    /// 面向人的结果说明（中文）。
    detail: String,
    /// 复现该检查所需的命令。
    command: String,
}

fn json_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    for c in value.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[derive(Default)]
struct Options {
    json: bool,
    output: Option<String>,
    /// 允许检查真的写一次系统剪贴板（会覆盖用户当前剪贴板里的内容）。
    allow_clipboard_write: bool,
}

fn parse_args() -> Options {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--output" => options.output = args.next(),
            "--allow-clipboard-write" => options.allow_clipboard_write = true,
            _ => {}
        }
    }
    options
}

fn main() {
    let options = parse_args();
    let capabilities = probe_capabilities();
    let mut checks = Vec::new();

    // 1. 编译与运行：能执行到这里说明已经编译。
    checks.push(CheckResult {
        id: "build.compile",
        title: "flashcast-platform 编译",
        status: Status::MeasuredPass,
        detail: format!(
            "{} {} 上编译并运行成功",
            capabilities.os.as_str(),
            capabilities.arch
        ),
        command: compile_command(),
    });

    // 2~8 依赖各平台的具体实现，因此按平台分开：
    // Linux（ticket 01）与 macOS（ticket 03）走真实实现；
    // Windows 等其余目标由后续平台 ticket 提供，这里如实报告「未覆盖」。
    #[cfg(target_os = "linux")]
    {
        // 2. 会话类型判定。
        checks.push(CheckResult {
            id: "session.detect",
            title: "桌面会话类型判定",
            status: if capabilities.session == SessionType::Unknown {
                Status::MeasuredFail
            } else {
                Status::MeasuredPass
            },
            detail: format!(
                "XDG_SESSION_TYPE={:?}，判定为 {}；有 DISPLAY={}，WAYLAND_DISPLAY={}",
                std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "<未设置>".to_string()),
                capabilities.session.label_zh(),
                std::env::var("DISPLAY")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
                std::env::var("WAYLAND_DISPLAY")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false),
            ),
            command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
        });

        // 3. 软件发现。
        let catalog = LinuxAppCatalog::new();
        let dirs = catalog.existing_application_dirs();
        let scan = catalog.scan_detailed();
        let app_count = scan.entries.len();
        let with_icons = scan
            .entries
            .iter()
            .filter(|e| e.icon.as_ref().and_then(|i| i.path.as_ref()).is_some())
            .count();
        let (app_status, app_detail) = if dirs.is_empty() {
            (
                Status::NotCovered,
                "未找到任何应用目录（可能不是桌面环境）".to_string(),
            )
        } else if app_count == 0 {
            (
                Status::MeasuredFail,
                format!("目录存在但未发现任何可启动软件：{dirs:?}"),
            )
        } else {
            (
                Status::MeasuredPass,
                format!(
                    "发现 {app_count} 个可启动软件，其中 {with_icons} 个解析到图标文件；\
                     扫描 {} 个 .desktop 文件，跳过 {} 个；应用目录 {} 个，图标索引主题 {} 个",
                    scan.files_seen,
                    scan.skipped.len(),
                    dirs.len(),
                    scan.icon_themes_seen,
                ),
            )
        };
        checks.push(CheckResult {
            id: "apps.discovery",
            title: "freedesktop 软件发现",
            status: app_status,
            detail: app_detail,
            command: "cargo run -p flashcast-platform --bin flashcast-platform-check -- --json"
                .to_string(),
        });

        // 4. 焦点读取。
        let focus = LinuxFocusTracker::new();
        let focus_check = match focus.capture() {
            Ok(app) => CheckResult {
                id: "focus.capture",
                title: "读取唤起前前台应用",
                status: Status::MeasuredPass,
                detail: format!(
                    "读取到 id={} name={} wm_class={:?} pid={:?} window={:?}",
                    app.id, app.name, app.wm_class, app.pid, app.window
                ),
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check"
                    .to_string(),
            },
            Err(flashcast_platform::focus::FocusError::Unsupported { reason }) => CheckResult {
                id: "focus.capture",
                title: "读取唤起前前台应用",
                status: Status::NotCovered,
                detail: reason,
                command: "FLASHCAST_FORCE_X11_BACKEND=1 cargo run -p flashcast-platform --bin \
                          flashcast-platform-check"
                    .to_string(),
            },
            Err(error) => CheckResult {
                id: "focus.capture",
                title: "读取唤起前前台应用",
                status: Status::MeasuredFail,
                detail: error.to_string(),
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check"
                    .to_string(),
            },
        };
        checks.push(focus_check);

        // 5. 全局快捷键注册。
        let hotkeys = LinuxHotkeyManager::new();
        let spec = HotkeySpec::parse("Ctrl+Alt+F12").expect("固定检查用快捷键必须可解析");
        let hotkey_check = match hotkeys.register(&spec, Arc::new(|| {})) {
            Ok(handle) => {
                let _ = hotkeys.unregister(&handle);
                CheckResult {
                    id: "hotkey.register",
                    title: "全局快捷键注册",
                    status: Status::MeasuredPass,
                    detail: format!("注册并注销 {} 成功", spec.canonical()),
                    command: "cargo run -p flashcast-platform --bin flashcast-platform-check"
                        .to_string(),
                }
            }
            Err(flashcast_platform::shortcut::HotkeyError::BackendUnavailable { reason }) => {
                CheckResult {
                    id: "hotkey.register",
                    title: "全局快捷键注册",
                    status: Status::NotCovered,
                    detail: reason,
                    command: "FLASHCAST_FORCE_X11_BACKEND=1 cargo run -p flashcast-platform --bin \
                              flashcast-platform-check"
                        .to_string(),
                }
            }
            Err(error) => CheckResult {
                id: "hotkey.register",
                title: "全局快捷键注册",
                status: Status::MeasuredFail,
                detail: error.to_string(),
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check"
                    .to_string(),
            },
        };
        checks.push(hotkey_check);

        // 6a. 剪贴板：能力报告（只有环境前提）与一次**真实**的写入 + 回读。
        checks.push(CheckResult {
            id: "clipboard.text",
            title: "剪贴板文字读写",
            status: match &capabilities.clipboard {
                Support::Supported => Status::MeasuredPass,
                Support::Unsupported { .. } => Status::MeasuredFail,
                Support::Unknown { .. } => Status::NotCovered,
            },
            detail: match &capabilities.clipboard {
                Support::Supported => "支持".to_string(),
                Support::Unsupported { reason } | Support::Unknown { reason } => reason.clone(),
            },
            command: "cargo run -p flashcast-platform --bin flashcast-platform-check                       -- --allow-clipboard-write"
                .to_string(),
        });
        checks.push(clipboard_write_check(
            &capabilities,
            options.allow_clipboard_write,
        ));

        // 6b. 自动粘贴：能力结论 + 注入前置条件（真实查询 XTEST / 会话类型）。
        checks.push(CheckResult {
            id: "paste.auto",
            title: "自动粘贴到唤起前应用",
            status: match &capabilities.auto_paste {
                Support::Supported => Status::MeasuredPass,
                Support::Unsupported { .. } => Status::MeasuredFail,
                Support::Unknown { .. } => Status::NotCovered,
            },
            detail: match &capabilities.auto_paste {
                Support::Supported => "支持：X11 + XTEST 可用，将注入 Ctrl+V".to_string(),
                Support::Unsupported { reason } | Support::Unknown { reason } => reason.clone(),
            },
            command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
        });
        checks.push(paste_prepare_check());

        // 7. X11 诊断（即便在 Wayland 下也如实报告 XWayland 暴露的内容）。
        let x11 = flashcast_platform::linux::x11::diagnostics();
        checks.push(CheckResult {
            id: "x11.diagnostics",
            title: "X11/EWMH 诊断",
            status: match x11.availability {
                flashcast_platform::linux::x11::X11Availability::Available => Status::MeasuredPass,
                _ => Status::NotCovered,
            },
            detail: format!(
                "可用性={:?}，窗口管理器={:?}，_NET_CLIENT_LIST 窗口数={:?}，前台窗口={:?}",
                x11.availability,
                x11.window_manager,
                x11.client_count,
                x11.active.as_ref().and_then(|a| a.identifier()),
            ),
            command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
        });
    }

    #[cfg(target_os = "macos")]
    {
        checks.extend(macos_checks(&capabilities));
    }

    #[cfg(target_os = "windows")]
    {
        checks.extend(windows_checks(&capabilities));
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        checks.extend(uncovered_checks(&capabilities));
    }

    if options.json {
        print_json(&capabilities, &checks, &options);
    } else {
        print_human(&capabilities, &checks);
    }
}

/// Windows 平台的检查列表。
///
/// 每一项都调用**真实实现**（真实扫描、真实 `CreateProcessW`/`ShellExecuteW`、
/// 在有限时间内运行一个外部命令并取回它的标准输出。
///
/// 诊断工具**不能**因为外部工具阻塞而挂死：当前会话里 `wl-copy` 就可能拿不到选区而永久
/// 阻塞（自动化会话没有可用的输入序列）。这里用「stdout 重定向到临时文件 + 轮询 + 超时
/// 杀掉」的方式等待，既不依赖管道（被 fork 出的守护进程会一直持有管道写端），也不会
/// 无限等待。
#[cfg(target_os = "linux")]
fn run_bounded(program: &str, args: &[&str], timeout: Duration) -> Result<(bool, String), String> {
    use std::process::{Command, Stdio};

    let out_path = std::env::temp_dir().join(format!(
        "flashcast-check-{}-{}.out",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let file = std::fs::File::create(&out_path)
        .map_err(|error| format!("{program}: 无法创建输出文件（{error}）"))?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(file))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("{program}: 无法启动（{error}）"))?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = std::fs::read_to_string(&out_path).unwrap_or_default();
                let _ = std::fs::remove_file(&out_path);
                return Ok((status.success(), text));
            }
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(20));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = std::fs::remove_file(&out_path);
                return Err(format!(
                    "{program}: 超过 {} 秒没有返回，已中止（当前会话里该工具被阻塞）",
                    timeout.as_secs()
                ));
            }
            Err(error) => {
                let _ = std::fs::remove_file(&out_path);
                return Err(format!("{program}: 等待失败（{error}）"));
            }
        }
    }
}

/// 在有限时间内运行一段可能阻塞的操作（真实剪贴板写入）。
///
/// 用独立线程 + `recv_timeout`：超时后丢弃那个线程（进程退出时它自然消失），
/// 这样诊断报告仍然能如实写出「写入被阻塞」，而不是整个检查挂死。
#[cfg(target_os = "linux")]
fn run_bounded_blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
    timeout: Duration,
) -> Result<T, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(timeout)
        .map_err(|_| format!("超过 {} 秒没有返回，已放弃等待", timeout.as_secs()))
}

/// Linux 剪贴板真实写入 + 回读。
///
/// 这一项**会覆盖用户当前的剪贴板**，因此必须显式加 `--allow-clipboard-write` 才执行；
/// 未允许时如实报「未覆盖」并给出复现命令，而不是假装通过。
///
/// 回读用同一族的工具（Wayland 下 `wl-paste`，X11 下 `xclip -o`）：只有真的读回同一段
/// 文本，才算「写进系统剪贴板」，命令退出码为 0 不算证据。
#[cfg(target_os = "linux")]
fn clipboard_write_check(capabilities: &Capabilities, allowed: bool) -> CheckResult {
    use flashcast_platform::clipboard::ClipboardAccess;

    let command = "cargo run -p flashcast-platform --bin flashcast-platform-check \
                   -- --allow-clipboard-write";
    if !allowed {
        return CheckResult {
            id: "clipboard.write_text",
            title: "真实写入系统剪贴板并回读",
            status: Status::NotCovered,
            detail: "未执行：该检查会覆盖当前剪贴板内容，需要显式加 --allow-clipboard-write"
                .to_string(),
            command: command.to_string(),
        };
    }
    let marker = format!(
        "flashcast-platform-check-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    let write_marker = marker.clone();
    let clipboard = flashcast_platform::linux::LinuxClipboard::new();
    let written = run_bounded_blocking(
        move || {
            clipboard
                .write_text(&write_marker)
                .map_err(|error| error.to_string())
        },
        Duration::from_secs(5),
    );
    match written {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return CheckResult {
                id: "clipboard.write_text",
                title: "真实写入系统剪贴板并回读",
                status: Status::MeasuredFail,
                detail: format!("写入失败：{error}"),
                command: command.to_string(),
            }
        }
        Err(reason) => {
            return CheckResult {
                id: "clipboard.write_text",
                title: "真实写入系统剪贴板并回读",
                status: Status::NotCovered,
                detail: format!(
                    "写入没有完成：{reason}。当前会话里剪贴板工具（wl-copy / xclip）无法取得                     选区——自动化会话缺少可用的输入序列时就是这样；这不代表真实桌面上的复制会失败"
                ),
                command: command.to_string(),
            }
        }
    }
    // 回读：优先会话对应的工具，失败再试另一族（XWayland 场景下两者都可能可用）。
    let candidates: &[(&str, &[&str])] = if capabilities.session == SessionType::Wayland {
        &[
            ("wl-paste", &["--no-newline"][..]),
            ("xclip", &["-selection", "clipboard", "-o"][..]),
        ]
    } else {
        &[
            ("xclip", &["-selection", "clipboard", "-o"][..]),
            ("wl-paste", &["--no-newline"][..]),
        ]
    };
    let mut attempts = Vec::new();
    for (program, args) in candidates {
        match run_bounded(program, args, Duration::from_secs(5)) {
            Ok((true, text)) if text.trim() == marker => {
                return CheckResult {
                    id: "clipboard.write_text",
                    title: "真实写入系统剪贴板并回读",
                    status: Status::MeasuredPass,
                    detail: format!(
                        "用 {program} 写入并回读到同一段文本（{} 字节）",
                        marker.len()
                    ),
                    command: command.to_string(),
                }
            }
            Ok((true, text)) => attempts.push(format!(
                "{program}: 回读内容不一致（{} 字节）",
                text.trim().len()
            )),
            Ok((false, _)) => attempts.push(format!("{program}: 退出码非 0")),
            Err(reason) => attempts.push(reason),
        }
    }
    CheckResult {
        id: "clipboard.write_text",
        title: "真实写入系统剪贴板并回读",
        status: Status::MeasuredFail,
        detail: format!("写入成功但回读失败（{}）", attempts.join("；")),
        command: command.to_string(),
    }
}

/// Linux 自动粘贴的前置条件：会话类型 + XTEST 扩展是否真的可用。
///
/// **不**注入真实按键：`flashcast-platform-check` 运行时前台窗口是用户的终端或编辑器，
/// 盲发一次 Ctrl+V 会往那个窗口里粘贴用户剪贴板里的任意内容。这里只核对注入所需的条件
/// （真实查询 XTEST 扩展），端到端粘贴由「在真实桌面里对准目标应用」的手动检查覆盖。
#[cfg(target_os = "linux")]
fn paste_prepare_check() -> CheckResult {
    use flashcast_platform::linux::{LinuxPaster, WAYLAND_PASTE_REASON};

    let paster = LinuxPaster::new();
    let session = paster.session();
    let xtest = flashcast_platform::linux::x11::xtest_available();
    let command = "cargo run -p flashcast-platform --bin flashcast-platform-check";
    let detail = format!(
        "会话={}，XTEST={}，注入后端={}；未注入真实按键（会打到当前前台窗口）",
        session.label_zh(),
        xtest,
        if paster.uses_x11() {
            "X11/XTEST"
        } else {
            "不可用"
        },
    );
    let (status, detail) = if !paster.uses_x11() {
        (
            Status::NotCovered,
            format!("{detail}；{WAYLAND_PASTE_REASON}"),
        )
    } else if xtest {
        (Status::MeasuredPass, detail)
    } else {
        (Status::MeasuredFail, detail)
    };
    CheckResult {
        id: "paste.prepare",
        title: "自动粘贴前置条件（XTEST / 会话类型）",
        status,
        detail,
        command: command.to_string(),
    }
}

/// 真实 `RegisterHotKey`、真实焦点读写），并按「实测通过 / 实测失败 / 未覆盖」如实
/// 报告。id 与 Linux 侧对齐，另外补上 Windows 独有的 `apps.launch`、`focus.restore`
/// 与 `win.shell`。
#[cfg(target_os = "windows")]
fn windows_checks(capabilities: &Capabilities) -> Vec<CheckResult> {
    use flashcast_platform::focus::FocusError;
    use flashcast_platform::windows::session;

    let command = "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string();
    let json_command =
        "cargo run -p flashcast-platform --bin flashcast-platform-check -- --json".to_string();
    let desktop = session::desktop_session();
    let mut checks = Vec::new();

    // 0. 自动粘贴前置条件（ticket 08）：SendInput 的实现随应用编译进来；本检查
    // **不**注入真实按键（盲发一次 Ctrl+V 会往当前前台窗口粘贴用户剪贴板内容），
    // 因此端到端粘贴在 Windows 上属于「未覆盖」，需要对准目标应用手动验证。
    checks.push(CheckResult {
        id: "paste.prepare",
        title: "自动粘贴前置条件（SendInput）",
        status: if capabilities.auto_paste.is_supported() {
            Status::NotCovered
        } else {
            Status::MeasuredFail
        },
        detail: format!(
            "注入后端=SendInput（Ctrl+V，带 dwExtraInfo 标记），能力={}；             本检查不注入真实按键，端到端粘贴未覆盖（受 UIPI 限制，管理员窗口收不到）",
            capabilities.auto_paste.label_zh()
        ),
        command: command.clone(),
    });

    // 1. 会话判定：Windows 上不存在 X11/Wayland 分类，探测本身总能给出确定答案。
    checks.push(CheckResult {
        id: "session.detect",
        title: "桌面会话判定",
        status: Status::MeasuredPass,
        detail: format!(
            "Windows 会话（不适用 X11/Wayland 分类）：{}；GetForegroundWindow={}，GetShellWindow={}",
            desktop.label_zh(),
            desktop.foreground_window,
            desktop.shell_window
        ),
        command: command.clone(),
    });

    // 2. 软件发现：开始菜单 + 注册表 + 打包应用三个来源都真实扫描。
    let catalog = WindowsAppCatalog::new();
    let outcome = catalog.scan_detailed();
    let with_icons = outcome
        .entries
        .iter()
        .filter(|entry| {
            entry
                .icon
                .as_ref()
                .and_then(|icon| icon.path.as_ref())
                .is_some()
        })
        .count();
    let (status, detail) = if outcome.start_menu_roots_present == 0
        && outcome.registry_keys_seen == 0
        && outcome.uwp_seen == 0
    {
        (
            Status::NotCovered,
            format!(
                "没有可读的开始菜单目录，注册表与打包应用来源也没有返回数据（可能是无桌面会话）；\
                 开始菜单根={:?}",
                outcome.start_menu_roots
            ),
        )
    } else if outcome.entries.is_empty() {
        (
            Status::MeasuredFail,
            format!(
                "来源存在但没有可启动条目：开始菜单根存在 {} 个，扫描快捷方式 {} 个，\
                 注册表子键 {} 个，打包应用 {} 个，跳过 {} 条",
                outcome.start_menu_roots_present,
                outcome.shortcuts_seen,
                outcome.registry_keys_seen,
                outcome.uwp_seen,
                outcome.skipped.len()
            ),
        )
    } else {
        (
            Status::MeasuredPass,
            format!(
                "发现 {} 个可启动条目：开始菜单快捷方式 {} 个（{} 次走 IShellLinkW 纠正），\
                 注册表子键 {} 个，打包应用 {} 个；其中 {} 个解析到图标文件、抽取失败 {} 次；\
                 跳过 {} 条",
                outcome.entries.len(),
                outcome.shortcuts_seen,
                outcome.shell_corrections,
                outcome.registry_keys_seen,
                outcome.uwp_seen,
                with_icons,
                outcome.icons_failed,
                outcome.skipped.len()
            ),
        )
    };
    checks.push(CheckResult {
        id: "apps.discovery",
        title: "Windows 软件发现（开始菜单 / 注册表 / 打包应用）",
        status,
        detail,
        command: json_command.clone(),
    });

    // 3. 真实启动：一个有效目标必须成功，一个失效目标必须报错（绝不静默成功）。
    let launcher = WindowsLauncher::new();
    let comspec =
        std::env::var("ComSpec").unwrap_or_else(|_| r"C:\Windows\System32\cmd.exe".to_string());
    let positive = launcher.launch(&LaunchRequest::new(comspec.clone()).with_args(["/c", "exit"]));
    let missing = r"C:\Flashcast\definitely-missing\nope.exe";
    let negative = launcher.launch(&LaunchRequest::new(missing));
    let (status, detail) = match (&positive, &negative) {
        (Ok(receipt), Err(error)) => (
            Status::MeasuredPass,
            format!(
                "用 {comspec} /c exit 启动成功（pid={:?}）；失效路径按预期报错：{error}",
                receipt.pid
            ),
        ),
        (Err(error), _) => (
            Status::MeasuredFail,
            format!("启动 {comspec} 失败：{error}"),
        ),
        (Ok(receipt), Ok(_)) => (
            Status::MeasuredFail,
            format!(
                "失效路径 {missing} 被当成启动成功（pid={:?}），违反「失效入口必须报错」",
                receipt.pid
            ),
        ),
    };
    checks.push(CheckResult {
        id: "apps.launch",
        title: "启动真实目标与失效条目报错",
        status,
        detail,
        command: command.clone(),
    });

    // 4~5. 焦点读取与恢复。
    let focus = WindowsFocusTracker::new();
    let captured = focus.capture();
    checks.push(match &captured {
        Ok(app) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::MeasuredPass,
            detail: format!(
                "读取到 id={} name={} pid={:?} window={:?}",
                app.id, app.name, app.pid, app.window
            ),
            command: command.clone(),
        },
        Err(FocusError::Unsupported { reason }) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::NotCovered,
            detail: reason.clone(),
            command: command.clone(),
        },
        Err(error) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::MeasuredFail,
            detail: error.to_string(),
            command: command.clone(),
        },
    });
    checks.push(match &captured {
        Err(FocusError::Unsupported { reason }) => CheckResult {
            id: "focus.restore",
            title: "把焦点还给唤起前应用",
            status: Status::NotCovered,
            detail: format!("没有可交互桌面，未尝试恢复焦点：{reason}"),
            command: command.clone(),
        },
        Ok(app) => match focus.restore(app) {
            Ok(()) => CheckResult {
                id: "focus.restore",
                title: "把焦点还给唤起前应用",
                status: Status::MeasuredPass,
                detail: format!(
                    "ShowWindow(SW_RESTORE)+SetForegroundWindow 把焦点还给 {} 并在回读校验中一致",
                    app.name
                ),
                command: command.clone(),
            },
            Err(error) => CheckResult {
                id: "focus.restore",
                title: "把焦点还给唤起前应用",
                status: Status::MeasuredFail,
                detail: format!("{}（Windows 可能拒绝前台切换，UI 需退回手动粘贴）", error),
                command: command.clone(),
            },
        },
        Err(error) => CheckResult {
            id: "focus.restore",
            title: "把焦点还给唤起前应用",
            status: Status::NotCovered,
            detail: format!("未能先捕获前台应用，因此未尝试恢复：{error}"),
            command: command.clone(),
        },
    });

    // 6. 全局快捷键：真实注册并立即注销。
    let hotkeys = WindowsHotkeyManager::new();
    let spec = HotkeySpec::parse("Ctrl+Alt+F12").expect("固定检查用快捷键必须可解析");
    let hotkey_check = if !desktop.interactive() {
        CheckResult {
            id: "hotkey.register",
            title: "全局快捷键注册",
            status: Status::NotCovered,
            detail: "没有可交互桌面会话，RegisterHotKey 无法工作".to_string(),
            command: command.clone(),
        }
    } else {
        match hotkeys.register(&spec, Arc::new(|| {})) {
            Ok(handle) => {
                let _ = hotkeys.unregister(&handle);
                CheckResult {
                    id: "hotkey.register",
                    title: "全局快捷键注册",
                    status: Status::MeasuredPass,
                    detail: format!("注册并注销 {} 成功", spec.canonical()),
                    command: command.clone(),
                }
            }
            Err(flashcast_platform::shortcut::HotkeyError::BackendUnavailable { reason }) => {
                CheckResult {
                    id: "hotkey.register",
                    title: "全局快捷键注册",
                    status: Status::NotCovered,
                    detail: reason,
                    command: command.clone(),
                }
            }
            Err(error) => CheckResult {
                id: "hotkey.register",
                title: "全局快捷键注册",
                status: Status::MeasuredFail,
                detail: error.to_string(),
                command: command.clone(),
            },
        }
    };
    checks.push(hotkey_check);

    // 7. 剪贴板与自动粘贴：ticket 08/09/10 才实现，这里只如实报告环境前提。
    for (id, title, support) in [
        (
            "clipboard.text",
            "剪贴板文字读写",
            capabilities.clipboard.clone(),
        ),
        (
            "paste.auto",
            "自动粘贴到唤起前应用",
            capabilities.auto_paste.clone(),
        ),
    ] {
        let (status, detail) = match &support {
            Support::Supported => (Status::MeasuredPass, "支持".to_string()),
            Support::Unsupported { reason } => (Status::MeasuredFail, reason.clone()),
            Support::Unknown { reason } => (Status::NotCovered, reason.clone()),
        };
        checks.push(CheckResult {
            id,
            title,
            status,
            detail,
            command: command.clone(),
        });
    }

    // 8. Windows shell 环境：真实检查 explorer.exe 与 PowerShell 是否可用
    //    （打包应用枚举依赖后者）。
    let windir = std::env::var("WINDIR").ok();
    let explorer = format!(
        "{}\\explorer.exe",
        windir.as_deref().unwrap_or(r"C:\Windows")
    );
    let explorer_present = std::path::Path::new(&explorer).is_file();
    let powershell = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$PSVersionTable.PSVersion.ToString()",
        ])
        .output();
    let (powershell_status, powershell_detail) = match powershell {
        Ok(output) if output.status.success() => (
            Status::MeasuredPass,
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ),
        Ok(output) => (
            Status::MeasuredFail,
            format!("退出码 {:?}", output.status.code()),
        ),
        Err(error) => (Status::MeasuredFail, error.to_string()),
    };
    checks.push(CheckResult {
        id: "win.shell",
        title: "Windows shell 环境（explorer / PowerShell）",
        status: if explorer_present && powershell_status == Status::MeasuredPass {
            Status::MeasuredPass
        } else {
            Status::MeasuredFail
        },
        detail: format!(
            "WINDIR={:?}，ComSpec={:?}，{} 存在={}，PowerShell={}（{}）",
            windir,
            std::env::var("ComSpec").ok(),
            explorer,
            explorer_present,
            powershell_detail,
            powershell_status.label_zh()
        ),
        command: command.clone(),
    });

    // 9. X11/EWMH 只存在于 Linux 会话。
    checks.push(CheckResult {
        id: "x11.diagnostics",
        title: "X11/EWMH 诊断",
        status: Status::NotCovered,
        detail: "X11 / EWMH 只存在于 Linux 会话，Windows 上不适用".to_string(),
        command: command.clone(),
    });

    checks
}

fn print_human(
    capabilities: &flashcast_platform::capability::Capabilities,
    checks: &[CheckResult],
) {
    println!(
        "Flashcast v{} 平台能力检查（真实环境）",
        env!("CARGO_PKG_VERSION")
    );
    println!("==================================================");
    println!(
        "系统：{} {}",
        capabilities.os.as_str(),
        capabilities
            .os_version
            .clone()
            .unwrap_or_else(|| "<未知>".to_string())
    );
    println!("架构：{}", capabilities.arch);
    println!("会话：{}", capabilities.session.label_zh());
    println!("桌面可用：{}", capabilities.desktop_available);
    println!("全局快捷键：{}", capabilities.hotkey.label_zh());
    println!("剪贴板：{}", capabilities.clipboard.label_zh());
    println!("自动粘贴：{}", capabilities.auto_paste.label_zh());
    for note in &capabilities.notes {
        println!("说明：{note}");
    }
    println!();
    for check in checks {
        println!(
            "[{}] {} — {}",
            check.status.label_zh(),
            check.title,
            check.detail
        );
    }
    println!();
    println!(
        "真实执行：以上检查全部调用真实平台实现，未使用任何测试替身（testDoubles 为空）。\
         替身通过的检查不构成平台适配证据。"
    );
    println!();
    let pass = checks
        .iter()
        .filter(|c| c.status == Status::MeasuredPass)
        .count();
    let fail = checks
        .iter()
        .filter(|c| c.status == Status::MeasuredFail)
        .count();
    let uncovered = checks
        .iter()
        .filter(|c| c.status == Status::NotCovered)
        .count();
    println!("汇总：实测通过 {pass}，实测失败 {fail}，未覆盖 {uncovered}");
    println!(
        "提示：本报告只反映当前会话（{}）。X11 检查通过不能推断 Wayland 可用。",
        capabilities.session.label_zh()
    );
}

fn print_json(
    capabilities: &flashcast_platform::capability::Capabilities,
    checks: &[CheckResult],
    options: &Options,
) {
    let mut out = String::new();
    out.push_str("{\n");
    out.push_str("  \"schemaVersion\": 1,\n");
    // 「真实执行 / 测试替身 / 未覆盖」三者在报告里分开：本二进制不使用替身，
    // 因此这里恒为空数组；未覆盖项由各 check 的 status 单独列出。
    out.push_str("  \"testDoubles\": [],\n");
    out.push_str("  \"product\": \"flashcast\",\n");
    out.push_str(&format!(
        "  \"version\": \"{}\",\n",
        env!("CARGO_PKG_VERSION")
    ));
    out.push_str("  \"environment\": {\n");
    out.push_str(&format!(
        "    \"os\": \"{}\",\n",
        json_escape(capabilities.os.as_str())
    ));
    out.push_str(&format!(
        "    \"osVersion\": {},\n",
        match &capabilities.os_version {
            Some(v) => format!("\"{}\"", json_escape(v)),
            None => "null".to_string(),
        }
    ));
    out.push_str(&format!(
        "    \"arch\": \"{}\",\n",
        json_escape(&capabilities.arch)
    ));
    out.push_str(&format!(
        "    \"sessionType\": \"{}\",\n",
        capabilities.session.as_str()
    ));
    out.push_str(&format!(
        "    \"desktopAvailable\": {},\n",
        capabilities.desktop_available
    ));
    out.push_str(&format!(
        "    \"xdgSessionType\": {},\n",
        std::env::var("XDG_SESSION_TYPE")
            .map(|v| format!("\"{}\"", json_escape(&v)))
            .unwrap_or_else(|_| "null".to_string())
    ));
    out.push_str(&format!(
        "    \"xdgCurrentDesktop\": {},\n",
        std::env::var("XDG_CURRENT_DESKTOP")
            .map(|v| format!("\"{}\"", json_escape(&v)))
            .unwrap_or_else(|_| "null".to_string())
    ));
    out.push_str("    \"source\": \"flashcast-platform-check\"\n");
    out.push_str("  },\n");
    out.push_str("  \"capabilities\": {\n");
    out.push_str(&format!(
        "    \"hotkey\": \"{}\",\n",
        capabilities.hotkey.label_zh()
    ));
    out.push_str(&format!(
        "    \"clipboard\": \"{}\",\n",
        capabilities.clipboard.label_zh()
    ));
    out.push_str(&format!(
        "    \"autoPaste\": \"{}\"\n",
        capabilities.auto_paste.label_zh()
    ));
    out.push_str("  },\n");
    out.push_str("  \"checks\": [\n");
    for (index, check) in checks.iter().enumerate() {
        out.push_str("    {\n");
        out.push_str(&format!("      \"id\": \"{}\",\n", json_escape(check.id)));
        out.push_str(&format!(
            "      \"title\": \"{}\",\n",
            json_escape(check.title)
        ));
        out.push_str(&format!("      \"status\": \"{}\",\n", check.status.key()));
        out.push_str(&format!(
            "      \"statusLabel\": \"{}\",\n",
            check.status.label_zh()
        ));
        out.push_str(&format!(
            "      \"detail\": \"{}\",\n",
            json_escape(&check.detail)
        ));
        out.push_str(&format!(
            "      \"command\": \"{}\"\n",
            json_escape(&check.command)
        ));
        out.push_str(if index + 1 == checks.len() {
            "    }\n"
        } else {
            "    },\n"
        });
    }
    out.push_str("  ],\n");
    out.push_str("  \"summary\": {\n");
    out.push_str(&format!(
        "    \"measuredPass\": {},\n",
        checks
            .iter()
            .filter(|c| c.status == Status::MeasuredPass)
            .count()
    ));
    out.push_str(&format!(
        "    \"measuredFail\": {},\n",
        checks
            .iter()
            .filter(|c| c.status == Status::MeasuredFail)
            .count()
    ));
    out.push_str(&format!(
        "    \"notCovered\": {}\n",
        checks
            .iter()
            .filter(|c| c.status == Status::NotCovered)
            .count()
    ));
    out.push_str("  },\n");
    out.push_str(&format!(
        "  \"notes\": [\n{}\n  ]\n",
        capabilities
            .notes
            .iter()
            .map(|n| format!("    \"{}\"", json_escape(n)))
            .collect::<Vec<_>>()
            .join(",\n")
    ));
    out.push_str("}\n");

    match &options.output {
        Some(path) => match std::fs::write(path, &out) {
            Ok(()) => println!("已写入 {path}"),
            Err(error) => {
                eprintln!("写入 {path} 失败：{error}");
                std::process::exit(1);
            }
        },
        None => print!("{out}"),
    }
}

/// 复现「编译」检查的命令：Linux 与 Windows 是本机构建，其余平台是交叉 `cargo check`。
fn compile_command() -> String {
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    {
        "cargo build -p flashcast-platform --bin flashcast-platform-check".to_string()
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        CROSS_CHECK_HINT.to_string()
    }
}

/// macOS 真实检查（ticket 03）。
///
/// 每一项都调用真实的 macOS 实现：目录遍历与 `Info.plist` 解析、`NSWorkspace`
/// 图标渲染、`NSRunningApplication` 焦点读取与激活、Carbon `RegisterEventHotKey`
/// 注册、`AXIsProcessTrusted` 权限查询。**不使用任何测试替身**。
///
/// 环境不支持的项（没有任何应用目录、未授予辅助功能权限、没有桌面会话）记为
/// 「未覆盖」并写明原因；命令调用成功不等于用户操作成功。
#[cfg(target_os = "macos")]
fn macos_checks(capabilities: &Capabilities) -> Vec<CheckResult> {
    let mut checks = Vec::new();
    let plain = || "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string();
    let json =
        || "cargo run -p flashcast-platform --bin flashcast-platform-check -- --json".to_string();

    // 2. 桌面会话可用性。macOS 没有 X11/Wayland 会话区分。
    checks.push(CheckResult {
        id: "session.detect",
        title: "桌面会话可用性",
        status: if capabilities.desktop_available {
            Status::MeasuredPass
        } else {
            Status::NotCovered
        },
        detail: if capabilities.desktop_available {
            "macOS 没有 X11/Wayland 会话类型；NSWorkspace 能报出前台应用，桌面会话可用".to_string()
        } else {
            "NSWorkspace 未报出前台应用：没有可交互的桌面会话（无 Aqua 会话）".to_string()
        },
        command: plain(),
    });

    // 3. 软件发现（真实遍历 /Applications 等目录并解析 Info.plist）。
    let catalog = MacosAppCatalog::new();
    let existing = catalog.existing_roots();
    let missing = catalog.missing_roots();
    let outcome = catalog.scan_detailed();
    let discovery_status = if existing.is_empty() {
        Status::NotCovered
    } else if outcome.entries.is_empty() {
        Status::MeasuredFail
    } else {
        Status::MeasuredPass
    };
    checks.push(CheckResult {
        id: "apps.discovery",
        title: "macOS 应用发现",
        status: discovery_status,
        detail: format!(
            "发现 {} 个 .app 包，其中 {} 个可启动；跳过 {} 个；应用目录存在 {} 个、缺失 {} 个（{:?}）；\
             图标渲染成功 {} 个、失败 {} 个",
            outcome.scan.bundles_seen,
            outcome.entries.len(),
            outcome.scan.skipped.len(),
            existing.len(),
            missing.len(),
            missing,
            outcome.icons_rendered,
            outcome.icons_failed.len(),
        ),
        command: json(),
    });
    if !outcome.scan.skipped.is_empty() {
        let detail = outcome
            .scan
            .skipped
            .iter()
            .take(8)
            .map(|skipped| format!("{}（{}）", skipped.path.display(), skipped.reason.as_str()))
            .collect::<Vec<_>>()
            .join("；");
        checks.push(CheckResult {
            id: "apps.discovery.skipped",
            title: "被跳过的 .app 包明细",
            status: Status::MeasuredPass,
            detail,
            command: plain(),
        });
    }

    // 4. 图标渲染：最能暴露 AppKit 路径问题的单项。
    let icon_check = match outcome.entries.first() {
        None => CheckResult {
            id: "apps.icon_render",
            title: "应用图标渲染（NSWorkspace → PNG）",
            status: Status::NotCovered,
            detail: "软件发现结果为空，没有可渲染的应用包".to_string(),
            command: plain(),
        },
        Some(entry) => match entry.exec.first().map(std::path::PathBuf::from) {
            None => CheckResult {
                id: "apps.icon_render",
                title: "应用图标渲染（NSWorkspace → PNG）",
                status: Status::NotCovered,
                detail: format!("条目 {} 没有可用的应用包路径", entry.name),
                command: plain(),
            },
            Some(bundle) => match macos_icons::render_icon_png(&bundle, ICON_POINT_SIZE) {
                Ok(bytes) if bytes.starts_with(&[0x89, b'P', b'N', b'G']) => CheckResult {
                    id: "apps.icon_render",
                    title: "应用图标渲染（NSWorkspace → PNG）",
                    status: Status::MeasuredPass,
                    detail: format!(
                        "NSWorkspace 渲染 {} 得到 {} 字节 PNG（宽度 {} 点）",
                        bundle.display(),
                        bytes.len(),
                        ICON_POINT_SIZE
                    ),
                    command: plain(),
                },
                Ok(bytes) => CheckResult {
                    id: "apps.icon_render",
                    title: "应用图标渲染（NSWorkspace → PNG）",
                    status: Status::MeasuredFail,
                    detail: format!(
                        "渲染结果不是 PNG：{} 字节，首字节 {:?}",
                        bytes.len(),
                        &bytes[..bytes.len().min(8)]
                    ),
                    command: plain(),
                },
                Err(error) => CheckResult {
                    id: "apps.icon_render",
                    title: "应用图标渲染（NSWorkspace → PNG）",
                    status: Status::MeasuredFail,
                    detail: format!("渲染 {} 失败：{error}", bundle.display()),
                    command: plain(),
                },
            },
        },
    };
    checks.push(icon_check);

    // 5. 焦点读取：唤起前前台应用。
    let focus = MacosFocusTracker::new();
    checks.push(match MacosFocusTrackerTrait::capture(&focus) {
        Ok(app) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::MeasuredPass,
            detail: format!(
                "读取到 id={} name={} bundle={:?} pid={:?}",
                app.id, app.name, app.wm_class, app.pid
            ),
            command: plain(),
        },
        Err(flashcast_platform::focus::FocusError::Unsupported { reason }) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::NotCovered,
            detail: reason,
            command: plain(),
        },
        Err(error) => CheckResult {
            id: "focus.capture",
            title: "读取唤起前前台应用",
            status: Status::MeasuredFail,
            detail: error.to_string(),
            command: plain(),
        },
    });

    // 6. 辅助功能权限：决定自动粘贴能否工作，也决定用户该去哪个设置面板。
    let granted = accessibility_granted();
    checks.push(CheckResult {
        id: "accessibility.permission",
        title: "辅助功能（Accessibility）权限",
        status: if granted {
            Status::MeasuredPass
        } else {
            Status::NotCovered
        },
        detail: if granted {
            format!("AXIsProcessTrusted() = true；{ACCESSIBILITY_SETTINGS_LABEL}")
        } else {
            format!(
                "AXIsProcessTrusted() = false，注入按键不可用；需要用户手动授权：\
                 {ACCESSIBILITY_SETTINGS_LABEL}（UI 可调用 open_accessibility_settings() 直接打开该面板）"
            )
        },
        command: plain(),
    });

    // 6b. 自动粘贴前置条件（ticket 08）：只核对权限与后端，不注入真实按键——
    // 盲发一次 Cmd+V 会往当前前台应用里粘贴用户剪贴板里的任意内容。
    let paster = flashcast_platform::macos::paste::MacosPaster::new();
    checks.push(CheckResult {
        id: "paste.prepare",
        title: "自动粘贴前置条件（辅助功能权限 / CGEvent）",
        status: if granted { Status::MeasuredPass } else { Status::NotCovered },
        detail: format!(
            "注入后端=CGEvent（Cmd+V），权限={}；未注入真实按键，端到端粘贴需要对准目标应用手动验证",
            if paster.accessible() { "已授权" } else { "未授权" }
        ),
        command: plain(),
    });

    // 7. 全局快捷键注册：真实的 Carbon RegisterEventHotKey 注册 + 注销。
    let hotkeys = MacosHotkeyManager::new();
    let spec = MacosHotkeySpec::parse("Ctrl+Alt+F12").expect("固定检查用快捷键必须可解析");
    checks.push(
        match MacosHotkeyManagerTrait::register(&hotkeys, &spec, MacosArc::new(|| {})) {
            Ok(handle) => {
                let _ = MacosHotkeyManagerTrait::unregister(&hotkeys, &handle);
                CheckResult {
                    id: "hotkey.register",
                    title: "全局快捷键注册",
                    status: Status::MeasuredPass,
                    detail: format!("注册并注销 {} 成功", spec.canonical()),
                    command: plain(),
                }
            }
            Err(MacosHotkeyError::BackendUnavailable { reason }) => CheckResult {
                id: "hotkey.register",
                title: "全局快捷键注册",
                status: Status::NotCovered,
                detail: reason,
                command: plain(),
            },
            Err(error) => CheckResult {
                id: "hotkey.register",
                title: "全局快捷键注册",
                status: Status::MeasuredFail,
                detail: error.to_string(),
                command: plain(),
            },
        },
    );

    // 8. 剪贴板与自动粘贴：适配由 ticket 08/09/10 提供，这里只报告真实前提。
    for (id, title, support) in [
        (
            "clipboard.text",
            "剪贴板文字读写",
            capabilities.clipboard.clone(),
        ),
        (
            "paste.auto",
            "自动粘贴到唤起前应用",
            capabilities.auto_paste.clone(),
        ),
    ] {
        let (status, detail) = match &support {
            MacosSupport::Supported => (Status::MeasuredPass, "支持".to_string()),
            MacosSupport::Unsupported { reason } => (Status::MeasuredFail, reason.clone()),
            MacosSupport::Unknown { reason } => (Status::NotCovered, reason.clone()),
        };
        checks.push(CheckResult {
            id,
            title,
            status,
            detail,
            command: plain(),
        });
    }

    checks
}

/// Linux / Windows / macOS 之外的目标上的检查列表。
///
/// id 与标题和 Linux 侧保持一致，状态一律为「未覆盖」，并写明原因：
/// 这些能力的真实实现由后续平台 ticket 提供，本平台目前只保证能编译。
#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn uncovered_checks(capabilities: &Capabilities) -> Vec<CheckResult> {
    let os = capabilities.os.as_str();
    let unchecked = |id: &'static str, title: &'static str, what: &str| CheckResult {
        id,
        title,
        status: Status::NotCovered,
        detail: format!("{what}（当前平台：{os}）"),
        command: CROSS_CHECK_HINT.to_string(),
    };

    vec![
        unchecked(
            "session.detect",
            "桌面会话类型判定",
            "该平台的会话类型判定由后续平台 ticket 提供",
        ),
        unchecked(
            "apps.discovery",
            "软件发现",
            "该平台的软件发现由后续平台 ticket 提供（Linux 使用 freedesktop 扫描）",
        ),
        unchecked(
            "focus.capture",
            "读取唤起前前台应用",
            "该平台的前台应用读取由后续平台 ticket 提供",
        ),
        unchecked(
            "hotkey.register",
            "全局快捷键注册",
            "该平台的全局快捷键注册由后续平台 ticket 提供",
        ),
        unchecked(
            "clipboard.text",
            "剪贴板文字读写",
            "该平台的剪贴板读写由后续 ticket 提供",
        ),
        unchecked(
            "paste.auto",
            "自动粘贴到唤起前应用",
            "该平台的自动粘贴由后续 ticket 提供",
        ),
        unchecked(
            "x11.diagnostics",
            "X11/EWMH 诊断",
            "X11 / EWMH 只存在于 Linux 会话，当前平台不适用",
        ),
    ]
}
