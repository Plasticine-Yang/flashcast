//! Chrome 的发现与启动（ADR §5 的 `ChromeProvider`）。
//!
//! 本模块只做两件事：**发现**（可执行文件、用户数据目录、profile 列表）与**启动**
//! （把 URL 作为独立 argv 元素交给 Chrome）。书签文件的解析、索引与检索属于宿主业务
//! 逻辑，放在 `flashcast-core`（ADR §2）。
//!
//! 设计要点（依据 `notes/research/chrome-bookmarks.md`）：
//!
//! - **路径推导是纯函数**：三个平台的用户数据目录与可执行文件候选都由不碰文件系统的
//!   函数给出，因此 Windows / macOS 的路径布局也能在 Linux 开发机上被测试。
//! - **不经 shell**：启动参数是一个 argv 数组，URL 永远是独立元素，因此书签里的
//!   `&`、`?`、空格与引号不可能被解释成命令。
//! - **不等待、不看退出码**：Chrome 已在运行时新进程会交接给现有浏览器进程并几乎立刻
//!   退出，因此退出码 0 不能证明页面打开（研究笔记 §4）。这里只回收进程表项。
//! - **`checksum` 永不校验**：它属于解析层，见 `flashcast-core` 的书签解析。
//! - `--profile-directory` 取的是**目录名**（`Profile 1`），不是 `Local State` 里的
//!   显示名；显示名只用于 UI。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// 浏览器品牌。v0.1.0 的 Chrome 书签插件支持这三种同源浏览器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChromeBrand {
    Chrome,
    Chromium,
    Edge,
}

impl ChromeBrand {
    pub fn id(self) -> &'static str {
        match self {
            ChromeBrand::Chrome => "chrome",
            ChromeBrand::Chromium => "chromium",
            ChromeBrand::Edge => "edge",
        }
    }

    pub fn label_zh(self) -> &'static str {
        match self {
            ChromeBrand::Chrome => "Google Chrome",
            ChromeBrand::Chromium => "Chromium",
            ChromeBrand::Edge => "Microsoft Edge",
        }
    }
}

/// 用户数据目录的来源。决定启动时是否需要显式传 `--user-data-dir`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UserDataOrigin {
    /// 平台默认目录：Chrome 自己就会用它。
    Default,
    /// snap 包（`~/snap/<name>/common/<name>`）：启动包装器在沙箱内映射，不需再传。
    Snap,
    /// flatpak 包（`~/.var/app/<app-id>/config/<name>`）：同上。
    Flatpak,
    /// 用户在默认位置之外的目录。
    Custom,
}

/// 一个候选的用户数据目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserDataCandidate {
    pub brand: ChromeBrand,
    pub path: PathBuf,
    pub origin: UserDataOrigin,
}

impl UserDataCandidate {
    pub fn new(brand: ChromeBrand, path: impl Into<PathBuf>, origin: UserDataOrigin) -> Self {
        Self {
            brand,
            path: path.into(),
            origin,
        }
    }

    /// 启动时是否需要显式传 `--user-data-dir=<path>`。
    ///
    /// 只有**默认位置之外的目录**才需要：平台默认目录 Chrome 本来就会用，而 snap /
    /// flatpak 的目录由各自的启动包装器映射到沙箱内，宿主再传一遍反而可能让 Chrome
    /// 认为这是另一个用户数据目录，从而启动第二个浏览器进程（研究笔记 §4）。
    pub fn requires_user_data_dir_switch(&self) -> bool {
        matches!(self.origin, UserDataOrigin::Custom)
    }
}

/// 一个候选的可执行文件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinaryCandidate {
    pub brand: ChromeBrand,
    pub path: PathBuf,
}

impl BinaryCandidate {
    pub fn new(brand: ChromeBrand, path: impl Into<PathBuf>) -> Self {
        Self {
            brand,
            path: path.into(),
        }
    }
}

