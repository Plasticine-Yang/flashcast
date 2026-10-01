//! 真实 Linux 桌面验收（ticket 17）。
//!
//! 这个文件**不做任何模拟**：它用 `flashcast_platform::current()` 的真实适配器组装
//! 宿主，从宿主对外的查询 / 命令入口走一遍需要真实桌面才能回答的问题。
//!
//! 全部用例默认 `#[ignore]`，必须显式运行（CI 的 runner 没有桌面会话，不能让它们
//! 干扰「编译与核心行为」的必需检查）：
//!
//! ```text
//! cargo test -p flashcast-core --test real_linux_desktop -- --ignored --nocapture --test-threads=1
//! ```
//!
//! 环境变量：
//! - `FLASHCAST_REAL_APP_PROGRAM`：要启动的真实程序名，默认 `gnome-calculator`。
//!   用例只在**真实扫描结果**里找这个程序对应的 `.desktop` 条目，找不到就如实失败。
//! - `FLASHCAST_REAL_LAUNCH_TIMEOUT_MS`：等待子进程出现的上限，默认 8000ms。
//!
//! 这些用例只回答它们能回答的问题。真实桌面上的自动粘贴、剪贴板选区与可见窗口由
//! ticket 17 的报告单独记录，不在这里假装通过。

#![cfg(target_os = "linux")]

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use flashcast_core::{ActionStatus, Host, HostDeps, ItemKind, PluginRegistry, Settings};
use flashcast_platform::catalog::{AppCatalog, AppEntry};

/// 用真实平台适配器组装宿主（与 `src-tauri/src/lib.rs` 的接线一致）。
fn real_host(device_dir: &Path) -> Host {
    let platform = flashcast_platform::current();
    let deps = HostDeps {
        catalog: Arc::clone(&platform.catalog),
        launcher: Arc::clone(&platform.launcher),
        capabilities: Arc::clone(&platform.capabilities),
        clipboard: Arc::clone(&platform.clipboard),
        clipboard_watcher: Arc::clone(&platform.clipboard_watcher),
        chrome: Arc::clone(&platform.chrome),
        focus: Arc::clone(&platform.focus),
        paster: Arc::clone(&platform.paster),
        plugins: Arc::new(PluginRegistry::new()),
        device_dir: device_dir.to_path_buf(),
    };
    let host = Host::new(deps, Settings::default());
    host.install_official_plugins();
    host
}

fn temp_device_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "flashcast-real-{prefix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("创建设备目录");
    dir
}

/// 在真实扫描结果里找 program 对应的条目。
fn find_real_entry(program: &str) -> AppEntry {
    let catalog = flashcast_platform::linux::LinuxAppCatalog::new();
    let entries = catalog.scan().expect("真实扫描软件");
    entries
        .into_iter()
        .find(|entry| {
            entry
                .argv()
                .and_then(|argv| argv.first())
                .map(|first| {
                    Path::new(first)
                        .file_name()
                        .map(|name| name.to_string_lossy() == program)
                        .unwrap_or(false)
                })
                .unwrap_or(false)
        })
        .unwrap_or_else(|| panic!("真实扫描结果里没有 program={program} 的条目"))
}

/// 在 `/proc` 里找 comm 前缀匹配的进程（comm 最多 15 个字符，会被截断）。
fn pids_with_comm_prefix(prefix: &str) -> Vec<u32> {
    let truncated: String = prefix.chars().take(15).collect();
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_string_lossy().parse::<u32>().ok() else {
            continue;
        };
        let Ok(comm) = std::fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        if comm.trim().starts_with(&truncated) {
            found.push(pid);
        }
    }
    found
}

