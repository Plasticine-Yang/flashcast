//! macOS `.app` 应用包的发现与 `Info.plist` 解析。
//!
//! 本模块**刻意不含任何 macOS 专有 API**：目录遍历与 `Info.plist` 解析都只用
//! `std::fs` 与跨平台的 `plist` crate，因此可以在 Linux 开发机与任意 CI runner
//! 上用真实夹具目录验证。真正需要 macOS 的部分（图标渲染、启动、焦点、快捷键）
//! 在 `macos` 的其余子模块中，由 macOS runner 上的 `flashcast-platform-check`
//! 做实测。
//!
//! 身份规则（见研究笔记 §2.5）：`CFBundleIdentifier` 是「同一个应用」的主键，
//! 包路径是同一 bundle id 多份副本之间的区分依据。两者共同构成条目 id，
//! 且**绝不丢弃**任何一份副本 —— `LaunchServices` 解析 `open -b <id>` 时选哪一份
//! 是任意的，所以启动始终按路径进行。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog::{AppEntry, AppSource, IconRef};

/// 每个扫描根目录下递归的最大层数（根的直接子项为第 1 层）。
///
/// Apple 自身的布局已经需要 2 层（`/System/Applications/Utilities/Terminal.app`），
/// 厂商目录还要更深一层（`/Applications/Adobe Photoshop 2024/Adobe Photoshop.app`），
/// 因此 3 层是务实的上界；不会递归进入已发现的 `.app` 包。
pub const MAX_SCAN_DEPTH: usize = 3;

/// 已知的应用目录。
///
/// 这些取值与 `NSApplicationDirectory` 搜索路径一致；显式列出而不调用
/// `NSFileManager` 是为了让「扫描哪些目录」成为可在任意平台验证的纯函数。
pub fn default_roots(home: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/Applications/Utilities"),
    ];
    if let Some(home) = home {
        roots.push(home.join("Applications"));
    }
    roots
}

/// 从 `HOME` 环境变量推断用户应用目录。
pub fn default_roots_from_env() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    default_roots(home.as_deref())
}

/// 扫描输入。
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    pub max_depth: usize,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            roots: default_roots_from_env(),
            max_depth: MAX_SCAN_DEPTH,
        }
    }
}

impl ScanOptions {
    /// 使用显式根目录构造，供真实平台检查与测试使用。
    pub fn with_roots<I, P>(roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self {
            roots: roots.into_iter().map(Into::into).collect(),
            max_depth: MAX_SCAN_DEPTH,
        }
    }
}

/// 跳过某个候选包的原因。诊断报告逐条列出，不静默丢弃。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// 缺少 `Contents/Info.plist`。
    MissingInfoPlist,
    /// `Info.plist` 无法解析。
    UnreadableInfoPlist,
    /// 不是可启动的应用包（`CFBundlePackageType` 不是 `APPL`）。
    NotAnApplication,
    /// `CFBundleExecutable` 指向的文件不存在，包已损坏。
    MissingExecutable,
    /// `LSBackgroundOnly`：没有界面的后台进程，不属于启动器范围。
    BackgroundOnly,
}

impl SkipReason {
    pub fn as_str(self) -> &'static str {
        match self {
            SkipReason::MissingInfoPlist => "缺少 Contents/Info.plist",
            SkipReason::UnreadableInfoPlist => "Contents/Info.plist 无法解析",
            SkipReason::NotAnApplication => "不是应用包（CFBundlePackageType 不是 APPL）",
            SkipReason::MissingExecutable => "CFBundleExecutable 指向的文件不存在",
            SkipReason::BackgroundOnly => "LSBackgroundOnly 的后台进程，没有可交互界面",
        }
    }
}

/// 被跳过的候选包。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub path: PathBuf,
    pub reason: SkipReason,
}

/// 扫描结果与诊断信息。
#[derive(Debug, Default)]
pub struct ScanOutcome {
    pub entries: Vec<AppEntry>,
    /// 实际发现的 `.app` 候选包数量（含随后被跳过的）。
    pub bundles_seen: usize,
    /// 本次使用的扫描根目录。
    pub roots: Vec<PathBuf>,
    pub skipped: Vec<Skipped>,
    pub warnings: Vec<String>,
}

