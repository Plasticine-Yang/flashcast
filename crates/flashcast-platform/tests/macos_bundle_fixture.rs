//! macOS `.app` 包解析与扫描的夹具测试。
//!
//! 这些测试**不依赖 macOS API**：它们用临时目录中的真实 `Info.plist` 与
//! `Contents/MacOS` 结构驱动 `flashcast_platform::macos::bundle` 的真实解析与
//! 扫描实现，因此可以在 Linux 开发机与任意 CI runner 上运行。
//!
//! 磁盘上真实的图标渲染（`NSWorkspace` → PNG）与启动、焦点、快捷键全部是
//! macOS 专有行为，不在本文件覆盖范围内，只能由 macOS runner 上的
//! `flashcast-platform-check` 报告。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flashcast_platform::catalog::AppSource;
use flashcast_platform::macos::bundle::{
    default_roots, entry_from_bundle, finalize_bundles, read_bundle, scan, SkipReason,
    MAX_SCAN_DEPTH,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 一个用完即删的临时目录。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "flashcast-macos-test-{}-{}-{}",
            std::process::id(),
            label,
            unique
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(&target, content).expect("写入夹具文件");
        target
    }

    fn touch(&self, relative: &str) -> PathBuf {
        self.write(relative, "fixture")
    }

    /// 写一个最小可用的 `.app` 包：`Info.plist` 与真实存在的可执行文件。
    fn app(&self, relative: &str, bundle_id: &str, display_name: &str) -> PathBuf {
        let executable = Path::new(relative)
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_else(|| "App".to_string());
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key><string>{bundle_id}</string>
    <key>CFBundleName</key><string>{display_name}</string>
    <key>CFBundleDisplayName</key><string>{display_name}</string>
    <key>CFBundleExecutable</key><string>{executable}</string>
    <key>CFBundlePackageType</key><string>APPL</string>
</dict>
</plist>
"#
        );
        self.write(&format!("{relative}/Contents/Info.plist"), &plist);
        self.touch(&format!("{relative}/Contents/MacOS/{executable}"));
        self.path.join(relative)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn paths(entries: &[flashcast_platform::catalog::AppEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| entry.name.clone())
        .collect::<Vec<_>>()
}

// ---------------------------------------------------------------------------
// 扫描根目录
// ---------------------------------------------------------------------------

// 下面两个用例断言的是 POSIX 路径语义（`/Applications`、`is_relative` 的含义）。
// Windows 上 `Path::is_absolute()` 还要求盘符前缀，因此 `\Applications` 反而算“相对路径”，
// 这些断言在 Windows 上不成立；而 `default_roots` 产出的本来就是 macOS 专用路径，
// 所以在非 unix 目标上跳过，而不是把断言改成迁就 Windows 的写法。
#[cfg(unix)]
#[test]
fn default_roots_cover_system_and_user_locations() {
    let home = Path::new("/Users/tester");
    let roots = default_roots(Some(home));
    let as_strings: Vec<String> = roots
        .iter()
        .map(|root| root.to_string_lossy().into_owned())
        .collect();
    for expected in [
        "/Applications",
        "/System/Applications",
        "/System/Applications/Utilities",
        "/Applications/Utilities",
        "/Users/tester/Applications",
    ] {
        assert!(
            as_strings.iter().any(|root| root == expected),
            "缺少根目录 {expected}，实际为 {as_strings:?}"
        );
    }
}

#[cfg(unix)]
#[test]
fn default_roots_without_home_skip_the_user_directory() {
    let roots = default_roots(None);
    assert!(
        !roots.iter().any(|root| root.starts_with("/Users")
            || root.ends_with("Applications") && root.is_relative()),
        "没有 HOME 时不应推断用户应用目录：{roots:?}"
    );
    assert_eq!(roots.len(), 4);
}

// ---------------------------------------------------------------------------
// 目录遍历
// ---------------------------------------------------------------------------

