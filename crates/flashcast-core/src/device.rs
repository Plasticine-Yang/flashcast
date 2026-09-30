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

use crate::workspace::write_atomic;

/// 设备本地状态文件。与工作区文件同名会混淆，因此放在设备目录根下。
pub const DEVICE_STATE_FILE: &str = "device-local.json";

/// 当前配置工作区的键。
pub const KEY_WORKSPACE_PATH: &str = "workspacePath";

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