/// 在 `PATH` 里按名字查找候选可执行文件。纯函数：只看传入的 `PATH` 字符串，
/// 因此可以用临时目录在任意平台上测试。
pub fn binaries_in_path(names: &[&str], path_var: Option<&str>) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Some(path_var) = path_var else {
        return found;
    };
    for name in names {
        for dir in std::env::split_paths(path_var) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let candidate = dir.join(name);
            if !found.contains(&candidate) {
                found.push(candidate);
            }
        }
    }
    found
}

/// `Local State` 里 `profile.info_cache` 的一条记录（我们只关心这几个字段）。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProfileInfo {
    /// 显示名（`profile.info_cache[*].name`）。
    pub name: Option<String>,
    /// 已登录的 Google 账号邮箱；未登录时为空。
    pub user_name: Option<String>,
    /// 企业策略管理（`is_managed` / `hosted_domain` / `force_signin_profile_locked`）。
    pub managed: bool,
    pub hosted_domain: Option<String>,
}

/// 一个 Chrome profile。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeProfile {
    /// **目录名**（`Default`、`Profile 1`）。`--profile-directory` 用的就是它。
    pub dir: String,
    /// 面向用户的显示名；`Local State` 里没有记录时退化为目录名。
    pub name: String,
    /// 已登录账号邮箱；未登录时为空。
    pub user_name: Option<String>,
    /// 是否受企业管理。
    pub managed: bool,
    /// `Bookmarks` 文件路径（可能不存在：全新 profile 是正常状态）。
    pub bookmarks: PathBuf,
    /// 文件是否存在。
    pub has_bookmarks: bool,
    /// 文件是否能打开（权限不足时为 false）。
    pub bookmarks_readable: bool,
    /// 无法读取时的中文原因。
    pub unreadable_reason: Option<String>,
}

/// 发现结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeEnvironment {
    pub brand: ChromeBrand,
    /// Chrome 可执行文件。
    pub binary: PathBuf,
    pub user_data_dir: PathBuf,
    pub user_data_origin: UserDataOrigin,
    /// 启动时是否需要显式传 `--user-data-dir`。
    pub pass_user_data_dir: bool,
    pub profiles: Vec<ChromeProfile>,
    /// 搜索过的可执行文件候选，用于「Chrome 未安装」的可操作提示。
    pub searched_binaries: Vec<PathBuf>,
    /// 非致命问题（例如 `Local State` 解析失败、用户数据目录尚不存在）。
    pub warnings: Vec<String>,
}

impl ChromeEnvironment {
    pub fn profile(&self, dir: &str) -> Option<&ChromeProfile> {
        self.profiles.iter().find(|profile| profile.dir == dir)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChromeError {
    #[error("没有找到 Chrome 可执行文件；已尝试：{searched}")]
    NotInstalled { searched: String },
    #[error("Chrome 用户数据目录不可读：{0}")]
    UserDataDirUnreadable(String),
    #[error("profile 目录不存在：{0}")]
    ProfileMissing(String),
    #[error("profile 不可读：{0}")]
    ProfileUnreadable(String),
    #[error("书签文件不可读：{0}")]
    BookmarksUnreadable(String),
    #[error("链接不受支持：{0}")]
    InvalidUrl(String),
    #[error("无法启动 Chrome：{0}")]
    LaunchFailed(String),
    #[error("当前平台的 Chrome 支持尚未提供（{0}）")]
    Unsupported(String),
}

/// 一次启动请求：可执行文件 + 完整参数数组。**没有**任何 shell 字符串。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeLaunchRequest {
    pub program: PathBuf,
    pub args: Vec<String>,
}

impl ChromeLaunchRequest {
    pub fn new(program: impl Into<PathBuf>, args: Vec<String>) -> Self {
        Self {
            program: program.into(),
            args,
        }
    }

    /// 完整的 argv（第 0 项是可执行文件）。测试断言的就是这个向量。
    pub fn argv(&self) -> Vec<String> {
        let mut argv = Vec::with_capacity(self.args.len() + 1);
        argv.push(self.program.to_string_lossy().into_owned());
        argv.extend(self.args.iter().cloned());
        argv
    }
}

/// 启动结果。`pid` 只用于诊断；**不**代表页面已经打开。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChromeLaunch {
    pub pid: Option<u32>,
    pub argv: Vec<String>,
}