#[test]
fn scan_finds_bundles_one_and_two_levels_deep() {
    let dir = TempDir::new("depth");
    dir.app("Applications/Safari.app", "com.apple.Safari", "Safari");
    dir.app(
        "Applications/Adobe Photoshop 2024/Adobe Photoshop.app",
        "com.adobe.Photoshop",
        "Adobe Photoshop",
    );
    dir.app(
        "Applications/Utilities/Terminal.app",
        "com.apple.Terminal",
        "Terminal",
    );

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    let mut names = paths(&outcome.entries);
    names.sort();
    assert_eq!(
        names,
        vec!["Adobe Photoshop", "Safari", "Terminal"],
        "跳过项：{:?}",
        outcome
            .skipped
            .iter()
            .map(|s| (s.path.clone(), s.reason.as_str()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn scan_does_not_recurse_into_a_bundle_or_beyond_max_depth() {
    let dir = TempDir::new("bounded");
    dir.app("Applications/Safari.app", "com.apple.Safari", "Safari");
    // `.app` 内部的嵌套 bundle 必须被忽略（不递归进入已发现的包）。
    dir.app(
        "Applications/Safari.app/Contents/Resources/Helper.app",
        "com.example.Helper",
        "Helper",
    );
    // 超过深度上限的包必须被忽略。
    dir.app(
        "Applications/Deep/L1/L2/L3/TooDeep.app",
        "com.example.TooDeep",
        "TooDeep",
    );

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    assert_eq!(paths(&outcome.entries), vec!["Safari"]);
}

#[test]
fn scan_of_overlapping_roots_does_not_duplicate_bundles() {
    let dir = TempDir::new("overlap");
    dir.app(
        "Applications/Utilities/Terminal.app",
        "com.apple.Terminal",
        "Terminal",
    );

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![
            dir.path().join("Applications"),
            dir.path().join("Applications/Utilities"),
        ],
        max_depth: MAX_SCAN_DEPTH,
    });

    assert_eq!(paths(&outcome.entries), vec!["Terminal"]);
}

#[test]
fn scan_skips_non_application_bundles_and_broken_bundles() {
    let dir = TempDir::new("broken");
    dir.app("Applications/Good.app", "com.example.Good", "Good");
    // CFBundlePackageType 不是 APPL 且没有可执行文件。
    dir.write(
        "Applications/NotApp.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.NotApp</string>
    <key>CFBundleName</key><string>NotApp</string>
    <key>CFBundlePackageType</key><string>BNDL</string>
</dict></plist>
"#,
    );
    // CFBundleExecutable 指向不存在的文件。
    dir.write(
        "Applications/Broken.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.Broken</string>
    <key>CFBundleName</key><string>Broken</string>
    <key>CFBundleExecutable</key><string>Broken</string>
    <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#,
    );
    // 缺少 Info.plist。
    dir.touch("Applications/Empty.app/Contents/MacOS/Empty");
    // 后台进程包（LSBackgroundOnly）不应出现在启动器里。
    dir.write(
        "Applications/Agent.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.Agent</string>
    <key>CFBundleName</key><string>Agent</string>
    <key>CFBundleExecutable</key><string>Agent</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>LSBackgroundOnly</key><true/>
</dict></plist>
"#,
    );
    dir.touch("Applications/Agent.app/Contents/MacOS/Agent");

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    assert_eq!(paths(&outcome.entries), vec!["Good"]);

    let reasons: Vec<(String, SkipReason)> = outcome
        .skipped
        .iter()
        .map(|skipped| {
            (
                skipped
                    .path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                skipped.reason,
            )
        })
        .collect();
    for (expected, reason) in [
        ("NotApp.app", SkipReason::NotAnApplication),
        ("Broken.app", SkipReason::MissingExecutable),
        ("Empty.app", SkipReason::MissingInfoPlist),
        ("Agent.app", SkipReason::BackgroundOnly),
    ] {
        assert!(
            reasons
                .iter()
                .any(|(name, actual)| name == expected && *actual == reason),
            "未按预期跳过 {expected}（{reason:?}），实际：{reasons:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Info.plist 字段映射
// ---------------------------------------------------------------------------

#[test]
fn plist_fields_map_to_bundle_metadata() {
    let dir = TempDir::new("fields");
    dir.write(
        "Applications/Reader.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.Reader</string>
    <key>CFBundleName</key><string>ReaderShort</string>
    <key>CFBundleDisplayName</key><string>Reader 阅读器</string>
    <key>CFBundleExecutable</key><string>ReaderBin</string>
    <key>CFBundleIconFile</key><string>ReaderIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>3.1.4</string>
</dict></plist>
"#,
    );
    dir.touch("Applications/Reader.app/Contents/MacOS/ReaderBin");
    dir.touch("Applications/Reader.app/Contents/Resources/ReaderIcon.icns");

    let bundle = read_bundle(&dir.path().join("Applications/Reader.app")).expect("读取应用包");
    assert_eq!(bundle.bundle_id.as_deref(), Some("com.example.Reader"));
    assert_eq!(bundle.name, "Reader 阅读器");
    assert_eq!(bundle.executable.as_deref(), Some("ReaderBin"));
    assert_eq!(bundle.version.as_deref(), Some("3.1.4"));
    assert_eq!(bundle.icon_file.as_deref(), Some("ReaderIcon"));
    assert_eq!(
        bundle.icon_path.as_deref(),
        Some(
            dir.path()
                .join("Applications/Reader.app/Contents/Resources/ReaderIcon.icns")
                .as_path()
        )
    );

    let entry = entry_from_bundle(&bundle);
    assert_eq!(entry.name, "Reader 阅读器");
    assert_eq!(entry.exec, vec![bundle.path.to_string_lossy().into_owned()]);
    assert_eq!(entry.wm_class.as_deref(), Some("com.example.Reader"));
    assert_eq!(entry.source, AppSource::Bundle);
    assert!(entry.keywords.contains(&"com.example.Reader".to_string()));
    assert!(entry
        .desktop_file
        .as_ref()
        .map(|path| path.ends_with("Contents/Info.plist"))
        .unwrap_or(false));
}

#[test]
fn display_name_falls_back_to_bundle_name_then_file_stem() {
    let dir = TempDir::new("names");
    // 缺少 CFBundleDisplayName → 用 CFBundleName。
    dir.write(
        "Applications/OnlyName.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleIdentifier</key><string>com.example.OnlyName</string>
    <key>CFBundleName</key><string>OnlyName</string>
    <key>CFBundleExecutable</key><string>OnlyName</string>
    <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#,
    );
    dir.touch("Applications/OnlyName.app/Contents/MacOS/OnlyName");
    // 两个名称键都缺失 → 用包目录名（去掉 .app）。
    dir.write(
        "Applications/StemName.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>StemName</string>
    <key>CFBundlePackageType</key><string>APPL</string>
</dict></plist>
"#,
    );
    dir.touch("Applications/StemName.app/Contents/MacOS/StemName");

    let only_name = read_bundle(&dir.path().join("Applications/OnlyName.app")).expect("读取");
    assert_eq!(only_name.name, "OnlyName");
    assert_eq!(only_name.bundle_id.as_deref(), Some("com.example.OnlyName"));

    let stem_name = read_bundle(&dir.path().join("Applications/StemName.app")).expect("读取");
    assert_eq!(stem_name.name, "StemName");
    assert_eq!(stem_name.bundle_id, None);
}

#[test]
fn icon_file_extension_is_optional() {
    let dir = TempDir::new("icon-ext");
    dir.touch("Applications/WithExt.app/Contents/Resources/App.icns");
    dir.touch("Applications/WithExt.app/Contents/Resources/Other.icns");
    dir.write(
        "Applications/WithExt.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>WithExt</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleIconFile</key><string>App.icns</string>
</dict></plist>
"#,
    );
    dir.touch("Applications/WithExt.app/Contents/MacOS/WithExt");

    let with_ext = read_bundle(&dir.path().join("Applications/WithExt.app")).expect("读取");
    assert_eq!(
        with_ext.icon_path.as_deref(),
        Some(
            dir.path()
                .join("Applications/WithExt.app/Contents/Resources/App.icns")
                .as_path()
        )
    );

    // 指定的图标文件不存在时不猜测：保持 None，由 NSWorkspace 渲染兜底。
    dir.write(
        "Applications/Missing.app/Contents/Info.plist",
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
    <key>CFBundleExecutable</key><string>Missing</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleIconFile</key><string>Gone</string>
</dict></plist>
"#,
    );
    dir.touch("Applications/Missing.app/Contents/MacOS/Missing");
    let missing = read_bundle(&dir.path().join("Applications/Missing.app")).expect("读取");
    assert_eq!(missing.icon_path, None);
}

#[test]
fn non_ascii_bundle_names_are_preserved() {
    let dir = TempDir::new("non-ascii");
    dir.app("Applications/深度工具.app", "com.example.深度", "深度工具");

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    assert_eq!(paths(&outcome.entries), vec!["深度工具"]);
    assert_eq!(outcome.entries[0].id, "com.example.深度");
    assert_eq!(
        outcome.entries[0].wm_class.as_deref(),
        Some("com.example.深度")
    );
}

// ---------------------------------------------------------------------------
// 重复 bundle id 与稳定身份
// ---------------------------------------------------------------------------

#[test]
fn duplicate_bundle_ids_keep_both_bundles_with_distinct_names_and_ids() {
    let dir = TempDir::new("duplicates");
    dir.app("Applications/Acme.app", "com.example.Acme", "Acme");
    dir.app("Applications/Beta/Acme.app", "com.example.Acme", "Acme");

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    assert_eq!(outcome.entries.len(), 2, "重复 bundle id 不得丢弃任何一份");
    let mut names = paths(&outcome.entries);
    names.sort();
    assert_eq!(names, vec!["Acme", "Acme（Beta）"]);
    let ids: Vec<&str> = outcome
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    assert_eq!(
        ids.iter().collect::<std::collections::BTreeSet<_>>().len(),
        2,
        "重复 bundle id 的条目 id 必须互不相同：{ids:?}"
    );
    assert!(
        ids.contains(&"com.example.Acme"),
        "主副本使用纯 bundle id：{ids:?}"
    );
}

#[test]
fn unique_bundle_id_is_used_verbatim_as_entry_id() {
    let dir = TempDir::new("unique-id");
    dir.app("Applications/Solo.app", "com.example.Solo", "Solo");
    dir.app("Applications/Other.app", "com.example.Other", "Other");

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    let by_name: std::collections::BTreeMap<&str, &str> = outcome
        .entries
        .iter()
        .map(|entry| (entry.name.as_str(), entry.id.as_str()))
        .collect();
    assert_eq!(by_name["Solo"], "com.example.Solo");
    assert_eq!(by_name["Other"], "com.example.Other");
}

#[test]
fn finalize_is_deterministic_regardless_of_input_order() {
    let dir = TempDir::new("determinism");
    dir.app("Applications/Acme.app", "com.example.Acme", "Acme");
    dir.app("Applications/Beta/Acme.app", "com.example.Acme", "Acme");

    let mut first = vec![
        read_bundle(&dir.path().join("Applications/Acme.app")).expect("读取"),
        read_bundle(&dir.path().join("Applications/Beta/Acme.app")).expect("读取"),
    ];
    let mut second = vec![first[1].clone(), first[0].clone()];
    finalize_bundles(&mut first);
    finalize_bundles(&mut second);

    let first_ids: std::collections::BTreeMap<String, String> = first
        .iter()
        .map(|bundle| (bundle.entry_id.clone(), bundle.name.clone()))
        .collect();
    let second_ids: std::collections::BTreeMap<String, String> = second
        .iter()
        .map(|bundle| (bundle.entry_id.clone(), bundle.name.clone()))
        .collect();
    assert_eq!(
        first_ids, second_ids,
        "同一组应用包的 id 与名称必须与扫描顺序无关"
    );
}

#[test]
fn scan_entries_are_sorted_by_id() {
    let dir = TempDir::new("sorted");
    dir.app("Applications/Zeta.app", "com.example.Zeta", "Zeta");
    dir.app("Applications/Alpha.app", "com.example.Alpha", "Alpha");
    dir.app("Applications/Middle.app", "com.example.Middle", "Middle");

    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("Applications")],
        max_depth: MAX_SCAN_DEPTH,
    });

    let ids: Vec<&str> = outcome
        .entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

#[test]
fn scan_reports_missing_roots_without_failing() {
    let dir = TempDir::new("missing-root");
    let outcome = scan(&flashcast_platform::macos::bundle::ScanOptions {
        roots: vec![dir.path().join("DoesNotExist")],
        max_depth: MAX_SCAN_DEPTH,
    });
    assert!(outcome.entries.is_empty());
    assert!(outcome.skipped.is_empty());
    assert!(outcome.warnings.is_empty());
}

// ---------------------------------------------------------------------------
// 启动计划（纯逻辑，macOS 上按同一份计划执行）
// ---------------------------------------------------------------------------

use flashcast_platform::macos::launcher::{is_bundle_path, launch_plan, LaunchPlan};

fn plan(program: &str, args: &[&str], terminal: bool, is_bundle: bool) -> LaunchPlan {
    let args: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    launch_plan(program, &args, terminal, is_bundle).expect("计划应可生成")
}

#[test]
fn bundle_launch_uses_the_path_not_the_bundle_id() {
    // 重复 bundle id 真实存在，按路径打开才不会命中另一份副本。
    let plan = plan("/Applications/Acme.app", &[], false, true);
    assert_eq!(
        plan,
        LaunchPlan::Open {
            argv: vec![
                "/usr/bin/open".into(),
                "-a".into(),
                "/Applications/Acme.app".into()
            ]
        }
    );
}

#[test]
fn bundle_launch_passes_arguments_after_the_args_separator() {
    let plan = plan("/Applications/Acme.app", &["--flag", "值"], false, true);
    assert_eq!(
        plan,
        LaunchPlan::Open {
            argv: vec![
                "/usr/bin/open".into(),
                "-a".into(),
                "/Applications/Acme.app".into(),
                "--args".into(),
                "--flag".into(),
                "值".into(),
            ]
        }
    );
}

#[test]
fn non_bundle_program_is_executed_directly() {
    let plan = plan("/opt/tool/bin/tool", &["--x"], false, false);
    assert_eq!(
        plan,
        LaunchPlan::Exec {
            program: "/opt/tool/bin/tool".into(),
            args: vec!["--x".into()]
        }
    );
}

#[test]
fn terminal_request_goes_through_terminal_app() {
    let plan = plan("/opt/tool/bin/tool", &[], true, false);
    assert_eq!(
        plan,
        LaunchPlan::Open {
            argv: vec![
                "/usr/bin/open".into(),
                "-a".into(),
                "Terminal".into(),
                "/opt/tool/bin/tool".into()
            ]
        }
    );
}

#[test]
fn terminal_request_with_arguments_fails_instead_of_dropping_them() {
    let args = vec!["--x".to_string()];
    let error =
        launch_plan("/opt/tool/bin/tool", &args, true, false).expect_err("不能静默丢弃参数");
    assert!(
        error.to_string().contains("无法传递额外参数"),
        "错误说明应指出参数无法传递：{error}"
    );
}

#[test]
fn empty_program_is_rejected() {
    assert!(launch_plan("   ", &[], false, false).is_err());
}

#[test]
fn bundle_detection_requires_an_existing_app_directory() {
    let dir = TempDir::new("bundle-detect");
    let app = dir.app("Applications/Acme.app", "com.example.Acme", "Acme");
    assert!(is_bundle_path(&app));
    assert!(!is_bundle_path(
        &dir.path().join("Applications/Missing.app")
    ));
    assert!(!is_bundle_path(&dir.path().join("Applications/plain.txt")));
}

// ---------------------------------------------------------------------------
// 焦点恢复策略、辅助功能结论与图标缓存路径
// ---------------------------------------------------------------------------

use flashcast_platform::capability::{OsKind, SessionType, Support};
use flashcast_platform::focus::FocusedApp;
use flashcast_platform::macos::cap::{capabilities_for, MacosEnvironment};
use flashcast_platform::macos::focus::restore_target;
use flashcast_platform::macos::icons::{icon_cache_dir, icon_cache_path};

fn focused(pid: Option<u32>, bundle_id: Option<&str>) -> FocusedApp {
    FocusedApp {
        id: bundle_id.unwrap_or("unknown").to_string(),
        name: "唤起前应用".to_string(),
        wm_class: bundle_id.map(str::to_string),
        pid,
        window: None,
    }
}

#[test]
fn restore_prefers_pid_then_falls_back_to_bundle_id() {
    assert_eq!(
        restore_target(&focused(Some(4321), Some("com.example.Acme"))),
        Some(flashcast_platform::macos::focus::RestoreTarget::Pid(4321))
    );
    assert_eq!(
        restore_target(&focused(None, Some("com.example.Acme"))),
        Some(flashcast_platform::macos::focus::RestoreTarget::BundleId(
            "com.example.Acme".to_string()
        ))
    );
    // 两者都没有时如实返回 None，由调用方报「无法恢复」。
    assert_eq!(restore_target(&focused(Some(0), None)), None);
}

#[test]
fn capability_snapshot_reports_macos_without_claiming_unimplemented_features() {
    let caps = capabilities_for(
        MacosEnvironment {
            desktop_available: true,
            accessibility_granted: false,
        },
        Some("Version 14.5 (Build 23F79)".to_string()),
        "aarch64".to_string(),
    );
    assert_eq!(caps.os, OsKind::Macos);
    assert_eq!(caps.session, SessionType::NotApplicable);
    assert!(caps.desktop_available);
    assert_eq!(caps.hotkey, Support::Supported);
    // 剪贴板适配属于后续 ticket：只能报「未覆盖」，不得写成支持。
    assert!(matches!(caps.clipboard, Support::Unknown { .. }));
    // 没有辅助功能权限时自动粘贴必须如实报「不支持」，并给出设置入口。
    let reason = caps.auto_paste.reason().expect("应有原因");
    assert!(
        reason.contains("辅助功能"),
        "原因应指向辅助功能权限：{reason}"
    );
    assert!(reason.contains("系统设置"), "原因应给出设置入口：{reason}");
    assert!(caps
        .notes
        .iter()
        .any(|note| note.contains("辅助功能权限：未授权")));
}

#[test]
fn granted_accessibility_still_does_not_claim_auto_paste_works() {
    let caps = capabilities_for(
        MacosEnvironment {
            desktop_available: true,
            accessibility_granted: true,
        },
        None,
        "x86_64".to_string(),
    );
    assert_eq!(caps.hotkey, Support::Supported);
    assert!(
        matches!(caps.auto_paste, Support::Unknown { .. }),
        "适配未实现时只能是未覆盖：{:?}",
        caps.auto_paste
    );
    assert!(caps
        .notes
        .iter()
        .any(|note| note.contains("辅助功能权限：已授权")));
}

#[test]
fn headless_environment_reports_hotkey_unsupported() {
    let caps = capabilities_for(
        MacosEnvironment {
            desktop_available: false,
            accessibility_granted: false,
        },
        None,
        "aarch64".to_string(),
    );
    assert!(!caps.desktop_available);
    assert!(matches!(caps.hotkey, Support::Unsupported { .. }));
}

#[test]
fn icon_cache_paths_are_stable_and_distinct() {
    let cache = Path::new("/tmp/flashcast-icon-cache");
    let first = icon_cache_path(Path::new("/Applications/Acme.app"), cache);
    let again = icon_cache_path(Path::new("/Applications/Acme.app"), cache);
    let other = icon_cache_path(Path::new("/Applications/Other.app"), cache);
    assert_eq!(first, again, "同一路径必须得到同一缓存文件");
    assert_ne!(first, other, "不同路径必须得到不同缓存文件");
    assert_eq!(first.extension().and_then(|e| e.to_str()), Some("png"));
    assert_eq!(first.parent(), Some(cache));

    assert_eq!(
        icon_cache_dir(Some(Path::new("/Users/tester"))),
        Path::new("/Users/tester/Library/Caches/Flashcast/icons")
    );
}
