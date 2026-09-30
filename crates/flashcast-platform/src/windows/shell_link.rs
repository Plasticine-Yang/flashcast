//! `.lnk` 解析结果的字段映射与目标判定（纯逻辑）。
//!
//! 两层解析策略取自研究笔记 §1.5 / §1.7：
//!
//! 1. `lnk`（纯 Rust）负责快速、可离线测试的解析；
//! 2. 仅当 `lnk` 的结果不可用时（缺少 LinkInfo、含未展开的环境变量、相对路径、
//!    目标已不在原处），才走 `IShellLinkW` 的纠正 pass —— 只有它会展开环境变量，
//!    也只有它能用 Windows 自己的链接跟踪找到被移动的目标。
//!
//! 本文件只做**判定**，不做系统调用，因此可以在 Linux 上用真实 `.lnk` 夹具验证：
//! `fields_from_lnk` 的输入就是 `lnk::ShellLink` 的真实解析产物。

use std::path::{Path, PathBuf};

use lnk::ShellLink;

/// 从 `.lnk` 中取出的、与平台无关的字段。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShortcutFields {
    /// `LinkInfo` 中的绝对目标路径（未展开环境变量，也不保证仍然存在）。
    pub target: Option<String>,
    /// `StringData` 中的相对目标（`RELATIVE_PATH`），相对于 `.lnk` 所在目录。
    pub relative_target: Option<String>,
    /// `StringData` 中的命令行参数（原始字符串，由系统按 `CommandLineToArgvW` 规则分词）。
    pub arguments: Option<String>,
    pub working_dir: Option<String>,
    /// 图标位置，形如 `C:\P\a.exe`、`C:\P\a.exe,0` 或 `C:\P\icon.ico`。
    pub icon_location: Option<String>,
    /// `NAME_STRING`，即快捷方式的描述文字。
    pub description: Option<String>,
}

fn clean(value: &Option<String>) -> Option<String> {
    value
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// 把 `lnk::ShellLink` 的真实解析产物映射为 [`ShortcutFields`]。
pub fn fields_from_lnk(link: &ShellLink) -> ShortcutFields {
    let strings = link.string_data();
    ShortcutFields {
        target: link.link_target(),
        relative_target: clean(strings.relative_path()),
        arguments: clean(strings.command_line_arguments()),
        working_dir: clean(strings.working_dir()),
        icon_location: clean(strings.icon_location()),
        description: clean(strings.name_string()),
    }
}

/// 用给定编码解析一个 `.lnk`。
///
/// `encoding` 只影响非 Unicode（`IS_UNICODE` 未置位）的字符串；Windows 上应传入
/// 系统 ACP（`HKLM\SYSTEM\CurrentControlSet\Control\Nls\CodePage\ACP`），
/// 开发机与测试上退化为 1252。
pub fn open_fields(
    path: &Path,
    encoding: lnk::Encoding,
) -> Result<ShortcutFields, String> {
    let link = ShellLink::open(path, encoding).map_err(|error| error.to_string())?;
    Ok(fields_from_lnk(&link))
}

/// 展开 `%VAR%` 形式的环境变量。
///
/// 未定义或格式不完整的 `%…` 原样保留，这样 [`needs_shell_correction`] 仍能识别出
/// 「这里有没展开的变量」，而不是悄悄产出一个不存在的路径。
pub fn expand_env(raw: &str, lookup: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match lookup(name) {
                    Some(value) => out.push_str(&value),
                    None => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                // 没有配对的 `%`：原样输出剩余内容。
                out.push_str(&rest[start..]);
                return out;
            }
        }
    }
    out.push_str(rest);
    out
}

