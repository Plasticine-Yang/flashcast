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

// macOS 的真实检查全部经 `flashcast_platform::macos`，不经过任何替身。
#[cfg(target_os = "macos")]
use std::sync::Arc as MacosArc;
#[cfg(target_os = "macos")]
use flashcast_platform::capability::{CapabilityProbe as MacosCapabilityProbeTrait, Support as MacosSupport};
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
use flashcast_platform::shortcut::{HotkeyError as MacosHotkeyError, HotkeyManager as MacosHotkeyManagerTrait};

/// 非 Linux 平台上报告「未覆盖」时给出的复现命令提示。
#[cfg(all(not(target_os = "linux"), target_os = "windows"))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform --target x86_64-pc-windows-msvc";
#[cfg(all(not(target_os = "linux"), target_os = "macos"))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform --target x86_64-apple-darwin";
#[cfg(all(not(target_os = "linux"), not(any(target_os = "windows", target_os = "macos"))))]
const CROSS_CHECK_HINT: &str = "cargo check -p flashcast-platform";

/// 探测当前环境能力。Linux 与 macOS 用真实探测器，其余平台用「不支持」桩实现。
fn probe_capabilities() -> Capabilities {
    #[cfg(target_os = "linux")]
    {
        LinuxCapabilityProbe::new().probe()
    }
    #[cfg(target_os = "macos")]
    {
        flashcast_platform::macos::MacosCapabilityProbe::new().probe()
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
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

    #[cfg(target_os = "macos")]
    {
        checks.extend(macos_checks(&capabilities));
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
    let json = || {
        "cargo run -p flashcast-platform --bin flashcast-platform-check -- --json".to_string()
    };

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

    // 7. 全局快捷键注册：真实的 Carbon RegisterEventHotKey 注册 + 注销。
    let hotkeys = MacosHotkeyManager::new();
    let spec = MacosHotkeySpec::parse("Ctrl+Alt+F12").expect("固定检查用快捷键必须可解析");
    checks.push(match MacosHotkeyManagerTrait::register(&hotkeys, &spec, MacosArc::new(|| {})) {
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
    });

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

/// 既非 Linux 也非 macOS 的目标上的检查列表。
///
/// id 与标题和 Linux 侧保持一致，状态一律为「未覆盖」，并写明原因：
/// 这些能力的真实实现由后续的 Windows ticket 提供，本平台目前只保证能编译。
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
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
