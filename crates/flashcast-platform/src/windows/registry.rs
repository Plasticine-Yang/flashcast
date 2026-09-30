//! 注册表 Uninstall 键的筛选与启动目标推导（纯逻辑）。
//!
//! 规则表来自研究笔记 §1.2，逐条落地在这里而不是散落在系统调用层，这样整张表
//! 都能在 Linux 上用真实值组合验证。三处最容易做错的地方：
//!
//! 1. `WindowsInstaller == 1` **本身不是**丢弃理由 —— 绝大多数 MSI 安装都会置位，
//!    只有同时缺少 `DisplayIcon` 与 `InstallLocation` 时才是纯安装器记录；
//! 2. `SystemComponent == 1` 与「补丁/子项」（`ParentKeyName`）必须丢弃；
//! 3. `NoRemove == 1` 是**保留**：那是真实产品，只是不允许卸载。
//!
//! 启动目标只从 `DisplayIcon`（必须指向 `.exe`）与 `InstallLocation` 中推导；
//! `UninstallString` / `QuietUninstallString` 只用于卸载，尤其是
//! `MsiExec.exe /X{GUID}`，绝不能作为启动动作提供给用户。

use std::path::{Path, PathBuf};

/// 卸载键中与本筛选相关的取值。全部为 `Option`，缺失与空值等价。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UninstallValues {
    /// 子键名（GUID 或厂商字符串）。
    pub key_name: String,
    pub display_name: Option<String>,
    pub display_version: Option<String>,
    pub publisher: Option<String>,
    /// 正在读取的视图（`hklm` / `hklm32` / `hkcu`），仅用于稳定标识与诊断。
    pub hive: String,
    pub system_component: Option<u32>,
    pub parent_key_name: Option<String>,
    pub parent_display_name: Option<String>,
    pub windows_installer: Option<u32>,
    pub release_type: Option<String>,
    pub uninstall_string: Option<String>,
    pub display_icon: Option<String>,
    pub install_location: Option<String>,
    pub no_remove: Option<u32>,
}

/// 丢弃原因。每一条都对应规则表中的一行，便于诊断输出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    NoDisplayName,
    SystemComponent,
    ChildOfAnotherEntry,
    InstallerWithoutTarget,
    UpdatePackage,
    WindowsUpdate,
    NothingToLaunch,
    UnresolvedGuidKey,
}

impl DropReason {
    pub fn reason_zh(self) -> &'static str {
        match self {
            DropReason::NoDisplayName => "没有 DisplayName，无法展示",
            DropReason::SystemComponent => "SystemComponent=1：运行库或驱动组件，用户无法启动",
            DropReason::ChildOfAnotherEntry => "带 ParentKeyName/ParentDisplayName：是补丁或子项",
            DropReason::InstallerWithoutTarget => {
                "WindowsInstaller=1 且没有 DisplayIcon / InstallLocation：纯 MSI 机制记录"
            }
            DropReason::UpdatePackage => "ReleaseType 表示更新而非产品",
            DropReason::WindowsUpdate => "DisplayName 表示 Windows 更新（KB… / Update for …）",
            DropReason::NothingToLaunch => {
                "没有 UninstallString、DisplayIcon 或 InstallLocation：既不能启动也不能卸载"
            }
            DropReason::UnresolvedGuidKey => "子键 GUID 与 DisplayName 相同：条目未解析",
        }
    }
}

/// 筛选结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Keep,
    Drop(DropReason),
}

fn non_empty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

/// 产品码 GUID：`{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`。
pub fn is_product_code_guid(value: &str) -> bool {
    let trimmed = value.trim();
    if !trimmed.starts_with('{') || !trimmed.ends_with('}') || trimmed.len() != 38 {
        return false;
    }
    trimmed[1..37].chars().enumerate().all(|(index, c)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            c == '-'
        } else {
            c.is_ascii_hexdigit()
        }
    })
}

/// DisplayName 是否表示 Windows 更新（`KB1234567` 或 `Update for …`）。
pub fn looks_like_windows_update(display_name: &str) -> bool {
    let trimmed = display_name.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.contains("update for") {
        return true;
    }
    let Some(rest) = lower.strip_prefix("kb") else {
        return false;
    };
    let digits = rest
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>();
    !digits.is_empty()
}