/// Chrome 适配器：发现 + 启动。
///
/// 宿主只通过它接触 Chrome。关联的 profile 目录**只能**来自 [`Self::discover`] 的结果，
/// 不能由调用方直接给出路径（否则就是一个任意文件读取 / 任意 argv 原语）。
pub trait ChromeProvider: Send + Sync {
    /// 发现可执行文件、用户数据目录与 profile。
    fn discover(&self) -> Result<ChromeEnvironment, ChromeError>;

    /// 按 argv 启动 Chrome：不等待、不读取退出码。
    fn launch(&self, request: &ChromeLaunchRequest) -> Result<ChromeLaunch, ChromeError>;
}

/// 平台无关的发现：按候选顺序做真实的存在性检查。
///
/// 优先选与可执行文件同品牌的用户数据目录，其次任意存在的目录，最后退回第一个候选
/// （此时通常是「Chrome 已安装但还没有用户数据目录」，属于正常空状态）。
pub fn discover_from_paths(
    binaries: &[BinaryCandidate],
    user_data: &[UserDataCandidate],
) -> Result<ChromeEnvironment, ChromeError> {
    let mut warnings = Vec::new();
    let searched_binaries: Vec<PathBuf> = binaries.iter().map(|item| item.path.clone()).collect();
    let binary = binaries
        .iter()
        .find(|item| item.path.is_file())
        .ok_or_else(|| ChromeError::NotInstalled {
            searched: searched_binaries
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>()
                .join("、"),
        })?;

    // 优先与可执行文件同品牌的目录：即使它还不存在（Chrome 装好但没运行过），
    // 也好过去读另一个浏览器的书签。同品牌的目录都没有时，才退回其它品牌已存在的目录。
    let existing: Vec<&UserDataCandidate> =
        user_data.iter().filter(|item| item.path.is_dir()).collect();
    let chosen = user_data
        .iter()
        .filter(|item| item.brand == binary.brand)
        .find(|item| item.path.is_dir())
        .or_else(|| user_data.iter().find(|item| item.brand == binary.brand))
        .or_else(|| existing.first().copied())
        .or_else(|| user_data.first());
    let Some(chosen) = chosen else {
        return Err(ChromeError::UserDataDirUnreadable(
            "没有任何候选的 Chrome 用户数据目录".to_string(),
        ));
    };
    if existing.is_empty() {
        warnings.push(format!(
            "还没有找到已存在的 Chrome 用户数据目录，暂按 {} 处理；\
             启动一次 Chrome 后这里会出现 profile",
            chosen.path.display()
        ));
    } else if chosen.brand != binary.brand {
        warnings.push(format!(
            "可执行文件是 {}，但用户数据目录来自 {}（{}）",
            binary.brand.label_zh(),
            chosen.brand.label_zh(),
            chosen.path.display()
        ));
    }

    let (profiles, profile_warnings) = enumerate_profiles(&chosen.path);
    warnings.extend(profile_warnings);

    Ok(ChromeEnvironment {
        brand: binary.brand,
        binary: binary.path.clone(),
        user_data_dir: chosen.path.clone(),
        user_data_origin: chosen.origin,
        pass_user_data_dir: chosen.requires_user_data_dir_switch(),
        profiles,
        searched_binaries,
        warnings,
    })
}

