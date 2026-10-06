//! 设备本地数据：应用数据目录下的键值文件，**绝不进入配置工作区**（ADR §8，spec「隐私」）。
//!
//! 这里保存的是「换一台设备就不该沿用」的内容：当前工作区的本机路径、设备路径、
//! 权限状态、日志与凭证。剪贴板历史、附件与索引体量较大，由后续 ticket 放在同一
//! 根目录下的 SQLite 中（ADR §8）；本模块负责的根目录就是它们的家。
//!
//! 工作区只记录可迁移的偏好（设置、插件清单、主题配置、备忘录），因此本模块写入的
//! 任何内容都不应出现在工作区目录树里 —— 集成测试对此有断言。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::workspace::{write_atomic, WorkspaceRemote};

/// 设备本地状态文件。与工作区文件同名会混淆，因此放在设备目录根下。
pub const DEVICE_STATE_FILE: &str = "device-local.json";

/// 设备本地的 Git 凭证文件（https 令牌）。单独一个文件，Unix 下权限 0600。
pub const GIT_CREDENTIALS_FILE: &str = "git-credentials.json";

/// 当前配置工作区的键。
pub const KEY_WORKSPACE_PATH: &str = "workspacePath";

/// 工作区 ↔ 远端关系表（按工作区路径索引的 JSON）。
pub const KEY_WORKSPACE_REMOTES: &str = "workspaceRemotes";

/// 为某个远端主机保存的 https 令牌。**只**存在于设备本地目录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredToken {
    /// 远端主机（含端口，小写）。
    pub host: String,
    /// 用作 git 用户名的值（GitHub 用 `x-access-token`，GitLab 用 `oauth2`）。
    pub username: String,
    /// 访问令牌本体。绝不写入工作区、配置或日志。
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeviceError {
    #[error("设备本地数据读写失败：{0}")]
    Io(String),
    #[error("设备本地数据损坏：{0}")]
    Corrupt(String),
}

impl From<std::io::Error> for DeviceError {
    fn from(error: std::io::Error) -> Self {
        DeviceError::Io(error.to_string())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct DeviceState {
    #[serde(default)]
    values: BTreeMap<String, String>,
}

/// 设备本地存储。
#[derive(Debug, Clone)]
pub struct DeviceStore {
    root: PathBuf,
}

impl DeviceStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 应用数据目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// 设备本地状态文件。
    pub fn file(&self) -> PathBuf {
        self.root.join(DEVICE_STATE_FILE)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>, DeviceError> {
        Ok(self.load()?.values.get(key).cloned())
    }

    pub fn put(&self, key: &str, value: &str) -> Result<(), DeviceError> {
        let mut state = self.load()?;
        state.values.insert(key.to_string(), value.to_string());
        self.save(&state)
    }

    /// 上次使用的配置工作区（本机路径，属于设备本地数据）。
    pub fn workspace_path(&self) -> Result<Option<PathBuf>, DeviceError> {
        Ok(self.get(KEY_WORKSPACE_PATH)?.map(PathBuf::from))
    }

    pub fn set_workspace_path(&self, path: Option<&Path>) -> Result<(), DeviceError> {
        match path {
            Some(path) => self.put(KEY_WORKSPACE_PATH, &path.to_string_lossy()),
            None => {
                let mut state = self.load()?;
                state.values.remove(KEY_WORKSPACE_PATH);
                self.save(&state)
            }
        }
    }

    /// 某个工作区对应的远端关系。ticket 16 的同步据此确定远端、默认分支与上游。
    pub fn workspace_remote(
        &self,
        workspace: &Path,
    ) -> Result<Option<WorkspaceRemote>, DeviceError> {
        let Some(raw) = self.get(KEY_WORKSPACE_REMOTES)? else {
            return Ok(None);
        };
        let map: BTreeMap<String, WorkspaceRemote> = serde_json::from_str(&raw)
            .map_err(|error| DeviceError::Corrupt(format!("{KEY_WORKSPACE_REMOTES}：{error}")))?;
        Ok(map.get(&workspace.to_string_lossy().into_owned()).cloned())
    }

    /// 记录工作区 ↔ 远端关系；传 `None` 时删除该工作区的记录。
    pub fn set_workspace_remote(
        &self,
        workspace: &Path,
        remote: Option<&WorkspaceRemote>,
    ) -> Result<(), DeviceError> {
        let raw = self.get(KEY_WORKSPACE_REMOTES)?;
        let mut map: BTreeMap<String, WorkspaceRemote> = match raw {
            Some(raw) => serde_json::from_str(&raw).map_err(|error| {
                DeviceError::Corrupt(format!("{KEY_WORKSPACE_REMOTES}：{error}"))
            })?,
            None => BTreeMap::new(),
        };
        let key = workspace.to_string_lossy().into_owned();
        match remote {
            Some(remote) => {
                map.insert(key, remote.clone());
            }
            None => {
                map.remove(&key);
            }
        }
        let text =
            serde_json::to_string(&map).map_err(|error| DeviceError::Io(error.to_string()))?;
        self.put(KEY_WORKSPACE_REMOTES, &text)
    }

    fn load(&self) -> Result<DeviceState, DeviceError> {
        let path = self.file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(DeviceState::default())
            }
            Err(error) => return Err(DeviceError::Io(error.to_string())),
        };
        serde_json::from_str(&text)
            .map_err(|error| DeviceError::Corrupt(format!("{}：{error}", path.display())))
    }

