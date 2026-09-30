//! Windows 软件发现：开始菜单 `.lnk` + 注册表 Uninstall 键 + 打包应用（AUMID）。
//!
//! 三层来源各自的取舍（研究笔记 §1.1–§1.3、§1.8）：
//!
//! - **开始菜单**是主来源：显示名就是 `.lnk` 文件名，图标走 `.lnk` 自己（Shell 会
//!   解析出快捷方式配置的图标）；`lnk` 解析不可用时（缺 LinkInfo、含未展开变量、
//!   相对路径、目标已移动）才调用 `IShellLinkW` 做纠正 pass；
//! - **注册表 Uninstall 键**是补充来源，只补上「没有开始菜单快捷方式」的安装；
//!   过滤规则全部在 [`super::registry`] 的纯函数里，这里只负责读值；
//! - **打包应用**用一次 `Get-StartApps` 批量枚举，启动走 AUMID。
//!
//! 图标在扫描时抽取并写入 `%LOCALAPPDATA%\Flashcast\icons` 缓存（键含来源路径的
//! 修改时间，应用升级后自动失效），因此第二次扫描不再调用 Shell 取图。

use std::path::{Path, PathBuf};

use crate::catalog::{AppCatalog, AppEntry, AppSource, CatalogError, IconRef};
use crate::windows::icons;
use crate::windows::identity::{aumid_id, dedupe_entries, exe_id, registry_id};
use crate::windows::registry as reg;
use crate::windows::shell_link::{self, TargetKind};
use crate::windows::start_menu;
use crate::windows::uwp;

/// 注册表 Uninstall 键的相对路径。
const UNINSTALL_KEY: &str = r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall";

/// 开始菜单快捷方式的排序权重：根目录优先，其次子目录。
const RANK_START_MENU_ROOT: u32 = 0;
const RANK_START_MENU_GROUP: u32 = 1;
const RANK_REGISTRY: u32 = 2;
const RANK_UWP: u32 = 3;

/// 被跳过的条目及原因，用于真实平台检查与诊断输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedEntry {
    pub source: String,
    pub reason: String,
}

/// 一次 Windows 扫描的完整结果。
#[derive(Debug, Clone, Default)]
pub struct WindowsScanOutcome {
    pub entries: Vec<AppEntry>,
    pub start_menu_roots: Vec<PathBuf>,
    pub start_menu_roots_present: usize,
    pub shortcuts_seen: usize,
    pub shell_corrections: usize,
    pub registry_keys_seen: usize,
    pub uwp_seen: usize,
    pub icons_extracted: usize,
    pub icons_failed: usize,
    pub skipped: Vec<SkippedEntry>,
}

impl WindowsScanOutcome {
    fn skip(&mut self, source: impl Into<String>, reason: impl Into<String>) {
        self.skipped.push(SkippedEntry {
            source: source.into(),
            reason: reason.into(),
        });
    }
}

pub struct WindowsAppCatalog {
    roots: Vec<PathBuf>,
    icon_cache: PathBuf,
    icon_px: i32,
}

impl Default for WindowsAppCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl WindowsAppCatalog {
    pub fn new() -> Self {
        let roots = start_menu::roots_from_env(
            std::env::var("APPDATA").ok().as_deref(),
            std::env::var("ProgramData").ok().as_deref(),
            std::env::var("LOCALAPPDATA").ok().as_deref(),
        );
        Self {
            roots,
            icon_cache: default_icon_cache_dir(),
            icon_px: icons::ICON_PX,
        }
    }

    /// 使用显式目录构造，供诊断与真实平台检查使用。
    pub fn with_roots(roots: Vec<PathBuf>, icon_cache: PathBuf) -> Self {
        Self {
            roots: start_menu::dedupe_roots(roots),
            icon_cache,
            icon_px: icons::ICON_PX,
        }
    }

    pub fn icon_cache_dir(&self) -> &Path {
        &self.icon_cache
    }

    /// 扫描并返回完整诊断信息。
    pub fn scan_detailed(&self) -> WindowsScanOutcome {
        // 后续的 IShellLinkW / GetImage 都需要当前线程处于 COM 单元中。
        icons::ensure_com_for_shell();
        let mut outcome = WindowsScanOutcome {
            start_menu_roots: self.roots.clone(),
            start_menu_roots_present: self.roots.iter().filter(|root| root.is_dir()).count(),
            ..Default::default()
        };
        let mut ranked: Vec<(u32, AppEntry)> = Vec::new();

        self.scan_shortcuts(&mut outcome, &mut ranked);
        self.scan_registry(&mut outcome, &mut ranked);
        self.scan_packaged_apps(&mut outcome, &mut ranked);

        let mut entries = dedupe_entries(ranked);
        entries.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        outcome.entries = entries;
        outcome
    }

