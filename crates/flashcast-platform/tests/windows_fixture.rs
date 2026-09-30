//! Windows 纯逻辑层的真实夹具测试。
//!
//! 这些测试**不依赖任何 Windows API**，因此在 Linux（与无桌面会话的 CI runner）上
//! 都会真实运行。它们驱动的不是替身，而是：
//!
//! - `lnk 0.6.4` 对**真实 `.lnk` 字节**的解析（夹具由本文件按 MS-SHLLINK 规范生成）；
//! - 本 crate 的 `windows::shell_link` / `start_menu` / `registry` / `uwp` /
//!   `icons` / `identity` / `launch_plan` / `version` 纯逻辑。
//!
//! 这里证明不了任何 Windows API 的行为：`IShellLinkW` 纠正 pass、`GetImage` 取图标、
//! `RegisterHotKey`、`SetForegroundWindow` 都只能在 Windows runner 上被真实检查覆盖。

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use flashcast_platform::catalog::{AppEntry, AppSource, IconRef};
use flashcast_platform::windows::icons::{
    encode_png, icon_cache_key, is_blank, unpremultiply_bgra_in_place,
};
use flashcast_platform::windows::identity::{aumid_id, dedupe_entries, exe_id};
use flashcast_platform::windows::launch_plan::{plan, LaunchPlan};
use flashcast_platform::windows::registry::{classify, launch_target, UninstallValues, Verdict};
use flashcast_platform::windows::shell_link::{
    choose_target, classify_target, code_page_encoding, expand_env, needs_shell_correction,
    open_fields, resolve_relative, TargetKind,
};
use flashcast_platform::windows::start_menu::{discover_shortcuts, roots_from_env};
use flashcast_platform::windows::uwp::{
    aumid_from_program, is_launchable_aumid, parse_get_start_apps_json,
};

static COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 用完即删的临时目录。
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new(label: &str) -> Self {
        let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
        let path = std::env::temp_dir().join(format!(
            "flashcast-win-{}-{label}-{unique}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("创建临时目录");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, bytes: &[u8]) -> PathBuf {
        let target = self.path.join(relative);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).expect("创建父目录");
        }
        std::fs::write(&target, bytes).expect("写入夹具");
        target
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

// ---------------------------------------------------------------------------
// 按 MS-SHLLINK 规范生成一个真实的 .lnk 字节流。
// ---------------------------------------------------------------------------

/// ShellLinkHeader 的 LinkFlags。
const HAS_LINK_INFO: u32 = 0x0000_0002;
const HAS_NAME: u32 = 0x0000_0004;
const HAS_RELATIVE_PATH: u32 = 0x0000_0008;
const HAS_WORKING_DIR: u32 = 0x0000_0010;
const HAS_ARGUMENTS: u32 = 0x0000_0020;
const HAS_ICON_LOCATION: u32 = 0x0000_0040;
const IS_UNICODE: u32 = 0x0000_0080;

/// 生成一个带 `LinkInfo`（ANSI `LocalBasePath`）与 Unicode `StringData` 的 `.lnk`。
///
/// 布局：76 字节的 ShellLinkHeader + LinkInfo（含 VolumeID、LocalBasePath、
/// CommonPathSuffix）+ StringData（NAME / RELATIVE_PATH / WORKING_DIR / ARGUMENTS /
/// ICON_LOCATION），不含 ExtraData。
fn build_lnk(
    target: &str,
    relative: &str,
    arguments: &str,
    working_dir: &str,
    icon_location: &str,
    display_name: &str,
) -> Vec<u8> {
    let flags = HAS_LINK_INFO
        | HAS_NAME
        | HAS_RELATIVE_PATH
        | HAS_WORKING_DIR
        | HAS_ARGUMENTS
        | HAS_ICON_LOCATION
        | IS_UNICODE;

    let mut out = Vec::new();
    // --- ShellLinkHeader（76 字节） ---
    out.extend_from_slice(&0x0000_004Cu32.to_le_bytes());
    // LinkCLSID {00021401-0000-0000-C000-000000000046}
    out.extend_from_slice(&[
        0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x46,
    ]);
    out.extend_from_slice(&flags.to_le_bytes());
    out.extend_from_slice(&0x0000_0020u32.to_le_bytes()); // FILE_ATTRIBUTE_ARCHIVE
    out.extend_from_slice(&[0u8; 8]); // CreationTime
    out.extend_from_slice(&[0u8; 8]); // AccessTime
    out.extend_from_slice(&[0u8; 8]); // WriteTime
    out.extend_from_slice(&0u32.to_le_bytes()); // FileSize
    out.extend_from_slice(&0i32.to_le_bytes()); // IconIndex
    out.extend_from_slice(&1u32.to_le_bytes()); // ShowCommand = SW_SHOWNORMAL
    out.extend_from_slice(&0u16.to_le_bytes()); // HotKey
    out.extend_from_slice(&0u16.to_le_bytes()); // Reserved1
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved2
    out.extend_from_slice(&0u32.to_le_bytes()); // Reserved3
    assert_eq!(out.len(), 76, "ShellLinkHeader 必须是 76 字节");

    // --- LinkInfo ---
    const HEADER_SIZE: u32 = 0x1C;
    let label: &[u8] = b"C:";
    let volume_id_size = 16 + label.len() as u32 + 1;
    let volume_id_offset = HEADER_SIZE;
    let local_base_path_offset = volume_id_offset + volume_id_size;
    let mut local_base: Vec<u8> = target.bytes().collect();
    local_base.push(0);
    let common_path_suffix_offset = local_base_path_offset + local_base.len() as u32;
    let common_path_suffix: [u8; 1] = [0]; // 空后缀
    let link_info_size = common_path_suffix_offset + common_path_suffix.len() as u32;

    let mut link_info = Vec::new();
    link_info.extend_from_slice(&link_info_size.to_le_bytes());
    link_info.extend_from_slice(&HEADER_SIZE.to_le_bytes());
    link_info.extend_from_slice(&1u32.to_le_bytes()); // VolumeIDAndLocalBasePath
    link_info.extend_from_slice(&volume_id_offset.to_le_bytes());
    link_info.extend_from_slice(&local_base_path_offset.to_le_bytes());
    link_info.extend_from_slice(&0u32.to_le_bytes()); // CommonNetworkRelativeLinkOffset
    link_info.extend_from_slice(&common_path_suffix_offset.to_le_bytes());
    assert_eq!(link_info.len() as u32, HEADER_SIZE);
    // VolumeID
    link_info.extend_from_slice(&volume_id_size.to_le_bytes());
    link_info.extend_from_slice(&3u32.to_le_bytes()); // DRIVE_FIXED
    link_info.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    link_info.extend_from_slice(&16u32.to_le_bytes()); // VolumeLabelOffset
    link_info.extend_from_slice(label);
    link_info.push(0);
    assert_eq!(link_info.len() as u32, local_base_path_offset);
    link_info.extend_from_slice(&local_base);
    link_info.extend_from_slice(&common_path_suffix);
    assert_eq!(link_info.len() as u32, link_info_size, "LinkInfoSize 必须与实际字节数一致");
    out.extend_from_slice(&link_info);

    // --- StringData（Unicode：u16 字符数 + UTF-16LE） ---
    for text in [display_name, relative, working_dir, arguments, icon_location] {
        let units: Vec<u16> = text.encode_utf16().collect();
        out.extend_from_slice(&(units.len() as u16).to_le_bytes());
        for unit in units {
            out.extend_from_slice(&unit.to_le_bytes());
        }
    }

    out
}

const TARGET: &str = r"C:\Program Files\Example App\example.exe";
const ARGUMENTS: &str = r#"--profile "C:\Users\测试\profile dir" --flag"#;
const WORKING_DIR: &str = r"C:\Program Files\Example App";
const ICON_LOCATION: &str = r"C:\Program Files\Example App\example.exe,0";
const DISPLAY_NAME: &str = "示例 应用";

/// `lnk` 的真实解析结果到本 crate 字段映射的映射必须正确。
#[test]
fn fixture_lnk_fields_map_to_target_arguments_and_icon() {
    let dir = TempDir::new("fields");
    let lnk_path = dir.write("Sample.lnk", &build_lnk(
        TARGET,
        r".\example.exe",
        ARGUMENTS,
        WORKING_DIR,
        ICON_LOCATION,
        DISPLAY_NAME,
    ));

    let fields = open_fields(&lnk_path, code_page_encoding(Some(1252))).expect("真实 .lnk 必须可解析");

    assert_eq!(
        fields.target.as_deref(),
        Some(TARGET),
        "LinkInfo 的 LocalBasePath 必须成为绝对目标"
    );
    assert_eq!(fields.relative_target.as_deref(), Some(r".\example.exe"));
    assert_eq!(
        fields.arguments.as_deref(),
        Some(ARGUMENTS),
        "非 ASCII 参数必须按 UTF-16LE 正确解码"
    );
    assert_eq!(fields.working_dir.as_deref(), Some(WORKING_DIR));
    assert_eq!(fields.icon_location.as_deref(), Some(ICON_LOCATION));
    assert_eq!(
        fields.description.as_deref(),
        Some(DISPLAY_NAME),
        "非 ASCII 名称必须按 UTF-16LE 正确解码"
    );

    // 映射结果直接驱动后续判定：绝对存在路径不需要纠正 pass，并且是可执行文件。
    assert!(!needs_shell_correction(&fields));
    assert_eq!(classify_target(fields.target.as_deref(), Some(r"C:\Windows")), TargetKind::Executable);
    let resolved = resolve_relative(
        fields.relative_target.as_deref().unwrap(),
        lnk_path.parent().unwrap(),
    )
    .expect("相对目标必须可解析");
    let expected = format!(
        "{}\\example.exe",
        lnk_path
            .parent()
            .unwrap()
            .to_string_lossy()
            .replace('/', "\\")
    );
    assert_eq!(
        resolved.to_string_lossy().replace('/', "\\"),
        expected,
        "相对目标解析到快捷方式所在目录"
    );
    assert_eq!(
        choose_target(fields.target.as_deref(), None, |path| path == TARGET).as_deref(),
        Some(TARGET)
    );
}

/// `lnk` 不做环境变量展开，含 `%VAR%` 的目标必须被判为「需要 `IShellLinkW` 纠正」，
/// 并能在拿到变量值后被展开。
#[test]
fn fixture_lnk_with_environment_variable_target_requests_correction() {
    let dir = TempDir::new("env");
    let lnk_path = dir.write("Env.lnk", &build_lnk(
        r"%ProgramFiles%\Example App\example.exe",
        r".\example.exe",
        "",
        r"%ProgramFiles%\Example App",
        r"%ProgramFiles%\Example App\example.exe,0",
        "Example App",
    ));

    let fields = open_fields(&lnk_path, code_page_encoding(Some(1252))).expect("必须可解析");
    assert_eq!(
        fields.target.as_deref(),
        Some(r"%ProgramFiles%\Example App\example.exe"),
        "lnk 不会展开环境变量，这正是必须再走 IShellLinkW 的原因"
    );
    assert!(needs_shell_correction(&fields));

    let lookup = |name: &str| {
        name.eq_ignore_ascii_case("ProgramFiles")
            .then(|| r"C:\Program Files".to_string())
    };
    assert_eq!(
        expand_env(fields.target.as_deref().unwrap(), lookup),
        TARGET
    );
}

/// 广告式 MSI 快捷方式（目标写成产品码）必须被判为不可重新启动。
#[test]
fn fixture_advertised_msi_shortcut_is_not_launchable() {
    let dir = TempDir::new("msi");
    let lnk_path = dir.write("Office.lnk", &build_lnk(
        "{90160000-008C-0000-1000-0000000FF1CE}",
        r".\WINWORD.EXE",
        "",
        r"C:\Program Files\Microsoft Office",
        "",
        "Microsoft Word",
    ));

    let fields = open_fields(&lnk_path, code_page_encoding(Some(1252))).expect("必须可解析");
    assert_eq!(
        classify_target(fields.target.as_deref(), Some(r"C:\Windows")),
        TargetKind::AdvertisedInstaller
    );
}

/// 开始菜单目录 → 快捷方式清单 → `.lnk` 解析 → 稳定标识：整条纯逻辑链路。
#[test]
fn start_menu_scan_parses_shortcuts_and_produces_stable_ids() {
    let dir = TempDir::new("scan");
    let root = dir.path().join("Start Menu");
    std::fs::create_dir_all(&root).expect("创建开始菜单根");

    let nested = root.join("Microsoft Office");
    std::fs::create_dir_all(&nested).expect("创建分组目录");
    std::fs::write(
        nested.join("Word.lnk"),
        build_lnk(
            r"C:\Program Files\Microsoft Office\WINWORD.EXE",
            r".\WINWORD.EXE",
            "",
            r"C:\Program Files\Microsoft Office",
            r"C:\Program Files\Microsoft Office\WINWORD.EXE,0",
            "Microsoft Word",
        ),
    )
    .expect("写入 Word.lnk");
    std::fs::write(
        root.join("Example.lnk"),
        build_lnk(TARGET, r".\example.exe", ARGUMENTS, WORKING_DIR, "", DISPLAY_NAME),
    )
    .expect("写入 Example.lnk");
    // 必须被跳过：`.url` 与 Startup 目录。
    std::fs::write(root.join("Readme.url"), b"[InternetShortcut]\r\nURL=https://example.com\r\n")
        .expect("写入 .url");
    let startup = root.join("Programs").join("Startup");
    std::fs::create_dir_all(&startup).expect("创建 Startup");
    std::fs::write(
        startup.join("AutoStart.lnk"),
        build_lnk(TARGET, r".\example.exe", "", "", "", "AutoStart"),
    )
    .expect("写入 Startup/.lnk");

    let shortcuts = discover_shortcuts(&root);
    assert_eq!(
        shortcuts
            .iter()
            .map(|s| s.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["Example", "Word"],
        "只保留非 Startup 的 .lnk"
    );
    assert_eq!(shortcuts[0].group, None);
    assert_eq!(shortcuts[1].group.as_deref(), Some("Microsoft Office"));

    let mut entries = Vec::new();
    for shortcut in &shortcuts {
        let fields = open_fields(&shortcut.path, code_page_encoding(Some(1252)))
            .unwrap_or_else(|error| panic!("{} 必须可解析：{error}", shortcut.path.display()));
        let target = fields.target.clone().expect("夹具必须给出绝对目标");
        entries.push((
            u32::from(shortcut.group.is_some()),
            exe_id(&target, fields.arguments.as_deref()),
        ));
    }
    assert_eq!(
        entries[0].1,
        exe_id(TARGET, Some(ARGUMENTS)),
        "稳定标识必须是小写化目标路径 + 参数"
    );
    assert_ne!(entries[0].1, entries[1].1);
}

/// 同一个目标在用户目录与分组目录各有一个快捷方式时只保留一条。
#[test]
fn duplicate_shortcuts_collapse_to_one_entry() {
    let dir = TempDir::new("dedupe");
    let root = dir.path().join("Start Menu");
    let nested = root.join("Vendor");
    std::fs::create_dir_all(&nested).expect("创建分组目录");
    let bytes = build_lnk(TARGET, r".\example.exe", "", "", "", DISPLAY_NAME);
    let root_lnk = dir.write("Start Menu/Example.lnk", &bytes);
    let nested_lnk = dir.write("Start Menu/Vendor/Example.lnk", &bytes);

    let shortcuts = discover_shortcuts(&root);
    assert_eq!(shortcuts.len(), 2, "扫描阶段不去重，去重按稳定标识进行");

    let mut ranked = Vec::new();
    for shortcut in [&root_lnk, &nested_lnk] {
        let fields = open_fields(shortcut, code_page_encoding(Some(1252))).expect("可解析");
        let target = fields.target.clone().expect("夹具必须给出绝对目标");
        let id = exe_id(&target, fields.arguments.as_deref());
        let rank = u32::from(shortcut.parent() != Some(root.as_path()));
        ranked.push((
            rank,
            AppEntry {
                id,
                name: "Example App".to_string(),
                comment: None,
                icon: Some(IconRef::unresolved("Example App")),
                exec: vec![shortcut.to_string_lossy().into_owned()],
                desktop_file: None,
                working_dir: fields.working_dir.map(PathBuf::from),
                wm_class: None,
                terminal: false,
                keywords: Vec::new(),
                source: AppSource::StartMenu,
            },
        ));
    }

    let deduped = dedupe_entries(ranked);
    assert_eq!(deduped.len(), 1, "同一目标必须只产生一个条目");
    assert_eq!(
        deduped[0].exec,
        vec![root_lnk.to_string_lossy().into_owned()],
        "保留 rank 更小的那个（开始菜单根目录）"
    );
}

/// 注册表规则表 + 图标来源 + UWP 枚举的组合验证（与系统调用层使用同一批函数）。
#[test]
fn registry_and_uwp_pure_logic_compose() {
    // 保留：真实 MSI 安装（WindowsInstaller=1 且有 DisplayIcon）。
    let mut entry = UninstallValues {
        key_name: "{11111111-2222-3333-4444-555555555555}".to_string(),
        display_name: Some("Example App".to_string()),
        display_icon: Some(r"C:\Program Files\Example App\example.exe,0".to_string()),
        windows_installer: Some(1),
        ..Default::default()
    };
    assert_eq!(classify(&entry), Verdict::Keep);
    assert_eq!(launch_target(&entry), Some(PathBuf::from(TARGET)));

    // 丢弃：SystemComponent=1。
    entry.system_component = Some(1);
    assert!(matches!(classify(&entry), Verdict::Drop(_)));

    // Get-StartApps 的真实输出形状 → AUMID → 启动计划。
    let apps = parse_get_start_apps_json(
        r#"[{"Name":"计算器","AppID":"Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"}]"#,
    );
    assert_eq!(apps.len(), 1);
    assert!(is_launchable_aumid(&apps[0].aumid));
    assert_eq!(
        aumid_id(&apps[0].aumid),
        "win:aumid:microsoft.windowscalculator_8wekyb3d8bbwe!app"
    );
    let program = format!("aumid:{}", apps[0].aumid);
    assert_eq!(aumid_from_program(&program).as_deref().map(|a| a == apps[0].aumid), Some(true));
    let planned = plan(&program, &[], None, false).expect("计划可生成");
    assert_eq!(
        planned,
        LaunchPlan::AppsFolder {
            aumid: apps[0].aumid.clone()
        }
    );
}

/// 开始菜单根由环境变量推导，并且 `%LOCALAPPDATA%` 的联接不会造成重复扫描。
#[test]
fn start_menu_roots_come_from_environment_and_dedupe() {
    let roots = roots_from_env(
        Some(r"C:\Users\me\AppData\Roaming"),
        Some(r"C:\ProgramData"),
        Some(r"c:/users/me/appdata/roaming"),
    );
    assert_eq!(roots.len(), 2, "LOCALAPPDATA 与 APPDATA 指向同一用户目录：{roots:?}");
    assert!(roots[0]
        .to_string_lossy()
        .replace('/', "\\")
        .ends_with(r"AppData\Roaming\Microsoft\Windows\Start Menu"));
    assert!(roots[1]
        .to_string_lossy()
        .replace('/', "\\")
        .ends_with(r"ProgramData\Microsoft\Windows\Start Menu"));
}

/// 预乘 BGRA → 直通 RGBA → PNG 的完整像素链路（`GetImage` 返回的正是这种数据）。
#[test]
fn premultiplied_pixels_become_a_valid_png() {
    let mut pixels = vec![
        64u8, 32, 16, 128, // 半透明像素（预乘 BGRA）
        0, 0, 255, 255, // 不透明蓝色
        7, 7, 7, 0, // 全透明
    ];
    assert!(!is_blank(&pixels));
    unpremultiply_bgra_in_place(&mut pixels);
    assert_eq!(
        pixels,
        vec![32, 64, 128, 128, 255, 0, 0, 255, 0, 0, 0, 0]
    );

    let png = encode_png(3, 1, &pixels).expect("编码成功");
    let decoded = image::load_from_memory(&png).expect("可解码").to_rgba8();
    assert_eq!(decoded.into_raw(), pixels);

    // 全透明位图必须被识别为空白（无效图标），并且缓存键随来源与尺寸变化。
    assert!(is_blank(&[0u8; 16]));
    assert_ne!(icon_cache_key(TARGET, 0, 256), icon_cache_key(TARGET, 0, 128));
}