/// 是否需要用 `IShellLinkW` 做纠正 pass。
///
/// 三种情况：`lnk` 没解析出绝对目标（缺 LinkInfo）、目标含未展开的环境变量、
/// 目标是相对路径。目标「不存在」由调用方在磁盘上判断后一并纳入。
pub fn needs_shell_correction(fields: &ShortcutFields) -> bool {
    match fields.target.as_deref() {
        None => true,
        Some(target) => target.contains('%') || !is_absolute_windows_path(target),
    }
}

/// 是否为 Windows 绝对路径（`C:\…`、`\\server\share`、`\\?\C:\…`）。
pub fn is_absolute_windows_path(path: &str) -> bool {
    let bytes: Vec<char> = path.chars().collect();
    if bytes.len() >= 2 && bytes[1] == ':' && bytes[0].is_ascii_alphabetic() {
        return true;
    }
    path.starts_with("\\\\") || path.starts_with("//")
}

/// 把 `StringData` 的相对目标按 `.lnk` 所在目录解析为绝对路径。
///
/// 只做词法拼接与 `..` 折叠，不访问磁盘（`.\a.exe`、`..\bin\a.exe`、
/// `C:\abs\a.exe`、`\\server\share\a.exe` 都支持）。
///
/// 分量用 `\` 手工拼接而不是 `PathBuf::push`：这样结果与宿主的路径分隔符无关，
/// Windows 路径在 Linux 的夹具测试里也能得到与 Windows 上一致的字符串。
pub fn resolve_relative(relative: &str, lnk_dir: &Path) -> Option<PathBuf> {
    let trimmed = relative.trim();
    if trimmed.is_empty() {
        return None;
    }
    // 带盘符的绝对路径与 UNC 路径无需拼接。
    if is_absolute_windows_path(trimmed) {
        return Some(PathBuf::from(trimmed));
    }

    let dir = lnk_dir.to_string_lossy();
    let rooted = dir.starts_with(['\\', '/']);
    let mut parts: Vec<String> = dir
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    for part in trimmed.split(['\\', '/']) {
        match part {
            "" | "." => {}
            ".." => {
                // 不允许越过根：`C:` 或 UNC 前缀必须保留。
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            other => parts.push(other.to_string()),
        }
    }
    if parts.is_empty() {
        return None;
    }
    let joined = if rooted {
        format!("\\{}", parts.join("\\"))
    } else {
        parts.join("\\")
    };
    Some(PathBuf::from(joined))
}

/// 目标的可启动性分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    /// 可以直接 `CreateProcessW` 的可执行文件。
    Executable,
    /// 文档、文件夹或其它可由 Shell 打开的目标。
    DocumentOrFolder,
    /// MSI 广告式快捷方式：目标是产品码或 `%WINDIR%\Installer\…`，无法直接重启该软件。
    AdvertisedInstaller,
    /// `msiexec.exe` / `rundll32.exe` 之类无法有意义地重新启动的代理程序。
    NonLaunchableShim,
    /// 没有目标。
    Missing,
}

/// 无法有意义地重新启动的代理程序（§1.1）。
const NON_LAUNCHABLE_SHIMS: [&str; 4] = ["msiexec", "msiexec.exe", "rundll32", "rundll32.exe"];

/// 判定目标属于哪一类。`windir` 为 `%WINDIR%`，用于识别安装器缓存目录。
pub fn classify_target(target: Option<&str>, windir: Option<&str>) -> TargetKind {
    let Some(target) = target.map(str::trim).filter(|t| !t.is_empty()) else {
        return TargetKind::Missing;
    };
    // MSI 广告式快捷方式：目标写成产品码 `{GUID}`。
    if target.starts_with('{') && target.contains('}') {
        return TargetKind::AdvertisedInstaller;
    }
    if is_installer_cache_path(target, windir) {
        return TargetKind::AdvertisedInstaller;
    }
    let file_name = target
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(target)
        .to_ascii_lowercase();
    if NON_LAUNCHABLE_SHIMS.contains(&file_name.as_str()) {
        return TargetKind::NonLaunchableShim;
    }
    if let Some(extension) = file_name.rsplit_once('.').map(|(_, ext)| ext) {
        if matches!(extension, "exe" | "com" | "bat" | "cmd") {
            return TargetKind::Executable;
        }
    }
    TargetKind::DocumentOrFolder
}