    fn scan_shortcuts(&self, outcome: &mut WindowsScanOutcome, ranked: &mut Vec<(u32, AppEntry)>) {
        let code_page = system_code_page();
        let encoding = shell_link::code_page_encoding(code_page);
        let windir = std::env::var("WINDIR").ok();

        for root in &self.roots {
            for shortcut in start_menu::discover_shortcuts(root) {
                outcome.shortcuts_seen += 1;
                let display = shortcut.path.to_string_lossy().into_owned();
                let fields = match shell_link::open_fields(&shortcut.path, encoding) {
                    Ok(fields) => fields,
                    Err(error) => {
                        outcome.skip(display, format!("无法解析快捷方式：{error}"));
                        continue;
                    }
                };

                // 纠正 pass：只有它会展开环境变量、也只有它能跟踪被移动的目标。
                let parsed = fields.target.clone();
                let mut corrected = None;
                if shell_link::needs_shell_correction(&fields)
                    || parsed
                        .as_deref()
                        .map(|target| !Path::new(target).exists())
                        .unwrap_or(true)
                {
                    corrected = resolve_with_shell_link(&shortcut.path);
                    if corrected.is_some() {
                        outcome.shell_corrections += 1;
                    }
                }
                let target = shell_link::choose_target(
                    parsed.as_deref(),
                    corrected.as_deref(),
                    |candidate| Path::new(candidate).exists(),
                );
                let target = target.or_else(|| {
                    fields.relative_target.as_deref().and_then(|relative| {
                        shortcut
                            .path
                            .parent()
                            .and_then(|dir| shell_link::resolve_relative(relative, dir))
                            .map(|path| path.to_string_lossy().into_owned())
                    })
                });

                let kind = shell_link::classify_target(target.as_deref(), windir.as_deref());
                match kind {
                    TargetKind::Missing => {
                        outcome.skip(display, "快捷方式没有可解析的目标");
                        continue;
                    }
                    TargetKind::AdvertisedInstaller => {
                        outcome.skip(display, "MSI 广告式快捷方式：目标无法直接重新启动");
                        continue;
                    }
                    TargetKind::NonLaunchableShim => {
                        outcome.skip(display, format!("目标是代理程序：{}", target.unwrap_or_default()));
                        continue;
                    }
                    TargetKind::Executable | TargetKind::DocumentOrFolder => {}
                }
                let target = target.expect("非 Missing 时必有目标");

                let id = exe_id(&target, fields.arguments.as_deref());
                let mut entry = AppEntry {
                    id,
                    name: shortcut.display_name.clone(),
                    comment: fields.description.clone(),
                    icon: None,
                    // `.lnk` 本身交给 Shell 启动：参数串由 Shell 按系统规则分词，
                    // 比我们自己重实现一遍 CommandLineToArgvW 更可靠。
                    exec: vec![shortcut.path.to_string_lossy().into_owned()],
                    desktop_file: None,
                    working_dir: fields.working_dir.as_deref().map(PathBuf::from),
                    wm_class: None,
                    terminal: false,
                    keywords: shortcut.group.iter().cloned().collect(),
                    source: AppSource::StartMenu,
                };
                let icon_source = entry.exec[0].clone();
                self.attach_icon(&mut entry, &icon_source, outcome);
                let rank = if shortcut.group.is_some() {
                    RANK_START_MENU_GROUP
                } else {
                    RANK_START_MENU_ROOT
                };
                ranked.push((rank, entry));
            }
        }
    }

