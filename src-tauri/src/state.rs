//! 宿主外壳的共享状态。只包含窗口、托盘、快捷键与转发所需的管线，
//! 不含任何业务判断（业务逻辑在 `flashcast-core::Host`）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use flashcast_core::Host;
use flashcast_platform::shortcut::HotkeyHandle;
use flashcast_platform::{FocusTracker, FocusedApp, PlatformAdapters};

/// 全局快捷键的注册状态，UI 用它显示冲突或不支持的原因。
#[derive(Default)]
pub struct HotkeyState {
    pub handle: Option<HotkeyHandle>,
    /// 注册失败原因；`None` 表示注册成功或尚未尝试。
    pub error: Option<String>,
    /// 当前生效的快捷键写法。
    pub label: String,
}

pub struct AppState {
    pub host: Arc<Host>,
    pub platform: PlatformAdapters,
    /// 唤起前处于前台的应用程序（用于后续的粘贴恢复）。
    pub previous_app: Mutex<Option<FocusedApp>>,
    pub hotkey: Mutex<HotkeyState>,
    /// 图标 data URL 缓存，按文件路径索引。
    pub icons: Mutex<HashMap<PathBuf, Option<String>>>,
    /// 最近一次唤起的时间，用于避免刚显示就被失焦事件立刻隐藏。
    pub last_summon: Mutex<Option<Instant>>,
}

impl AppState {
    pub fn new(host: Arc<Host>, platform: PlatformAdapters) -> Self {
        Self {
            host,
            platform,
            previous_app: Mutex::new(None),
            hotkey: Mutex::new(HotkeyState::default()),
            icons: Mutex::new(HashMap::new()),
            last_summon: Mutex::new(None),
        }
    }

    /// 记录唤起前的前台应用。Wayland 下 `capture` 会返回「不支持」，
    /// 此时保留上一次的可用信息并返回原因。
    pub fn capture_previous_app(&self) -> Result<FocusedApp, String> {
        let mut guard = lock(&self.previous_app);
        match self.platform.focus.capture() {
            Ok(app) => {
                *guard = Some(app.clone());
                Ok(app)
            }
            Err(error) => Err(error.to_string()),
        }
    }

    pub fn previous_app(&self) -> Option<FocusedApp> {
        lock(&self.previous_app).clone()
    }

    pub fn mark_summoned(&self) {
        *lock(&self.last_summon) = Some(Instant::now());
    }

    /// 是否处于「刚唤起」的保护窗口内。
    pub fn is_recent_summon(&self, window: std::time::Duration) -> bool {
        lock(&self.last_summon)
            .map(|instant| instant.elapsed() < window)
            .unwrap_or(false)
    }
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}