/// 枚举 profile：`Local State` 的 `profile.info_cache` 与目录本身取并集。
///
/// 研究笔记 §2：`info_cache` 会滞后于磁盘（它在 profile 变化时才重写），因此目录里
/// 含有 `Preferences` 或 `Bookmarks` 的子目录也算 profile。目录不可读时只用
/// `info_cache`，并返回可报告的中文原因（不是致命错误）。
pub fn enumerate_profiles(user_data_dir: &Path) -> (Vec<ChromeProfile>, Vec<String>) {
    let mut warnings = Vec::new();
    let (info_cache, local_state_warning) = read_info_cache(user_data_dir);
    if let Some(warning) = local_state_warning {
        warnings.push(warning);
    }

    let mut dirs: Vec<String> = info_cache.keys().cloned().collect();
    match std::fs::read_dir(user_data_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_dir() {
                    continue;
                }
                let Some(name) = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                else {
                    continue;
                };
                // 目录名不是 profile 的（缓存目录、`GrShaderCache` 等）用内容判断。
                let looks_like_profile =
                    path.join("Preferences").is_file() || path.join("Bookmarks").is_file();
                if looks_like_profile && !dirs.contains(&name) {
                    dirs.push(name);
                }
            }
        }
        Err(error) => warnings.push(format!(
            "无法列出 Chrome 用户数据目录 {}：{error}；已退回到 Local State 的记录",
            user_data_dir.display()
        )),
    }

    dirs.sort();
    // `Default` 永远排在最前，其余按显示名 / 目录名稳定排序。
    dirs.sort_by_key(|dir| (dir != "Default", dir.clone()));

    let profiles = dirs
        .into_iter()
        .map(|dir| {
            let info = info_cache.get(&dir).cloned().unwrap_or_default();
            let profile_dir = user_data_dir.join(&dir);
            let bookmarks = profile_dir.join("Bookmarks");
            let has_bookmarks = bookmarks.is_file();
            let (bookmarks_readable, unreadable_reason) = if !has_bookmarks {
                (true, None)
            } else {
                match std::fs::File::open(&bookmarks) {
                    Ok(_) => (true, None),
                    Err(error) => (false, Some(format!("无法读取书签文件：{error}"))),
                }
            };
            let display_name = info
                .name
                .clone()
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| dir.clone());
            ChromeProfile {
                dir,
                name: display_name,
                user_name: info
                    .user_name
                    .clone()
                    .filter(|value| !value.trim().is_empty()),
                managed: info.managed,
                bookmarks,
                has_bookmarks,
                bookmarks_readable,
                unreadable_reason,
            }
        })
        .collect();

    (profiles, warnings)
}

/// 读取 `Local State` 的 `profile.info_cache`。
///
/// 只读这一个字段：该文件还含有 `os_crypt.encrypted_key`（保护 cookies 与保存的密码），
/// **绝不读取、记录或暴露它**（研究笔记 §5）。文件缺失或损坏都只产生警告。
fn read_info_cache(user_data_dir: &Path) -> (BTreeMap<String, ProfileInfo>, Option<String>) {
    let path = user_data_dir.join("Local State");
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (BTreeMap::new(), None)
        }
        Err(error) => {
            return (
                BTreeMap::new(),
                Some(format!("无法读取 {}：{error}", path.display())),
            )
        }
    };
    match parse_local_state(&text) {
        Ok(cache) => (cache, None),
        Err(error) => (
            BTreeMap::new(),
            Some(format!(
                "{} 不是有效的 JSON（{error}）；已改用目录枚举",
                path.display()
            )),
        ),
    }
}