/// 一个 `.app` 包解析出的元信息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleInfo {
    pub path: PathBuf,
    /// `finalize_bundles` 分配的稳定条目 id；`read_bundle` 会先填入「视为唯一」的取值。
    pub entry_id: String,
    pub bundle_id: Option<String>,
    /// 已消歧的展示名。
    pub name: String,
    pub executable: Option<String>,
    pub executable_path: Option<PathBuf>,
    /// `CFBundleIconFile` 原值（可能不含扩展名）。
    pub icon_file: Option<String>,
    /// `CFBundleIconFile` 解析到的**实际存在**的图标文件。
    ///
    /// 现代应用只提供资源目录（`Assets.car`）时该值为 `None`，图标由
    /// `NSWorkspace` 渲染兜底；本字段只作为 `.icns` 快速路径的记录。
    pub icon_path: Option<PathBuf>,
    pub version: Option<String>,
    pub package_type: Option<String>,
    pub is_background_only: bool,
}

/// 读取并解析一个 `.app` 包。
pub fn read_bundle(path: &Path) -> Result<BundleInfo, SkipReason> {
    let plist_path = path.join("Contents/Info.plist");
    if !plist_path.is_file() {
        return Err(SkipReason::MissingInfoPlist);
    }
    let value = plist::Value::from_file(&plist_path).map_err(|_| SkipReason::UnreadableInfoPlist)?;
    let mut info = bundle_from_plist(path, &value)?;
    // 单独读取时没有全局视图，先按「bundle id 唯一」填入 id；
    // `finalize_bundles` 会在发现重复 bundle id 时改写。
    info.entry_id = bundle_entry_id(info.bundle_id.as_deref(), &info.path);
    Ok(info)
}

/// 把 `Info.plist` 的字典映射为 [`BundleInfo`]（纯逻辑，便于夹具测试）。
pub fn bundle_from_plist(path: &Path, value: &plist::Value) -> Result<BundleInfo, SkipReason> {
    let dict = value
        .as_dictionary()
        .ok_or(SkipReason::UnreadableInfoPlist)?;

    let package_type = string_value(dict, "CFBundlePackageType");
    if let Some(kind) = package_type.as_deref() {
        if !kind.eq_ignore_ascii_case("APPL") {
            return Err(SkipReason::NotAnApplication);
        }
    }
    if bool_value(dict, "LSBackgroundOnly") {
        return Err(SkipReason::BackgroundOnly);
    }

    let bundle_id = string_value(dict, "CFBundleIdentifier").filter(|id| !id.is_empty());
    let executable = string_value(dict, "CFBundleExecutable").filter(|name| !name.is_empty());
    let executable_path = resolve_executable(path, executable.as_deref())?;

    let name = string_value(dict, "CFBundleDisplayName")
        .filter(|name| !name.trim().is_empty())
        .or_else(|| string_value(dict, "CFBundleName").filter(|name| !name.trim().is_empty()))
        .unwrap_or_else(|| bundle_stem(path));

    let icon_file = string_value(dict, "CFBundleIconFile").filter(|name| !name.is_empty());
    let icon_path = icon_file
        .as_deref()
        .and_then(|file| resolve_icon_path(path, file));

    Ok(BundleInfo {
        path: path.to_path_buf(),
        entry_id: String::new(),
        bundle_id,
        name,
        executable,
        executable_path,
        icon_file,
        icon_path,
        version: string_value(dict, "CFBundleShortVersionString")
            .or_else(|| string_value(dict, "CFBundleVersion"))
            .filter(|version| !version.is_empty()),
        package_type,
        is_background_only: false,
    })
}