    fn scan_registry(&self, outcome: &mut WindowsScanOutcome, ranked: &mut Vec<(u32, AppEntry)>) {
        for view in RegistryView::all() {
            let Some(key) = view.open(UNINSTALL_KEY) else {
                continue;
            };
            let Ok(names) = key.keys() else {
                continue;
            };
            for name in names {
                outcome.registry_keys_seen += 1;
                let Ok(sub) = key.open(&name) else {
                    continue;
                };
                let values = reg::UninstallValues {
                    key_name: name.clone(),
                    display_name: opt_string(&sub, "DisplayName"),
                    display_version: opt_string(&sub, "DisplayVersion"),
                    publisher: opt_string(&sub, "Publisher"),
                    hive: view.label().to_string(),
                    system_component: opt_u32(&sub, "SystemComponent"),
                    parent_key_name: opt_string(&sub, "ParentKeyName"),
                    parent_display_name: opt_string(&sub, "ParentDisplayName"),
                    windows_installer: opt_u32(&sub, "WindowsInstaller"),
                    release_type: opt_string(&sub, "ReleaseType"),
                    uninstall_string: opt_string(&sub, "UninstallString"),
                    display_icon: opt_string(&sub, "DisplayIcon"),
                    install_location: opt_string(&sub, "InstallLocation"),
                    no_remove: opt_u32(&sub, "NoRemove"),
                };
                if let reg::Verdict::Drop(reason) = reg::classify(&values) {
                    outcome.skip(
                        format!("{}:{}", view.label(), name),
                        reason.reason_zh(),
                    );
                    continue;
                }
                let display_name = values.display_name.clone().unwrap_or_default();
                let target = reg::launch_target(&values);
                let Some(target) = target else {
                    // 保留条目但没有可启动目标：只作为「打开安装目录」的信息项没有意义，
                    // 因此按研究笔记的建议丢弃，并如实记录原因。
                    outcome.skip(
                        format!("{}:{}", view.label(), name),
                        "注册表条目没有可推导的 .exe 启动目标",
                    );
                    continue;
                };
                let mut entry = AppEntry {
                    id: registry_id(view.label(), &name, &display_name),
                    name: display_name.clone(),
                    comment: registry_comment(&values),
                    icon: None,
                    exec: vec![target.to_string_lossy().into_owned()],
                    desktop_file: None,
                    working_dir: values
                        .install_location
                        .as_deref()
                        .filter(|dir| Path::new(dir).is_dir())
                        .map(PathBuf::from),
                    wm_class: None,
                    terminal: false,
                    keywords: Vec::new(),
                    source: AppSource::Registry,
                };
                let icon_source = reg::icon_source(&values)
                    .unwrap_or_else(|| target.to_string_lossy().into_owned());
                self.attach_icon(&mut entry, &icon_source, outcome);
                ranked.push((RANK_REGISTRY, entry));
            }
        }
    }

    fn scan_packaged_apps(&self, outcome: &mut WindowsScanOutcome, ranked: &mut Vec<(u32, AppEntry)>) {
        let apps = match enumerate_start_apps() {
            Ok(apps) => apps,
            Err(reason) => {
                outcome.skip("Get-StartApps", reason);
                return;
            }
        };
        for app in apps {
            outcome.uwp_seen += 1;
            let mut entry = AppEntry {
                id: aumid_id(&app.aumid),
                name: app.name.clone(),
                comment: None,
                icon: None,
                exec: vec![uwp::aumid_program(&app.aumid)],
                desktop_file: None,
                working_dir: None,
                wm_class: None,
                terminal: false,
                // AUMID 本身含有包名，参与元数据匹配很有用。
                keywords: vec![app.aumid.clone()],
                source: AppSource::Uwp,
            };
            let icon_source = uwp::apps_folder_argument(&app.aumid);
            self.attach_icon(&mut entry, &icon_source, outcome);
            ranked.push((RANK_UWP, entry));
        }
    }

    /// 抽取图标；失败时保留条目但把图标标记为未解析（UI 回退到内置占位图标）。
    fn attach_icon(&self, entry: &mut AppEntry, source: &str, outcome: &mut WindowsScanOutcome) {
        match icons::extract_icon_png(source, &self.icon_cache, self.icon_px) {
            Ok(path) => {
                outcome.icons_extracted += 1;
                entry.icon = Some(IconRef {
                    name: entry.name.clone(),
                    path: Some(path),
                });
            }
            Err(reason) => {
                outcome.icons_failed += 1;
                let detail = format!("{}：{reason}", entry.name);
                if outcome.skipped.len() < 64 {
                    outcome.skip(detail, "图标抽取失败");
                }
                entry.icon = Some(IconRef::unresolved(entry.name.clone()));
            }
        }
    }
}

impl AppCatalog for WindowsAppCatalog {
    fn scan(&self) -> Result<Vec<AppEntry>, CatalogError> {
        let outcome = self.scan_detailed();
        if outcome.start_menu_roots_present == 0 && outcome.entries.is_empty() {
            return Err(CatalogError::Unreadable(
                "未找到任何开始菜单目录，注册表与打包应用也没有返回可启动条目".to_string(),
            ));
        }
        Ok(outcome.entries)
    }
}

fn registry_comment(values: &reg::UninstallValues) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(publisher) = values.publisher.as_deref().filter(|v| !v.trim().is_empty()) {
        parts.push(publisher.trim().to_string());
    }
    if let Some(version) = values
        .display_version
        .as_deref()
        .filter(|v| !v.trim().is_empty())
    {
        parts.push(version.trim().to_string());
    }
    (!parts.is_empty()).then(|| parts.join(" "))
}

/// 图标缓存目录：`%LOCALAPPDATA%\Flashcast\icons`（不可用时退回临时目录）。
fn default_icon_cache_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("Flashcast").join("icons")
}

/// 注册表视图：三种 Uninstall 根，用 `wow64_32` / `wow64_64` 而不是拼接 `WOW6432Node`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryView {
    LocalMachine64,
    LocalMachine32,
    CurrentUser,
}

impl RegistryView {
    fn all() -> [Self; 3] {
        [
            Self::LocalMachine64,
            Self::LocalMachine32,
            Self::CurrentUser,
        ]
    }