/// 解析 `Local State`，取出 `profile.info_cache`。兼容缺失与多余字段。
pub fn parse_local_state(text: &str) -> Result<BTreeMap<String, ProfileInfo>, ChromeError> {
    #[derive(Deserialize)]
    struct LocalState {
        #[serde(default)]
        profile: Option<ProfileSection>,
    }
    #[derive(Deserialize)]
    struct ProfileSection {
        #[serde(default)]
        info_cache: BTreeMap<String, RawProfileInfo>,
    }
    /// Chrome 写这些标记的类型**不固定**：本机实测 Chrome 153 的
    /// `is_managed` 是整数 `0`，而 `force_signin_profile_locked` 是布尔。
    /// 因此这里接受布尔、整数与字符串，绝不因为类型差异丢掉整个 `info_cache`。
    #[derive(Deserialize, Default)]
    #[serde(transparent)]
    struct LenientFlag(Option<serde_json::Value>);

    impl LenientFlag {
        fn get(&self) -> bool {
            match self.0.as_ref() {
                Some(serde_json::Value::Bool(value)) => *value,
                Some(serde_json::Value::Number(number)) => {
                    number.as_i64().map(|value| value != 0).unwrap_or(false)
                }
                Some(serde_json::Value::String(text)) => matches!(
                    text.trim().to_ascii_lowercase().as_str(),
                    "true" | "1" | "yes"
                ),
                _ => false,
            }
        }
    }

    #[derive(Deserialize, Default)]
    struct RawProfileInfo {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        user_name: Option<String>,
        #[serde(default)]
        is_managed: LenientFlag,
        #[serde(default)]
        hosted_domain: Option<String>,
        #[serde(default)]
        force_signin_profile_locked: LenientFlag,
    }

    let state: LocalState = serde_json::from_str(text).map_err(|error| {
        ChromeError::UserDataDirUnreadable(format!("Local State 解析失败：{error}"))
    })?;
    let Some(profile) = state.profile else {
        return Ok(BTreeMap::new());
    };
    Ok(profile
        .info_cache
        .into_iter()
        .map(|(dir, raw)| {
            // `"NO_HOSTED_DOMAIN"` 是 Chrome 在**未**受管理时的固定写法（本机实测），
            // 不能当成「有 hosted_domain 就是受管理」。
            let hosted_domain = raw
                .hosted_domain
                .filter(|domain| !is_absent_hosted_domain(domain));
            let managed = raw.is_managed.get()
                || raw.force_signin_profile_locked.get()
                || hosted_domain.is_some();
            (
                dir,
                ProfileInfo {
                    name: raw.name,
                    user_name: raw.user_name,
                    managed,
                    hosted_domain,
                },
            )
        })
        .collect())
}

/// Chrome 用来表示「没有 hosted_domain」的字面量。
pub const NO_HOSTED_DOMAIN: &str = "NO_HOSTED_DOMAIN";

/// 该 `hosted_domain` 取值是否表示「没有企业域」。
fn is_absent_hosted_domain(value: &str) -> bool {
    let value = value.trim();
    value.is_empty() || value.eq_ignore_ascii_case(NO_HOSTED_DOMAIN)
}

/// 启动时传给 Chrome 的参数。
///
/// `--profile-directory` 取**目录名**；`--user-data-dir` 只在
/// `pass_user_data_dir` 为真（默认位置之外的目录）时加入；URL 永远是最后一个独立
/// argv 元素。绝不经过 `sh -c` / `cmd /C`，也不做字符串拼接。
pub fn build_open_args(profile_dir: &str, user_data_dir: Option<&Path>, url: &str) -> Vec<String> {
    let mut args = vec![
        format!("--profile-directory={profile_dir}"),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];
    if let Some(user_data_dir) = user_data_dir {
        args.push(format!("--user-data-dir={}", user_data_dir.display()));
    }
    args.push(url.to_string());
    args
}

/// URL 长度上限。真实书签 URL 极少超过它；超出说明输入本身有问题。
pub const MAX_URL_CHARS: usize = 4096;

/// 校验要交给 Chrome 的 URL。
///
/// - 只接受 `http` / `https`（`--app` 等开关不接受其它 scheme，`javascript:` 也不该被
///   当作可打开的书签交给浏览器）；
/// - 拒绝控制字符与换行；
/// - 拒绝以 `-` 开头（会被 Chrome 当成开关）；
/// - 不做百分号编码或任何改写：URL 原样作为 argv 元素传递。
pub fn validate_open_url(url: &str) -> Result<&str, ChromeError> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err(ChromeError::InvalidUrl("链接为空".to_string()));
    }
    if trimmed.chars().count() > MAX_URL_CHARS {
        return Err(ChromeError::InvalidUrl(format!(
            "链接过长（超过 {MAX_URL_CHARS} 个字符）"
        )));
    }
    let lower = trimmed.to_ascii_lowercase();
    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
        return Err(ChromeError::InvalidUrl(format!(
            "只支持 http / https 链接，当前是「{}」",
            trimmed.chars().take(32).collect::<String>()
        )));
    }
    if trimmed.starts_with('-') {
        return Err(ChromeError::InvalidUrl(
            "链接不能以 - 开头（会被 Chrome 当成命令行开关）".to_string(),
        ));
    }
    if let Some(bad) = trimmed
        .chars()
        .find(|c| c.is_control() || *c == '\n' || *c == '\r' || *c == '\t')
    {
        return Err(ChromeError::InvalidUrl(format!(
            "链接含有控制字符（U+{:04X}）",
            bad as u32
        )));
    }
    Ok(trimmed)
}