/// 目标是否位于 `%WINDIR%\Installer\`（MSI 缓存）或 `rundll32` 代理之下。
pub fn is_installer_cache_path(target: &str, windir: Option<&str>) -> bool {
    let normalized = target.replace('/', "\\").to_lowercase();
    if normalized.contains("\\installer\\") && normalized.ends_with(".msi") {
        return true;
    }
    match windir {
        Some(windir) if !windir.trim().is_empty() => {
            let prefix = format!("{}\\installer\\", windir.trim_end_matches(['\\', '/']).to_lowercase());
            normalized.starts_with(&prefix)
        }
        _ => false,
    }
}

/// 把系统 ACP（`HKLM\SYSTEM\CurrentControlSet\Control\Nls\CodePage\ACP`）映射为
/// `lnk` 需要的编码。
///
/// 非 Unicode 的 `.lnk` 用系统默认代码页编码字符串，猜错会得到乱码名称。
/// `encoding_rs`（经 `lnk` 再导出）只提供下列代码页；其余一律退回 1252 —— 那是最
/// 常见的西欧/美式默认值，且对纯 ASCII 名称总是正确。
pub fn code_page_encoding(acp: Option<u32>) -> lnk::Encoding {
    use lnk::encoding;
    match acp {
        Some(874) => encoding::WINDOWS_874,
        Some(1250) => encoding::WINDOWS_1250,
        Some(1251) => encoding::WINDOWS_1251,
        Some(1252) => encoding::WINDOWS_1252,
        Some(1253) => encoding::WINDOWS_1253,
        Some(1254) => encoding::WINDOWS_1254,
        Some(1255) => encoding::WINDOWS_1255,
        Some(1256) => encoding::WINDOWS_1256,
        Some(1257) => encoding::WINDOWS_1257,
        Some(1258) => encoding::WINDOWS_1258,
        _ => encoding::WINDOWS_1252,
    }
}

