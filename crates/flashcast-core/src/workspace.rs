//! 配置工作区：用户选择的本地 Git 仓库，保存人类可读的配置文件（ADR §8）。
//!
//! 目录布局：：
//!
//! ```text
//! <工作区>/
//!   settings.toml    设置（TOML）
//!   manifest.json    插件清单（JSON；语义由 ticket 07 落地）
//!   theme.json       主题配置（JSON；语义由 ticket 06 落地）
//!   memos/*.md       带 front matter 的 Markdown 备忘录（ticket 13）
//! ```
//!
//! 本模块只负责工作区**本身**：目录校验、Git 仓库识别、设置文件的读写。
//! 写入一律原子完成（同目录临时文件 + `rename`），供文件监听据此抑制自身写入
//! （见 [`crate::watch`]）。
//!
//! 设备本地数据（剪贴板历史、缓存、设备路径、权限状态、日志、凭证）**不**放在
//! 工作区，见 [`crate::device`]。

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::{Settings, SettingsError};

/// 工作区内的设置文件（TOML）。
pub const SETTINGS_FILE: &str = "settings.toml";
/// 工作区内的插件清单（JSON）。
pub const MANIFEST_FILE: &str = "manifest.json";
/// 工作区内的主题配置（JSON）。
pub const THEME_FILE: &str = "theme.json";
/// 工作区内的备忘录目录。
pub const MEMOS_DIR: &str = "memos";

/// 工作区根目录下由应用维护的文件。
pub const WORKSPACE_FILES: [&str; 3] = [SETTINGS_FILE, MANIFEST_FILE, THEME_FILE];

/// 原子写入使用的临时文件后缀。文件监听把它当作噪声丢弃。
pub const TEMP_SUFFIX: &str = ".flashcast.tmp";

/// 工作区操作的失败原因。全部为面向用户的中文描述。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WorkspaceError {
    #[error("目录不存在：{}", .0.display())]
    NotFound(PathBuf),
    #[error("路径不是目录：{}", .0.display())]
    NotADirectory(PathBuf),
    #[error("目录不可写：{}", .0.display())]
    NotWritable(PathBuf),
    #[error("目标目录非空，已拒绝初始化以免覆盖已有文件：{}", .0.display())]
    NonEmptyDirectory(PathBuf),
    #[error("配置无效：{0}")]
    InvalidSettings(String),
    #[error("Git 仓库无法读取：{0}")]
    Git(String),
    #[error("文件读写失败：{0}")]
    Io(String),
    #[error("无法监听工作区变更：{0}")]
    Watch(String),
}

impl From<std::io::Error> for WorkspaceError {
    fn from(error: std::io::Error) -> Self {
        WorkspaceError::Io(error.to_string())
    }
}

/// 当前工作区与它的有效性。UI 用它显示「已关联 / 未关联 / 失败原因」。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceStatus {
    /// 工作区根目录；`None` 表示尚未关联工作区。
    pub path: Option<PathBuf>,
    /// Git 仓库目录（`.git`）；不是 Git 仓库时为 `None`。
    pub git_dir: Option<PathBuf>,
    /// 当前工作区是否可用。未关联时为 `false`。
    pub valid: bool,
    /// 设置文件路径（已关联时）。
    pub settings_file: Option<PathBuf>,
    /// 设置是否落在工作区文件里。未关联时只在内存中，重启不保留。
    pub persisted: bool,
    /// 最近一次失败的中文原因；`None` 表示没有已知问题。
    pub error: Option<String>,
}

impl WorkspaceStatus {
    /// 尚未关联任何工作区。
    pub fn unlinked(error: Option<String>) -> Self {
        Self {
            path: None,
            git_dir: None,
            valid: false,
            settings_file: None,
            persisted: false,
            error,
        }
    }

    pub fn linked(workspace: &Workspace) -> Self {
        Self {
            path: Some(workspace.root().to_path_buf()),
            git_dir: workspace.git_dir().map(Path::to_path_buf),
            valid: true,
            settings_file: Some(workspace.settings_path()),
            persisted: true,
            error: None,
        }
    }
}