/// 应用规则表，给出保留/丢弃结论。
pub fn classify(values: &UninstallValues) -> Verdict {
    let Some(display_name) = non_empty(&values.display_name) else {
        return Verdict::Drop(DropReason::NoDisplayName);
    };
    if values.system_component == Some(1) {
        return Verdict::Drop(DropReason::SystemComponent);
    }
    if non_empty(&values.parent_key_name).is_some() || non_empty(&values.parent_display_name).is_some()
    {
        return Verdict::Drop(DropReason::ChildOfAnotherEntry);
    }
    let has_icon = non_empty(&values.display_icon).is_some();
    let has_install_location = non_empty(&values.install_location).is_some();
    if values.windows_installer == Some(1) && !has_icon && !has_install_location {
        return Verdict::Drop(DropReason::InstallerWithoutTarget);
    }
    if let Some(release_type) = non_empty(&values.release_type) {
        // `Security Update` 也可能是真实产品的补丁集合，但规则表仍要求保留它。
        if !release_type.eq_ignore_ascii_case("Security Update") {
            return Verdict::Drop(DropReason::UpdatePackage);
        }
    }
    if looks_like_windows_update(display_name) {
        return Verdict::Drop(DropReason::WindowsUpdate);
    }
    let has_uninstall = non_empty(&values.uninstall_string).is_some();
    if !has_uninstall && !has_icon && !has_install_location {
        return Verdict::Drop(DropReason::NothingToLaunch);
    }
    if is_product_code_guid(&values.key_name) && values.key_name.eq_ignore_ascii_case(display_name) {
        return Verdict::Drop(DropReason::UnresolvedGuidKey);
    }
    Verdict::Keep
}

/// `DisplayIcon` 的解析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayIcon {
    /// 去掉引号与 `,<索引>` 之后的路径。
    pub path: Option<String>,
    /// 图标索引（`C:\P\a.exe,-101` 中的 `-101`）。
    pub index: Option<i32>,
}

/// 解析 `DisplayIcon`：它的真实形态包括
/// `C:\P\a.exe`、`C:\P\a.exe,0`、`C:\P\a.exe,-101`、`"C:\P With Space\a.exe",0`、
/// `C:\P\icon.ico`。引号只包住路径，索引在引号之后。
pub fn parse_display_icon(raw: &str) -> DisplayIcon {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DisplayIcon {
            path: None,
            index: None,
        };
    }
    let (path_part, rest) = if let Some(inner) = trimmed.strip_prefix('"') {
        match inner.find('"') {
            Some(end) => (&inner[..end], &inner[end + 1..]),
            // 只有开引号：按未被引号包住处理，避免把整串当路径又留下引号。
            None => (trimmed, ""),
        }
    } else {
        (trimmed, "")
    };

    let (path, index) = if path_part.is_empty() {
        (None, None)
    } else {
        match split_trailing_index(path_part) {
            Some((path, index)) => (Some(path), Some(index)),
            None => (Some(path_part.trim().to_string()), None),
        }
    };
    // 引号之后的 `,<int>` 覆盖路径内解析的结果（二者等价，后者更明确）。
    let index = match rest.trim().strip_prefix(',') {
        Some(value) => value.trim().parse::<i32>().ok().or(index),
        None => index,
    };
    let path = path.filter(|value| !value.is_empty());
    DisplayIcon { path, index }
}

/// 把 `C:\P\a.exe,-101` 拆成路径与索引；`C:\P\a.exe` 原样返回。
fn split_trailing_index(value: &str) -> Option<(String, i32)> {
    let (path, suffix) = value.rsplit_once(',')?;
    let index = suffix.trim().parse::<i32>().ok()?;
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    Some((path.to_string(), index))
}

/// 是否是可以直接启动的可执行文件扩展名。
pub fn is_executable_path(path: &str) -> bool {
    matches!(
        path.rsplit_once('.').map(|(_, ext)| ext.to_ascii_lowercase()),
        Some(ref ext) if matches!(ext.as_str(), "exe" | "com" | "bat" | "cmd")
    )
}

/// `InstallLocation` 扫描时要排除的安装器/卸载器/更新器文件名前缀。
const EXE_STEM_BLOCKLIST: [&str; 15] = [
    "unins", "uninstall", "setup", "install", "update", "updater", "crash", "crashpad", "helper",
    "repair", "modify", "vcredist", "dotnet", "msiexec", "rundll32",
];