/// `CFBundleIconFile` → 实际存在的图标文件。
///
/// 该键可以不带 `.icns` 扩展名，也可以带子路径；两种写法都要支持，
/// 且只有文件确实存在时才返回路径（不做猜测）。
pub fn resolve_icon_path(bundle: &Path, icon_file: &str) -> Option<PathBuf> {
    let resources = bundle.join("Contents/Resources");
    let candidate = resources.join(icon_file);
    if candidate.is_file() {
        return Some(candidate);
    }
    if Path::new(icon_file).extension().is_none() {
        let with_extension = candidate.with_extension("icns");
        if with_extension.is_file() {
            return Some(with_extension);
        }
    }
    None
}

/// 稳定条目 id：bundle id + 路径。
///
/// 缺少 bundle id 的损坏包回退到路径，并因此失去「同一应用」的语义。
pub fn bundle_entry_id(bundle_id: Option<&str>, path: &Path) -> String {
    match bundle_id {
        Some(id) if !id.is_empty() => format!("{id}::{}", path.display()),
        _ => path.display().to_string(),
    }
}

/// 为同一批应用包分配稳定 id 并消歧重名。
///
/// 规则：
/// - bundle id 唯一时，条目 id 就是 bundle id 本身（干净、可用于匹配）；
/// - 同一 bundle id 有多份副本时，路径字典序最小的一份保留纯 bundle id，
///   其余使用 `<bundle_id>@<路径>`，并给展示名加上父目录后缀；
/// - 完全没有 bundle id 时使用包路径作为 id。
///
/// 结果只取决于这一批包的集合，与输入顺序无关。
pub fn finalize_bundles(bundles: &mut [BundleInfo]) {
    let mut by_id: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for (index, bundle) in bundles.iter().enumerate() {
        if let Some(id) = bundle.bundle_id.as_deref().filter(|id| !id.is_empty()) {
            by_id.entry(id.to_lowercase()).or_default().push(index);
        }
    }

    for indices in by_id.values() {
        let mut ordered = indices.clone();
        ordered.sort_by(|a, b| bundles[*a].path.cmp(&bundles[*b].path));
        for (position, index) in ordered.iter().enumerate() {
            let bundle = &mut bundles[*index];
            let id = bundle.bundle_id.clone().unwrap_or_default();
            if ordered.len() == 1 {
                bundle.entry_id = id;
                continue;
            }
            if position == 0 {
                bundle.entry_id = id;
                continue;
            }
            bundle.entry_id = format!("{id}@{}", bundle.path.display());
            if let Some(parent) = bundle.path.parent() {
                let label = parent
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| parent.display().to_string());
                bundle.name = format!("{}（{label}）", bundle.name);
            }
        }
    }

    for bundle in bundles.iter_mut() {
        if bundle.entry_id.is_empty() {
            bundle.entry_id = bundle_entry_id(bundle.bundle_id.as_deref(), &bundle.path);
        }
    }
}

/// 映射为宿主消费的 [`AppEntry`]。
///
/// `exec` 只有一项：**应用包路径**。macOS 的启动实现据此用
/// `/usr/bin/open -a <路径>` 打开，而不是按 bundle id 解析 —— 重复 bundle id
/// 真实存在，`LaunchServices` 会任意挑一份。
pub fn entry_from_bundle(info: &BundleInfo) -> AppEntry {
    let id = if info.entry_id.is_empty() {
        bundle_entry_id(info.bundle_id.as_deref(), &info.path)
    } else {
        info.entry_id.clone()
    };
    let mut keywords = Vec::new();
    if let Some(bundle_id) = info.bundle_id.as_deref() {
        keywords.push(bundle_id.to_string());
    }
    if let Some(executable) = info.executable.as_deref() {
        keywords.push(executable.to_string());
    }

    AppEntry {
        id,
        name: info.name.clone(),
        comment: info.version.as_ref().map(|version| format!("版本 {version}")),
        icon: Some(IconRef {
            name: info
                .icon_file
                .clone()
                .unwrap_or_else(|| info.name.clone()),
            path: info.icon_path.clone(),
        }),
        exec: vec![info.path.to_string_lossy().into_owned()],
        desktop_file: Some(info.path.join("Contents/Info.plist")),
        working_dir: None,
        wm_class: info.bundle_id.clone(),
        terminal: false,
        keywords,
        source: AppSource::Bundle,
    }
}