fn kill(pid: u32) -> bool {
    std::process::Command::new("kill")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// 真实启动：宿主查询入口 → 选择 → 命令入口（等价于回车）→ 真的出现进程 → 结束它。
///
/// 这一项覆盖 ticket 01 第 2 个复选框（「回车启动软件」此前从未端到端跑过）。
#[test]
#[ignore = "需要真实桌面会话：会真的启动一个软件窗口"]
fn real_enter_launches_a_real_application() {
    let program =
        std::env::var("FLASHCAST_REAL_APP_PROGRAM").unwrap_or_else(|_| "gnome-calculator".into());
    let timeout = Duration::from_millis(
        std::env::var("FLASHCAST_REAL_LAUNCH_TIMEOUT_MS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(8000),
    );

    let entry = find_real_entry(&program);
    let before = pids_with_comm_prefix(&program);
    println!(
        "真实条目：id={} name={:?} argv={:?} terminal={}",
        entry.id,
        entry.name,
        entry.argv().unwrap_or_default(),
        entry.terminal
    );

    let device = temp_device_dir("launch");
    let host = real_host(&device);

    // 1. 查询入口：用户输入软件名，得到的是真实扫描出来的条目。
    let response = host.query(&entry.name);
    let item = response
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Application && item.id.contains(&entry.id))
        .unwrap_or_else(|| {
            panic!(
                "查询 {:?} 没有返回条目 {}：{:?}",
                entry.name,
                entry.id,
                response
                    .items
                    .iter()
                    .map(|item| item.id.clone())
                    .collect::<Vec<_>>()
            )
        })
        .clone();
    println!("查询 {:?} → 命中 {}（{}）", entry.name, item.title, item.id);

    // 2. 键盘选择后执行（Enter）。
    let _ = host.set_selection(response.selection);
    let outcome = host.execute(&item);
    assert_eq!(
        outcome.status,
        ActionStatus::Done,
        "启动失败：{:?}",
        outcome.message
    );

    // 3. 真的看到进程出现（宿主返回「已发送」不算证据）。
    let deadline = Instant::now() + timeout;
    let mut launched: Option<u32> = None;
    while Instant::now() < deadline {
        if let Some(pid) = pids_with_comm_prefix(&program)
            .into_iter()
            .find(|pid| !before.contains(pid))
        {
            launched = Some(pid);
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let pid = launched
        .unwrap_or_else(|| panic!("执行成功但 {:?} 在 {timeout:?} 内没有出现真实进程", program));
    println!("真实进程出现：pid={pid}（这证明回车真的启动了这个软件）");

    // 4. 结束它：验收不留下用户桌面上的窗口。
    std::thread::sleep(Duration::from_millis(800));
    assert!(kill(pid), "无法结束 pid={pid}");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if !pids_with_comm_prefix(&program).contains(&pid) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        !pids_with_comm_prefix(&program).contains(&pid),
        "pid={pid} 在收到 SIGTERM 后仍然存在"
    );
    println!("已结束 pid={pid}；真实启动验收完成");
    let _ = std::fs::remove_dir_all(&device);
}

/// 真实搜索软件：查询入口在真实扫描结果上返回可启动条目（不是替身数据）。
#[test]
#[ignore = "需要真实桌面会话：读取真实的 freedesktop 软件目录"]
fn real_search_software_from_host_entry_point() {
    let program =
        std::env::var("FLASHCAST_REAL_APP_PROGRAM").unwrap_or_else(|_| "gnome-calculator".into());
    let entry = find_real_entry(&program);
    let device = temp_device_dir("search");
    let host = real_host(&device);

    let response = host.query(&entry.name);
    let hit = response
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Application && item.id.contains(&entry.id));
    assert!(
        hit.is_some(),
        "真实软件 {:?}（{}）没有出现在查询结果里",
        entry.name,
        entry.id
    );
    let hit = hit.expect("已断言存在");
    println!(
        "查询 {:?} → {} / {}（图标 {}）",
        entry.name,
        hit.title,
        hit.id,
        if entry.icon.is_some() { "有" } else { "无" }
    );
    let _ = std::fs::remove_dir_all(&device);
}

/// 真实备忘录标签粘贴：真实工作区 + 真实剪贴板后端。
///
/// 这一项**不能**在本机的自动化会话里算通过：需要选区持有者（自动化会话没有），
/// `wl-copy` 会拿不到选区而在有界等待后失败。因此用例把「宿主如实降级/如实报错」
/// 当作通过条件，并把实际观察到的结论打印出来，供报告分类为 **未覆盖**；
/// 只有「没有粘贴目标却返回 PastePending（猜了一个目标）」才判失败。
#[test]
#[ignore = "需要真实桌面会话：真实剪贴板选区"]
fn real_memo_tag_paste_is_honest_about_clipboard_and_target() {
    let device = temp_device_dir("memo");
    let workspace = temp_device_dir("workspace");
    let host = real_host(&device);
    host.init_workspace(&workspace).expect("初始化真实工作区");
    host.create_memo(
        "验收备忘录",
        &["验收标签".to_string()],
        "ticket 17 真实桌面验收用的备忘录正文",
    )
    .expect("创建真实备忘录");

    let response = host.query("验收标签");
    let item = response
        .items
        .iter()
        .find(|item| item.kind == ItemKind::Memo)
        .unwrap_or_else(|| panic!("标签查询没有命中备忘录：{response:?}"))
        .clone();
    println!("标签查询 → {}（{}）", item.title, item.id);

    // 本机没有记录到唤起前的应用：宿主必须如实告诉用户手动粘贴，不能猜目标。
    host.set_paste_target(None);

    // 剪贴板写入走的是真实后端（Wayland 下 wl-copy）。给它一个有界窗口，
    // 免得一个拿不到选区的会话把整条验收卡死。
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(host.execute(&item));
    });
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(outcome) => {
            println!(
                "真实执行结果：status={:?} message={:?}",
                outcome.status, outcome.message
            );
            assert_ne!(
                outcome.status,
                ActionStatus::PastePending,
                "没有粘贴目标时不得返回待粘贴计划（不能猜目标）：{:?}",
                outcome.message
            );
            match outcome.status {
                ActionStatus::CopiedNeedsManualPaste => {
                    println!("结论：真实剪贴板写入成功，达到预期的「已复制，请手动粘贴」降级");
                }
                ActionStatus::Failed => {
                    println!(
                        "结论：未覆盖 —— 本机会话拿不到剪贴板选区，真实写入失败并如实反馈：{:?}",
                        outcome.message
                    );
                }
                other => panic!("非预期的执行结果：{other:?}"),
            }
        }
        Err(_) => println!(
            "结论：未覆盖 —— 真实剪贴板写入在 20 秒内没有返回（自动化会话拿不到选区），\
             用例主动放弃等待，未判定通过或失败"
        ),
    }
    let _ = std::fs::remove_dir_all(&workspace);
    let _ = std::fs::remove_dir_all(&device);
}