/// 名称归一化：只保留字母与数字，用于把 `Example App 2.1` 与 `ExampleApp` 视作相近。
pub fn normalize_name(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

fn stem(path: &Path) -> String {
    path.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 列出目录下所有 `.exe`，排除安装器/卸载器/更新器。
pub fn list_exe_candidates(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let lower = path
                .file_name()
                .map(|n| n.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            lower.ends_with(".exe")
        })
        .filter(|path| {
            let stem = stem(path).to_lowercase();
            !EXE_STEM_BLOCKLIST
                .iter()
                .any(|blocked| stem.starts_with(blocked))
        })
        .collect();
    out.sort_by_key(|path| {
        (
            path.to_string_lossy().len(),
            path.to_string_lossy().to_lowercase(),
        )
    });
    out
}

/// 在候选可执行文件中按名称相似度挑一个；没有可信匹配时返回 `None`。
///
/// 刻意**不**在无法匹配时随便挑一个：`InstallLocation` 里常常还有更新器、
/// 崩溃处理器与卸载器，猜错会让用户点开错误的东西。
pub fn rank_exe_candidates(display_name: &str, candidates: &[PathBuf]) -> Option<PathBuf> {
    let wanted = normalize_name(display_name);
    if wanted.is_empty() {
        return None;
    }
    let mut best: Option<(u8, usize, PathBuf)> = None;
    for candidate in candidates {
        let candidate_stem = normalize_name(&stem(candidate));
        if candidate_stem.is_empty() {
            continue;
        }
        let score = if candidate_stem == wanted {
            0
        } else if wanted.contains(&candidate_stem) || candidate_stem.contains(&wanted) {
            1
        } else {
            continue;
        };
        let length = candidate.to_string_lossy().len();
        let better = match &best {
            None => true,
            Some((best_score, best_length, _)) => (score, length) < (*best_score, *best_length),
        };
        if better {
            best = Some((score, length, candidate.clone()));
        }
    }
    best.map(|(_, _, path)| path)
}

/// 从 `DisplayIcon` 推导可启动目标（只有 `.exe` 才算）。
pub fn launch_target_from_display_icon(raw: &str) -> Option<PathBuf> {
    let icon = parse_display_icon(raw);
    let path = icon.path?;
    is_executable_path(&path).then(|| PathBuf::from(path))
}

/// 由 `DisplayIcon` 推导图标来源：任何扩展名都可用于 `GetImage`。
pub fn icon_source(values: &UninstallValues) -> Option<String> {
    if let Some(raw) = non_empty(&values.display_icon) {
        if let Some(path) = parse_display_icon(raw).path {
            return Some(path);
        }
    }
    non_empty(&values.install_location).and_then(|location| {
        rank_exe_candidates(
            non_empty(&values.display_name).unwrap_or_default(),
            &list_exe_candidates(Path::new(location)),
        )
        .map(|path| path.to_string_lossy().into_owned())
    })
}

/// 由注册表取值推导启动目标：先 `DisplayIcon`，再 `InstallLocation`。
pub fn launch_target(values: &UninstallValues) -> Option<PathBuf> {
    if let Some(raw) = non_empty(&values.display_icon) {
        if let Some(target) = launch_target_from_display_icon(raw) {
            return Some(target);
        }
    }
    let location = non_empty(&values.install_location)?;
    let display_name = non_empty(&values.display_name).unwrap_or_default();
    rank_exe_candidates(display_name, &list_exe_candidates(Path::new(location)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(display_name: &str) -> UninstallValues {
        UninstallValues {
            key_name: "{11111111-2222-3333-4444-555555555555}".to_string(),
            display_name: Some(display_name.to_string()),
            uninstall_string: Some(r"MsiExec.exe /X{11111111-2222-3333-4444-555555555555}".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn windows_installer_alone_is_not_a_reason_to_drop() {
        let mut entry = values("Example App");
        entry.windows_installer = Some(1);
        // 有 DisplayIcon：典型的 MSI 安装，必须保留。
        entry.display_icon = Some(r"C:\Program Files\Example\example.exe,0".to_string());
        assert_eq!(classify(&entry), Verdict::Keep);

        // 有 InstallLocation：同样保留。
        let mut with_location = values("Example App");
        with_location.windows_installer = Some(1);
        with_location.install_location = Some(r"C:\Program Files\Example".to_string());
        assert_eq!(classify(&with_location), Verdict::Keep);

        // 两者都没有：纯安装器机制记录，丢弃。
        let mut bare = values("Example App");
        bare.windows_installer = Some(1);
        bare.uninstall_string = None;
        assert_eq!(
            classify(&bare),
            Verdict::Drop(DropReason::InstallerWithoutTarget)
        );
    }

    #[test]
    fn drop_table_matches_the_rules_one_by_one() {
        let mut no_name = values("Example App");
        no_name.display_name = Some("   ".to_string());
        assert_eq!(classify(&no_name), Verdict::Drop(DropReason::NoDisplayName));

        let mut system_component = values("Example App");
        system_component.system_component = Some(1);
        assert_eq!(
            classify(&system_component),
            Verdict::Drop(DropReason::SystemComponent)
        );

        let mut child = values("Example App");
        child.parent_key_name = Some("Parent".to_string());
        assert_eq!(
            classify(&child),
            Verdict::Drop(DropReason::ChildOfAnotherEntry)
        );

        let mut patch = values("Example App");
        patch.release_type = Some("Hotfix".to_string());
        assert_eq!(classify(&patch), Verdict::Drop(DropReason::UpdatePackage));

        // Security Update 按规则表保留。
        let mut security = values("Example App");
        security.release_type = Some("Security Update".to_string());
        assert_eq!(classify(&security), Verdict::Keep);

        let mut kb = values("KB5034441");
        assert_eq!(classify(&kb), Verdict::Drop(DropReason::WindowsUpdate));
        kb.display_name = Some("Update for Example App (KB5034441)".to_string());
        assert_eq!(classify(&kb), Verdict::Drop(DropReason::WindowsUpdate));

        let mut nothing = values("Example App");
        nothing.uninstall_string = None;
        assert_eq!(
            classify(&nothing),
            Verdict::Drop(DropReason::NothingToLaunch)
        );

        let mut unresolved = values("{11111111-2222-3333-4444-555555555555}");
        unresolved.key_name = "{11111111-2222-3333-4444-555555555555}".to_string();
        assert_eq!(
            classify(&unresolved),
            Verdict::Drop(DropReason::UnresolvedGuidKey)
        );
    }

    #[test]
    fn no_remove_entries_are_kept() {
        let mut entry = values("Protected App");
        entry.no_remove = Some(1);
        assert_eq!(
            classify(&entry),
            Verdict::Keep,
            "NoRemove=1 是真实产品，只是不能卸载"
        );
    }

    #[test]
    fn display_icon_shapes_are_parsed() {
        let cases = [
            (r"C:\P\a.exe", Some(r"C:\P\a.exe"), None),
            (r"C:\P\a.exe,0", Some(r"C:\P\a.exe"), Some(0)),
            (r"C:\P\a.exe,-101", Some(r"C:\P\a.exe"), Some(-101)),
            (
                r#""C:\P With Space\a.exe",0"#,
                Some(r"C:\P With Space\a.exe"),
                Some(0),
            ),
            (r"C:\P\icon.ico", Some(r"C:\P\icon.ico"), None),
            ("", None, None),
        ];
        for (raw, path, index) in cases {
            let parsed = parse_display_icon(raw);
            assert_eq!(parsed.path.as_deref(), path, "路径：{raw}");
            assert_eq!(parsed.index, index, "索引：{raw}");
        }
    }

    #[test]
    fn launch_target_prefers_display_icon_then_install_location() {
        let mut entry = values("Example App");
        entry.display_icon = Some(r"C:\Program Files\Example\example.exe,-101".to_string());
        assert_eq!(
            launch_target(&entry),
            Some(PathBuf::from(r"C:\Program Files\Example\example.exe"))
        );

        // DisplayIcon 只给 .ico：不能作为启动目标，退回到 InstallLocation。
        let mut icon_only = values("Example App");
        icon_only.display_icon = Some(r"C:\Program Files\Example\example.ico".to_string());
        assert_eq!(launch_target(&icon_only), None, "不存在的目录不能猜出目标");
        assert_eq!(
            icon_source(&icon_only).as_deref(),
            Some(r"C:\Program Files\Example\example.ico")
        );
    }

    #[test]
    fn install_location_scan_ranks_by_name_and_skips_installers() {
        let dir = std::env::temp_dir().join(format!("flashcast-uninst-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("创建夹具目录");
        for name in [
            "ExampleApp.exe",
            "unins000.exe",
            "Updater.exe",
            "helper.exe",
            "Other.exe",
        ] {
            std::fs::write(dir.join(name), b"fixture").expect("写入夹具");
        }

        let candidates = list_exe_candidates(&dir);
        let names: Vec<String> = candidates
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(
            !names.iter().any(|n| n.to_lowercase().starts_with("unins")),
            "卸载器必须排除：{names:?}"
        );
        assert!(
            !names.iter().any(|n| n.eq_ignore_ascii_case("updater.exe")),
            "更新器必须排除：{names:?}"
        );

        assert_eq!(
            rank_exe_candidates("Example App 2.1", &candidates),
            Some(dir.join("ExampleApp.exe"))
        );
        assert_eq!(
            rank_exe_candidates("完全不相干的名字", &candidates),
            None,
            "无法可信匹配时不得随便挑一个"
        );

        let mut entry = values("Example App 2.1");
        entry.install_location = Some(dir.to_string_lossy().into_owned());
        assert_eq!(launch_target(&entry), Some(dir.join("ExampleApp.exe")));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn product_code_guid_detection() {
        assert!(is_product_code_guid(
            "{90160000-008C-0000-1000-0000000FF1CE}"
        ));
        assert!(!is_product_code_guid("Google Chrome"));
        assert!(!is_product_code_guid("{not-a-guid}"));
        assert!(!is_product_code_guid(
            "{90160000-008C-0000-1000-0000000FF1C}"
        ));
    }
}