/// 扫描应用目录，产出可启动的软件条目。
pub fn scan(options: &ScanOptions) -> ScanOutcome {
    let mut outcome = ScanOutcome {
        roots: options.roots.clone(),
        ..ScanOutcome::default()
    };

    let mut candidates = Vec::new();
    for root in &options.roots {
        collect_bundles(root, options.max_depth, &mut candidates);
    }

    // 重叠的根目录（`/Applications` 与 `/Applications/Utilities`）会看到同一个包：
    // 按解析后的真实路径去重，避免同一个应用出现两次。
    let mut seen: BTreeMap<PathBuf, ()> = BTreeMap::new();
    let mut bundles: Vec<BundleInfo> = Vec::new();
    for candidate in candidates {
        let key = std::fs::canonicalize(&candidate).unwrap_or_else(|_| candidate.clone());
        if seen.insert(key, ()).is_some() {
            continue;
        }
        outcome.bundles_seen += 1;
        match read_bundle(&candidate) {
            Ok(bundle) => bundles.push(bundle),
            Err(reason) => outcome.skipped.push(Skipped {
                path: candidate,
                reason,
            }),
        }
    }

    finalize_bundles(&mut bundles);
    let mut entries: Vec<AppEntry> = bundles.iter().map(entry_from_bundle).collect();
    // 稳定输出：不依赖文件系统的返回顺序。
    entries.sort_by(|a, b| a.id.cmp(&b.id));
    outcome.entries = entries;
    outcome
}

/// 递归收集 `.app` 候选包，不进入已发现的包内部。
fn collect_bundles(dir: &Path, max_depth: usize, out: &mut Vec<PathBuf>) {
    if max_depth == 0 {
        return;
    }
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = read_dir.flatten().map(|entry| entry.path()).collect();
    children.sort();
    for child in children {
        if is_hidden(&child) {
            continue;
        }
        // 跟随符号链接：`/Applications` 里指向别处的 `.app` 是真实存在的布局。
        let is_dir = std::fs::metadata(&child)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false);
        if !is_dir {
            continue;
        }
        if has_app_extension(&child) {
            out.push(child);
        } else {
            collect_bundles(&child, max_depth - 1, out);
        }
    }
}

fn has_app_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.eq_ignore_ascii_case("app"))
        .unwrap_or(false)
}

fn is_hidden(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

/// `Foo.app` → `Foo`。
fn bundle_stem(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// 定位包内可执行文件。
///
/// `CFBundleExecutable` 指向的文件必须存在，否则包已损坏；缺少该键时，
/// `Contents/MacOS` 下恰好一个可执行文件才被接受。
fn resolve_executable(bundle: &Path, executable: Option<&str>) -> Result<Option<PathBuf>, SkipReason> {
    let macos_dir = bundle.join("Contents/MacOS");
    if let Some(executable) = executable {
        let candidate = macos_dir.join(executable);
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        return Err(SkipReason::MissingExecutable);
    }
    let mut found: Vec<PathBuf> = std::fs::read_dir(&macos_dir)
        .map(|read_dir| {
            read_dir
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.is_file())
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    match found.len() {
        1 => Ok(found.pop()),
        _ => Err(SkipReason::MissingExecutable),
    }
}

fn string_value(dict: &plist::Dictionary, key: &str) -> Option<String> {
    dict.get(key)
        .and_then(|value| value.as_string())
        .map(str::to_string)
}

/// 读取 plist 布尔值，接受 `<true/>` 与 `"1"` / `"YES"` 这类字符串写法。
fn bool_value(dict: &plist::Dictionary, key: &str) -> bool {
    match dict.get(key) {
        Some(plist::Value::Boolean(value)) => *value,
        Some(plist::Value::Integer(value)) => value.as_signed().map(|v| v != 0).unwrap_or(false),
        Some(plist::Value::String(value)) => {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes"
            )
        }
        _ => false,
    }
}
