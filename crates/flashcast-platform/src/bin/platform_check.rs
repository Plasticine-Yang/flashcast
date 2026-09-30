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
use std::sync::Arc;
#[cfg(target_os = "linux")]
use flashcast_platform::capability::{CapabilityProbe, SessionType, Support};
#[cfg(target_os = "linux")]
use flashcast_platform::focus::FocusTracker;
#[cfg(target_os = "linux")]
use flashcast_platform::hotkey::HotkeySpec;
#[cfg(target_os = "linux")]
use flashcast_platform::shortcut::HotkeyManager;
#[cfg(target_os = "linux")]
use flashcast_platform::linux::{
    LinuxAppCatalog, LinuxCapabilityProbe, LinuxFocusTracker, LinuxHotkeyManager,
};

/// 非 Linux 平台上报告「未覆盖」时给出的复现命令提示。
#[cfg(all(not(target_os = "linux"), target_os = "windows"))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform --target x86_64-pc-windows-msvc";
#[cfg(all(not(target_os = "linux"), target_os = "macos"))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform --target x86_64-apple-darwin";
#[cfg(all(not(target_os = "linux"), not(any(target_os = "windows", target_os = "macos"))))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform";

/// 探测当前环境能力。Linux 用真实探测器，其他平台用「不支持」桩实现。
fn probe_capabilities() -> Capabilities {
    #[cfg(target_os = "linux")]
    {
        LinuxCapabilityProbe::new().probe()
    }
    #[cfg(not(target_os = "linux"))]
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
}

fn parse_args() -> Options {
    let mut options = Options::default();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--output" => options.output = args.next(),
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

    // 2~7 依赖各平台的具体实现，因此按平台分开：
    // Linux 走真实实现；Windows / macOS 由后续平台 ticket 提供，这里如实报告「未覆盖」。
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
                std::env::var("DISPLAY").map(|v| !v.is_empty()).unwrap_or(false),
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
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
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
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
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
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
            },
        };
        checks.push(hotkey_check);

        // 6. 剪贴板与自动粘贴：ticket 01 只报告环境前提，状态保持未覆盖。
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
                command: "cargo run -p flashcast-platform --bin flashcast-platform-check".to_string(),
            });
        }

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

    #[cfg(not(target_os = "linux"))]
    {
        checks.extend(uncovered_checks(&capabilities));
    }

    if options.json {
        print_json(&capabilities, &checks, &options);
    } else {
        print_human(&capabilities, &checks);
    }
}

fn print_human(capabilities: &flashcast_platform::capability::Capabilities, checks: &[CheckResult]) {
    println!("Flashcast v{} 平台能力检查（真实环境）", env!("CARGO_PKG_VERSION"));
    println!("==================================================");
    println!("系统：{} {}", capabilities.os.as_str(), capabilities.os_version.clone().unwrap_or_else(|| "<未知>".to_string()));
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
        checks.iter().filter(|c| c.status == Status::MeasuredPass).count()
    ));
    out.push_str(&format!(
        "    \"measuredFail\": {},\n",
        checks.iter().filter(|c| c.status == Status::MeasuredFail).count()
    ));
    out.push_str(&format!(
        "    \"notCovered\": {}\n",
        checks.iter().filter(|c| c.status == Status::NotCovered).count()
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

/// 复现「编译」检查的命令：Linux 是本机构建，其他平台是交叉 `cargo check`。
fn compile_command() -> String {
    #[cfg(target_os = "linux")]
    {
        "cargo build -p flashcast-platform --bin flashcast-platform-check".to_string()
    }
    #[cfg(not(target_os = "linux"))]
    {
        CROSS_CHECK_HINT.to_string()
    }
}

/// 非 Linux 平台的检查列表。
///
/// id 与标题和 Linux 侧保持一致，状态一律为「未覆盖」，并写明原因：
/// 这些能力的真实实现由后续的 Windows / macOS ticket 提供，本平台目前只保证能编译。
#[cfg(not(target_os = "linux"))]
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