    fn label(self) -> &'static str {
        match self {
            Self::LocalMachine64 => "hklm",
            Self::LocalMachine32 => "hklm32",
            Self::CurrentUser => "hkcu",
        }
    }

    fn open(self, path: &str) -> Option<windows_registry::Key> {
        match self {
            Self::LocalMachine64 => windows_registry::LOCAL_MACHINE
                .options()
                .wow64_64()
                .read()
                .open(path)
                .ok(),
            Self::LocalMachine32 => windows_registry::LOCAL_MACHINE
                .options()
                .wow64_32()
                .read()
                .open(path)
                .ok(),
            Self::CurrentUser => windows_registry::CURRENT_USER.open(path).ok(),
        }
    }
}

fn opt_string(key: &windows_registry::Key, name: &str) -> Option<String> {
    key.get_string(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn opt_u32(key: &windows_registry::Key, name: &str) -> Option<u32> {
    key.get_u32(name).ok()
}

/// 从注册表读取系统 ACP，用于解码非 Unicode 的 `.lnk` 字符串。
fn system_code_page() -> Option<u32> {
    let key = windows_registry::LOCAL_MACHINE
        .open(r"SYSTEM\CurrentControlSet\Control\Nls\CodePage")
        .ok()?;
    key.get_string("ACP").ok()?.trim().parse().ok()
}

/// 用 `Get-StartApps` 枚举打包应用（含注册了 AUMID 的 Win32 应用）。
///
/// 显式把 PowerShell 的输出编码设为 UTF-8：Windows PowerShell 5.1 默认按控制台
/// OEM 代码页写 stdout，中文应用名会变成乱码。
fn enumerate_start_apps() -> Result<Vec<uwp::StartApp>, String> {
    let output = std::process::Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[Console]::OutputEncoding=[Text.Encoding]::UTF8; Get-StartApps | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|error| format!("无法运行 PowerShell 枚举打包应用：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Get-StartApps 退出码为 {:?}（可能被 AppLocker 或约束语言模式阻止）",
            output.status.code()
        ));
    }
    let json = String::from_utf8_lossy(&output.stdout);
    Ok(uwp::parse_get_start_apps_json(&json))
}

/// `%WINDIR%` 参数留给诊断使用。
pub fn system_windir() -> Option<String> {
    std::env::var("WINDIR").ok().filter(|value| !value.trim().is_empty())
}

// ---------------------------------------------------------------------------
// IShellLinkW 纠正 pass
// ---------------------------------------------------------------------------

/// `IShellLinkW::Resolve` 的标志（shobjidl_core.h）：
/// `SLR_NO_UI` 不弹「找不到目标」的对话框，`SLR_NOSEARCH` 不在磁盘/网络上搜索，
/// 避免扫描时卡在慢速 I/O 上。
const SLR_NO_UI: u32 = 0x0000_0001;
const SLR_NOSEARCH: u32 = 0x0000_0010;

/// 用 `IShellLinkW` 解析快捷方式：`GetPath(fFlags = 0)` 返回**展开环境变量后**的路径。
fn resolve_with_shell_link(lnk: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{Interface, GUID, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, IPersistFile, CLSCTX_INPROC_SERVER, STGM_READ,
    };
    use windows::Win32::UI::Shell::IShellLinkW;

    /// `{00021401-0000-0000-C000-000000000046}`（shobjidl_core.h 的 `CLSID_ShellLink`）。
    /// `windows` crate 不导出这个常量，因此按公共头文件写死；
    /// `IShellLinkW` 的 IID 由 `CoCreateInstance` 通过 `T::IID` 自动提供，不写死。
    const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x0002_1401_0000_0000_C000_0000_0000_0046);

    let wide_path: Vec<u16> = lnk.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let link: IShellLinkW =
            CoCreateInstance(&CLSID_SHELL_LINK, None, CLSCTX_INPROC_SERVER).ok()?;
        let persist: IPersistFile = link.cast().ok()?;
        persist.Load(PCWSTR(wide_path.as_ptr()), STGM_READ).ok()?;
        // 纠正 pass：只在目标已失效时才让 Shell 尝试跟踪移动过的目标。
        let _ = link.Resolve(
            windows::Win32::Foundation::HWND::default(),
            SLR_NO_UI | SLR_NOSEARCH,
        );
        let mut buffer = [0u16; 32_768];
        link.GetPath(&mut buffer, std::ptr::null_mut(), 0).ok()?;
        let end = buffer.iter().position(|unit| *unit == 0).unwrap_or(buffer.len());
        let path = String::from_utf16_lossy(&buffer[..end]);
        let _ = persist; // 保持 COM 引用存活到读取完成
        (!path.trim().is_empty()).then(|| path.trim().to_string())
    }
}