/// 启动 Chrome：`spawn` 后立即返回，**绝不**等待、也不读取退出码。
///
/// Chrome 已在运行时新进程会交接给现有浏览器进程并几乎立刻退出，因此：
///
/// - 调用方不能把「进程启动成功」当成「页面已打开」；
/// - 这里用一个短命线程回收进程表项，避免长期运行累积僵尸进程；该线程丢弃退出码。
pub fn spawn_chrome(request: &ChromeLaunchRequest) -> Result<ChromeLaunch, ChromeError> {
    use std::process::{Command, Stdio};

    let mut command = Command::new(&request.program);
    command.args(&request.args);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| {
        ChromeError::LaunchFailed(format!("{}：{error}", request.program.display()))
    })?;
    let pid = child.id();
    // 只回收进程表项，不判断退出码。
    let _ = std::thread::Builder::new()
        .name("flashcast-chrome-reaper".to_string())
        .spawn(move || {
            let _ = child.wait();
        });
    Ok(ChromeLaunch {
        pid: Some(pid),
        argv: request.argv(),
    })
}

/// 按候选路径工作的适配器：发现走真实的存在性检查，启动走 [`spawn_chrome`]。
///
/// 各平台的 `ChromeProvider` 实现只是它 + 一份候选路径表；测试可以用临时夹具目录
/// 构造候选，从而在不接触真实 Chrome profile 的前提下走完真实发现代码。
pub struct PathChromeProvider {
    binaries: Vec<BinaryCandidate>,
    user_data: Vec<UserDataCandidate>,
}

impl PathChromeProvider {
    pub fn new(binaries: Vec<BinaryCandidate>, user_data: Vec<UserDataCandidate>) -> Self {
        Self {
            binaries,
            user_data,
        }
    }

    pub fn binaries(&self) -> &[BinaryCandidate] {
        &self.binaries
    }

    pub fn user_data(&self) -> &[UserDataCandidate] {
        &self.user_data
    }

    /// 发现（启动前调用方可以据此判断 profile 是否存在）。
    pub fn discover_environment(&self) -> Result<ChromeEnvironment, ChromeError> {
        discover_from_paths(&self.binaries, &self.user_data)
    }
}

impl ChromeProvider for PathChromeProvider {
    fn discover(&self) -> Result<ChromeEnvironment, ChromeError> {
        self.discover_environment()
    }