    fn save(&self, state: &DeviceState) -> Result<(), DeviceError> {
        std::fs::create_dir_all(&self.root)?;
        let text = serde_json::to_string_pretty(state)
            .map_err(|error| DeviceError::Io(error.to_string()))?;
        write_atomic(&self.file(), text.as_bytes())
            .map_err(|error| DeviceError::Io(error.to_string()))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CredentialState {
    /// 按主机索引（小写，含端口）。
    #[serde(default)]
    tokens: BTreeMap<String, StoredToken>,
}

/// 设备本地的 Git 凭证存储：https 令牌按主机保存，供克隆与同步复用。
///
/// 与工作区严格分离（ADR §8、spec「凭证不写入工作区或日志」）：文件放在应用数据
/// 目录，Unix 下权限 `0600`。ssh 的凭证不在这里 —— 那些复用 ssh-agent 与 `~/.ssh`。
#[derive(Debug, Clone)]
pub struct CredentialStore {
    root: PathBuf,
}

impl CredentialStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// 凭证文件路径。
    pub fn file(&self) -> PathBuf {
        self.root.join(GIT_CREDENTIALS_FILE)
    }

    /// 该主机保存的令牌。
    pub fn token(&self, host: &str) -> Result<Option<StoredToken>, DeviceError> {
        Ok(self.load()?.tokens.get(&host.to_ascii_lowercase()).cloned())
    }

    /// 保存该主机的令牌（覆盖同主机旧值）。
    pub fn set_token(&self, token: &StoredToken) -> Result<(), DeviceError> {
        let mut state = self.load()?;
        state
            .tokens
            .insert(token.host.to_ascii_lowercase(), token.clone());
        self.save(&state)
    }

    /// 删除该主机的令牌。
    pub fn remove_token(&self, host: &str) -> Result<(), DeviceError> {
        let mut state = self.load()?;
        state.tokens.remove(&host.to_ascii_lowercase());
        self.save(&state)
    }

    fn load(&self) -> Result<CredentialState, DeviceError> {
        let path = self.file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(CredentialState::default())
            }
            Err(error) => return Err(DeviceError::Io(error.to_string())),
        };
        serde_json::from_str(&text)
            .map_err(|error| DeviceError::Corrupt(format!("{}：{error}", path.display())))
    }

    fn save(&self, state: &CredentialState) -> Result<(), DeviceError> {
        std::fs::create_dir_all(&self.root)?;
        let text = serde_json::to_string_pretty(state)
            .map_err(|error| DeviceError::Io(error.to_string()))?;
        let path = self.file();
        write_atomic(&path, text.as_bytes()).map_err(|error| DeviceError::Io(error.to_string()))?;
        // 令牌文件只给当前用户读写。Unix 之外（Windows）依赖用户目录的 ACL。
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(())
    }
}