/// 在「快速解析结果」与「`IShellLinkW` 纠正结果」之间挑选最终目标。
///
/// 优先选**磁盘上真实存在**的那一个；都不存在时依次退回纠正结果、解析结果，
/// 让调用方仍能给出「目标不存在」这类可展示的错误，而不是静默丢弃条目。
pub fn choose_target(
    parsed: Option<&str>,
    corrected: Option<&str>,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    let candidates = [corrected, parsed];
    for candidate in candidates.into_iter().flatten() {
        let trimmed = candidate.trim();
        if !trimmed.is_empty() && exists(trimmed) {
            return Some(trimmed.to_string());
        }
    }
    candidates
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_known_environment_variables_only() {
        let lookup = |name: &str| match name.to_ascii_lowercase().as_str() {
            "programfiles" => Some(r"C:\Program Files".to_string()),
            "windir" => Some(r"C:\Windows".to_string()),
            _ => None,
        };
        assert_eq!(
            expand_env(r"%ProgramFiles%\Foo\foo.exe", lookup),
            r"C:\Program Files\Foo\foo.exe"
        );
        assert_eq!(expand_env(r"a%WINDIR%b", lookup), r"aC:\Windowsb");
        assert_eq!(
            expand_env(r"%UNKNOWN%\foo.exe", lookup),
            r"%UNKNOWN%\foo.exe",
            "未知变量必须原样保留"
        );
        assert_eq!(expand_env("no-percent", lookup), "no-percent");
        assert_eq!(expand_env("100% done", lookup), "100% done");
        assert_eq!(expand_env(r"%ProgramFiles%\", lookup), r"C:\Program Files\");
    }

    #[test]
    fn correction_is_needed_for_missing_expandable_or_relative_targets() {
        let cases = [
            (ShortcutFields::default(), true, "缺少目标"),
            (
                ShortcutFields {
                    target: Some(r"%ProgramFiles%\Foo\foo.exe".to_string()),
                    ..Default::default()
                },
                true,
                "含未展开变量",
            ),
            (
                ShortcutFields {
                    target: Some(r".\foo.exe".to_string()),
                    ..Default::default()
                },
                true,
                "相对路径",
            ),
            (
                ShortcutFields {
                    target: Some(r"C:\Tools\foo.exe".to_string()),
                    ..Default::default()
                },
                false,
                "绝对路径",
            ),
            (
                ShortcutFields {
                    target: Some(r"\\server\share\foo.exe".to_string()),
                    ..Default::default()
                },
                false,
                "UNC 路径",
            ),
        ];
        for (fields, expected, why) in cases {
            assert_eq!(needs_shell_correction(&fields), expected, "{why}");
        }
    }

    #[test]
    fn relative_targets_resolve_against_the_shortcut_directory() {
        let dir = Path::new(r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Vendor");
        assert_eq!(
            resolve_relative(r".\app.exe", dir),
            Some(PathBuf::from(
                r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\Vendor\app.exe"
            ))
        );
        assert_eq!(
            resolve_relative(r"..\bin\app.exe", dir),
            Some(PathBuf::from(
                r"C:\ProgramData\Microsoft\Windows\Start Menu\Programs\bin\app.exe"
            ))
        );
        assert_eq!(
            resolve_relative(r"C:\Other\app.exe", dir),
            Some(PathBuf::from(r"C:\Other\app.exe"))
        );
        assert_eq!(resolve_relative("   ", dir), None);
    }

    #[test]
    fn targets_are_classified_by_launchability() {
        let windir = Some(r"C:\Windows");
        assert_eq!(
            classify_target(Some(r"C:\Tools\app.exe"), windir),
            TargetKind::Executable
        );
        assert_eq!(
            classify_target(Some(r"C:\Tools\APP.EXE"), windir),
            TargetKind::Executable
        );
        assert_eq!(
            classify_target(Some(r"C:\Tools\readme.pdf"), windir),
            TargetKind::DocumentOrFolder
        );
        assert_eq!(
            classify_target(Some("{90160000-008C-0000-1000-0000000FF1CE}"), windir),
            TargetKind::AdvertisedInstaller
        );
        assert_eq!(
            classify_target(
                Some(r"C:\Windows\Installer\{90160000-008C-0000-1000-0000000FF1CE}\icon.exe"),
                windir
            ),
            TargetKind::AdvertisedInstaller,
            "安装器缓存目录下的目标不可重新启动"
        );
        assert_eq!(
            classify_target(Some(r"C:\Windows\System32\msiexec.exe"), windir),
            TargetKind::NonLaunchableShim
        );
        assert_eq!(
            classify_target(Some(r"C:\Windows\System32\rundll32.exe"), windir),
            TargetKind::NonLaunchableShim,
            "代理程序不能作为可重新启动的目标"
        );
        assert_eq!(classify_target(None, windir), TargetKind::Missing);
        assert_eq!(classify_target(Some("  "), windir), TargetKind::Missing);
    }

    #[test]
    fn existing_target_wins_over_stale_one() {
        let exists = |path: &str| path == r"C:\New\app.exe";
        assert_eq!(
            choose_target(Some(r"C:\Old\app.exe"), Some(r"C:\New\app.exe"), exists).as_deref(),
            Some(r"C:\New\app.exe")
        );
        // 都不存在时优先给出纠正结果，让上层能报告具体缺失路径。
        assert_eq!(
            choose_target(Some(r"C:\Old\app.exe"), Some(r"C:\New\app.exe"), |_| false).as_deref(),
            Some(r"C:\New\app.exe")
        );
        assert_eq!(choose_target(None, None, |_| false), None);
    }
}