/// 一次外部修改被处理后的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceReload {
    /// 触发本次处理的文件。
    pub path: PathBuf,
    /// 是否真的改变了生效设置。相同内容不重复应用（幂等，避免写入循环）。
    pub applied: bool,
    /// 处理之后生效的设置。
    pub settings: Settings,
    /// 配置无效时的中文原因；此时保留上一次有效设置。
    pub error: Option<String>,
}

/// 一个已校验的配置工作区。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
    git_dir: Option<PathBuf>,
}

impl Workspace {
    /// 打开并校验一个已存在的目录作为工作区。**不写任何文件**。
    ///
    /// 校验内容：目录存在且可写；若目录内已有 Git 仓库则必须能正常打开；
    /// 若已有 `settings.toml` 则必须能解析并通过校验。
    pub fn open(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let raw = root.as_ref();
        if !raw.exists() {
            return Err(WorkspaceError::NotFound(raw.to_path_buf()));
        }
        if !raw.is_dir() {
            return Err(WorkspaceError::NotADirectory(raw.to_path_buf()));
        }
        let root = raw
            .canonicalize()
            .map_err(|error| WorkspaceError::Io(error.to_string()))?;
        if !is_writable(&root) {
            return Err(WorkspaceError::NotWritable(root));
        }
        let git_dir = detect_git_dir(&root)?;
        let workspace = Self { root, git_dir };
        // 已有设置文件必须有效，否则拒绝把它设为当前工作区。
        workspace.read_settings()?;
        Ok(workspace)
    }

    /// 在空目录（或尚不存在的目录）上初始化工作区与它的 Git 仓库。
    ///
    /// 目标目录非空时一律拒绝：宁可不初始化，也不覆盖用户已有文件。
    pub fn init(root: impl AsRef<Path>) -> Result<Self, WorkspaceError> {
        let raw = root.as_ref();
        if raw.exists() {
            if !raw.is_dir() {
                return Err(WorkspaceError::NotADirectory(raw.to_path_buf()));
            }
            let mut entries = fs::read_dir(raw)?;
            if entries.next().is_some() {
                return Err(WorkspaceError::NonEmptyDirectory(raw.to_path_buf()));
            }
        }
        let created = !raw.exists();
        fs::create_dir_all(raw)?;
        let root = raw
            .canonicalize()
            .map_err(|error| WorkspaceError::Io(error.to_string()))?;

        // 失败时回滚：只删除本次自己创建的目录，绝不动已有目录。
        if !is_writable(&root) {
            let error = WorkspaceError::NotWritable(root.clone());
            return Err(rollback_created(&root, created, error));
        }

        let mut opts = git2::RepositoryInitOptions::new();
        opts.no_reinit(false).initial_head("main");
        let repo = git2::Repository::init_opts(&root, &opts).map_err(|error| {
            rollback_created(
                &root,
                created,
                WorkspaceError::Git(error.message().to_string()),
            )
        })?;
        let git_dir = Some(repo.path().to_path_buf());
        drop(repo);

        let workspace = Self { root, git_dir };
        // 新工作区从默认设置开始；目录为空，不存在覆盖风险。
        let settings = Settings::default();
        workspace.write_settings(&settings)?;
        Ok(workspace)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Git 仓库目录（`.git`）。工作区不是 Git 仓库时为 `None`。
    pub fn git_dir(&self) -> Option<&Path> {
        self.git_dir.as_deref()
    }

    pub fn settings_path(&self) -> PathBuf {
        self.root.join(SETTINGS_FILE)
    }

    /// 读取设置。文件不存在返回 `None`；文件存在但无效时返回错误。
    pub fn read_settings(&self) -> Result<Option<Settings>, WorkspaceError> {
        match self.read_settings_bytes()? {
            Some(bytes) => Ok(Some(self.parse_settings(&bytes)?)),
            None => Ok(None),
        }
    }

    /// 读取设置文件的**原始字节**。文件不存在返回 `None`。
    ///
    /// 宿主重载时先取字节、再解析，而不是直接读成 `Settings`：无论解析成功与否，读到的
    /// 内容都要记进监听层的账本，否则 macOS / Windows 会把这次读上报成一次修改事件
    /// （见 [`crate::watch`] 模块文档「重载自己的读也必须记账」）。
    pub fn read_settings_bytes(&self) -> Result<Option<Vec<u8>>, WorkspaceError> {
        match fs::read(self.settings_path()) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(WorkspaceError::Io(error.to_string())),
        }
    }