    fn launch(&self, request: &ChromeLaunchRequest) -> Result<ChromeLaunch, ChromeError> {
        spawn_chrome(request)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_args_keep_the_url_as_one_element() {
        let args = build_open_args("Profile 1", None, "https://example.com/搜索?q=书签&x=1 2");
        assert_eq!(
            args,
            vec![
                "--profile-directory=Profile 1".to_string(),
                "--no-first-run".to_string(),
                "--no-default-browser-check".to_string(),
                "https://example.com/搜索?q=书签&x=1 2".to_string(),
            ]
        );
    }

    #[test]
    fn user_data_dir_switch_is_added_only_for_custom_dirs() {
        let args = build_open_args("Default", Some(Path::new("/tmp/udd")), "https://a.example/");
        assert_eq!(
            args,
            vec![
                "--profile-directory=Default".to_string(),
                "--no-first-run".to_string(),
                "--no-default-browser-check".to_string(),
                "--user-data-dir=/tmp/udd".to_string(),
                "https://a.example/".to_string(),
            ]
        );
        assert!(
            !UserDataCandidate::new(ChromeBrand::Chrome, "/tmp/a", UserDataOrigin::Default)
                .requires_user_data_dir_switch()
        );
        assert!(
            UserDataCandidate::new(ChromeBrand::Chrome, "/tmp/a", UserDataOrigin::Custom)
                .requires_user_data_dir_switch()
        );
    }

    #[test]
    fn url_validation_rejects_dangerous_values() {
        assert!(validate_open_url("https://example.com/").is_ok());
        assert!(validate_open_url("  http://example.com/a?b=c&d=e  ").is_ok());
        assert!(validate_open_url("https://例子.example/路径?q=中文").is_ok());
        assert!(validate_open_url("javascript:alert(1)").is_err());
        assert!(validate_open_url("file:///etc/passwd").is_err());
        assert!(validate_open_url("chrome://settings").is_err());
        assert!(validate_open_url("").is_err());
        assert!(validate_open_url("https://example.com/\n--headless").is_err());
        assert!(validate_open_url(&format!("https://example.com/{}", "x".repeat(5000))).is_err());
    }

    #[test]
    fn local_state_without_info_cache_is_empty_not_an_error() {
        assert!(parse_local_state("{}").unwrap().is_empty());
        assert!(parse_local_state(r#"{"profile":{}}"#).unwrap().is_empty());
        assert!(parse_local_state("不是 JSON").is_err());
    }

    #[test]
    fn local_state_maps_directory_names_to_display_names() {
        let cache = parse_local_state(
            r#"{
              "os_crypt": { "encrypted_key": "绝不读取" },
              "profile": {
                "info_cache": {
                  "Default": { "name": "个人", "user_name": "me@example.com" },
                  "Profile 1": { "name": "工作", "is_managed": true, "hosted_domain": "corp.example" }
                }
              }
            }"#,
        )
        .expect("Local State 必须能解析");
        assert_eq!(cache.len(), 2);
        assert_eq!(cache["Default"].name.as_deref(), Some("个人"));
        assert_eq!(
            cache["Default"].user_name.as_deref(),
            Some("me@example.com")
        );
        assert!(!cache["Default"].managed);
        assert!(cache["Profile 1"].managed);
    }

    #[test]
    fn real_world_flag_types_do_not_break_the_parser() {
        // 本机实测的 Chrome 153 形状：is_managed 是整数 0，hosted_domain 是
        // "NO_HOSTED_DOMAIN"，force_signin_profile_locked 是布尔。
        let cache = parse_local_state(
            r#"{
              "profile": { "info_cache": {
                "Default": {
                  "name": "文锋",
                  "user_name": "me@example.com",
                  "is_managed": 0,
                  "hosted_domain": "NO_HOSTED_DOMAIN",
                  "force_signin_profile_locked": false
                },
                "Profile 1": {
                  "name": "工作",
                  "is_managed": 1,
                  "force_signin_profile_locked": 1
                },
                "Profile 2": { "name": "托管域", "hosted_domain": "corp.example" }
              } }
            }"#,
        )
        .expect("真实形状的 Local State 必须能解析");
        assert_eq!(cache["Default"].name.as_deref(), Some("文锋"));
        assert!(
            !cache["Default"].managed,
            "整数 0 + NO_HOSTED_DOMAIN 不是受管理"
        );
        assert_eq!(cache["Default"].hosted_domain, None);
        assert!(cache["Profile 1"].managed, "整数 1 就是受管理");
        assert!(cache["Profile 2"].managed, "有企业域就是受管理");
    }

    #[test]
    fn binaries_in_path_uses_the_given_path_string() {
        let found = binaries_in_path(&["google-chrome", "chromium"], Some("/usr/bin:/opt/x"));
        assert_eq!(
            found,
            vec![
                PathBuf::from("/usr/bin/google-chrome"),
                PathBuf::from("/opt/x/google-chrome"),
                PathBuf::from("/usr/bin/chromium"),
                PathBuf::from("/opt/x/chromium"),
            ]
        );
        assert!(binaries_in_path(&["google-chrome"], None).is_empty());
    }
}