    /// 把设置文件的原始字节解析并校验为设置。
    pub fn parse_settings(&self, bytes: &[u8]) -> Result<Settings, WorkspaceError> {
        let text = std::str::from_utf8(bytes).map_err(|error| {
            WorkspaceError::Io(format!("{SETTINGS_FILE} 不是有效文本：{error}"))
        })?;
        let settings = Settings::from_toml(text).map_err(|error| {
            WorkspaceError::InvalidSettings(format!("{SETTINGS_FILE} 解析失败：{error}"))
        })?;
        settings.validate().map_err(|error| {
            WorkspaceError::InvalidSettings(format!("{SETTINGS_FILE} 内容不合法：{error}"))
        })?;
        Ok(settings)
    }

    /// 原子写入设置文件。
    pub fn write_settings(&self, settings: &Settings) -> Result<(), WorkspaceError> {
        let bytes = settings
            .to_toml()
            .map_err(|error: SettingsError| WorkspaceError::Io(error.to_string()))?;
        self.write_settings_bytes(bytes.as_bytes())
    }

    /// 原子写入设置文件的原始字节。调用方负责先注册自写抑制（见 [`crate::watch`]）。
    pub fn write_settings_bytes(&self, bytes: &[u8]) -> Result<(), WorkspaceError> {
        write_atomic(&self.settings_path(), bytes)
    }
}

/// 原子写入：同目录临时文件 + `rename`（ADR §8）。
///
/// 先 `fsync` 临时文件再 `rename`，避免崩溃时留下半截文件；重命名在同一文件系统
/// 内是原子的，因此监听侧只会看到一次 `Create` + `Rename`，容易合并。
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), WorkspaceError> {
    let dir = path
        .parent()
        .ok_or_else(|| WorkspaceError::Io(format!("无法确定 {} 的父目录", path.display())))?;
    let name = path
        .file_name()
        .ok_or_else(|| WorkspaceError::Io(format!("路径缺少文件名：{}", path.display())))?
        .to_string_lossy()
        .into_owned();
    let tmp = dir.join(format!(".{name}{TEMP_SUFFIX}"));
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(WorkspaceError::Io(error.to_string()));
    }
    // 目录项落盘，保证 rename 之后的内容在崩溃后仍可见。
    #[cfg(unix)]
    {
        if let Ok(handle) = fs::File::open(dir) {
            let _ = handle.sync_all();
        }
    }
    Ok(())
}

/// 初始化失败时的回滚：只删除本次自己创建的目录。
fn rollback_created(root: &Path, created: bool, error: WorkspaceError) -> WorkspaceError {
    if created {
        let _ = fs::remove_dir_all(root);
    }
    error
}

fn detect_git_dir(root: &Path) -> Result<Option<PathBuf>, WorkspaceError> {
    match git2::Repository::open(root) {
        Ok(repo) => Ok(Some(repo.path().to_path_buf())),
        // 目录里没有仓库是正常情况：工作区可以是普通目录，克隆由 ticket 14 提供。
        Err(error) if error.code() == git2::ErrorCode::NotFound => Ok(None),
        Err(error) => Err(WorkspaceError::Git(error.message().to_string())),
    }
}

/// 目录是否可写。用一个临时探针文件判断，随后立即删除。
///
/// 只读文件系统的权限位不一定反映实际可写性（例如挂载为只读），因此实际写一次
/// 比读权限位可靠。探针文件名以 `.` 开头且带 `.tmp` 后缀，会被监听过滤掉。
fn is_writable(root: &Path) -> bool {
    let probe = root.join(format!(".flashcast-write-probe{TEMP_SUFFIX}"));
    match fs::File::create(&probe) {
        Ok(_) => {
            let _ = fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}
