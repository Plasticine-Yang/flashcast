//! Linux 全局快捷键：基于 `global-hotkey`（X11 `XGrabKey`）。
//!
//! 该后端**只支持 X11**。Wayland 会话下抓取即便返回成功也不会收到按键事件，
//! 因此默认拒绝注册并返回 [`HotkeyError::BackendUnavailable`]，让 UI 明确提示
//! 用户改用托盘入口，而不是给出一个「看起来注册成功」的假象。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::capability::SessionType;
use crate::hotkey::{HotkeySpec, Key, Modifier};
use crate::shortcut::{HotkeyError, HotkeyHandle, HotkeyManager, PressCallback};

use super::{detect_session_type, force_x11_backend};

/// 进程级回调表。`global-hotkey` 的事件接收端是进程级单例，因此回调用全局表分发。
static CALLBACKS: OnceLock<Mutex<HashMap<u32, PressCallback>>> = OnceLock::new();

fn callbacks() -> &'static Mutex<HashMap<u32, PressCallback>> {
    CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_callbacks() -> std::sync::MutexGuard<'static, HashMap<u32, PressCallback>> {
    callbacks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn manager() -> Result<&'static GlobalHotKeyManager, HotkeyError> {
    static MANAGER: OnceLock<Result<GlobalHotKeyManager, String>> = OnceLock::new();
    MANAGER
        .get_or_init(|| GlobalHotKeyManager::new().map_err(|error| error.to_string()))
        .as_ref()
        .map_err(|reason| HotkeyError::BackendUnavailable {
            reason: format!("无法初始化全局快捷键后端：{reason}"),
        })
}

/// 启动事件监听线程。进程内只启动一次。
fn ensure_listener() {
    static LISTENER: OnceLock<()> = OnceLock::new();
    LISTENER.get_or_init(|| {
        std::thread::Builder::new()
            .name("flashcast-hotkey-listener".to_string())
            .spawn(|| {
                let receiver = GlobalHotKeyEvent::receiver();
                while let Ok(event) = receiver.recv() {
                    if event.state() != HotKeyState::Pressed {
                        continue;
                    }
                    let callback = lock_callbacks().get(&event.id()).cloned();
                    if let Some(callback) = callback {
                        callback();
                    }
                }
            })
            .ok();
    });
}

pub struct LinuxHotkeyManager {
    session: SessionType,
    force_x11: bool,
}

impl Default for LinuxHotkeyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxHotkeyManager {
    pub fn new() -> Self {
        Self {
            session: detect_session_type(),
            force_x11: force_x11_backend(),
        }
    }

    pub fn with_session(session: SessionType, force_x11: bool) -> Self {
        Self {
            session,
            force_x11,
        }
    }

    /// 当前会话下是否允许尝试注册。
    pub fn backend_allowed(&self) -> Result<(), HotkeyError> {
        match self.session {
            SessionType::X11 => Ok(()),
            SessionType::Wayland if self.force_x11 => Ok(()),
            SessionType::Wayland => Err(HotkeyError::BackendUnavailable {
                reason: "Wayland 会话不支持全局快捷键抓取；请使用托盘入口，或在 X11 会话下运行"
                    .to_string(),
            }),
            SessionType::Headless => Err(HotkeyError::BackendUnavailable {
                reason: "当前没有桌面会话，无法注册全局快捷键".to_string(),
            }),
            SessionType::Unknown | SessionType::NotApplicable => {
                Err(HotkeyError::BackendUnavailable {
                    reason: "无法确定会话类型，未尝试注册全局快捷键".to_string(),
                })
            }
        }
    }
}

impl HotkeyManager for LinuxHotkeyManager {
    fn register(
        &self,
        spec: &HotkeySpec,
        on_press: PressCallback,
    ) -> Result<HotkeyHandle, HotkeyError> {
        self.backend_allowed()?;
        let manager = manager()?;
        let hotkey = to_backend_hotkey(spec)?;
        let id = hotkey.id();

        manager.register(hotkey).map_err(|error| classify(error, spec))?;
        lock_callbacks().insert(id, on_press);
        ensure_listener();
        Ok(HotkeyHandle::new(u64::from(id), spec.clone(), {
            // 句柄不自持回调副本，避免重复持有；触发走全局表。
            dummy_callback(id)
        }))
    }

    fn update(
        &self,
        handle: &HotkeyHandle,
        spec: &HotkeySpec,
    ) -> Result<HotkeyHandle, HotkeyError> {
        // 保持回调不变：先取出旧回调，注销后重新注册。
        let callback = lock_callbacks()
            .get(&(handle.id as u32))
            .cloned()
            .unwrap_or_else(|| dummy_callback(handle.id as u32));
        self.unregister(handle)?;
        match self.register(spec, callback.clone()) {
            Ok(new_handle) => Ok(new_handle),
            Err(error) => {
                // 更新失败时恢复原快捷键，避免用户彻底失去入口。
                let _ = self.register(&handle.spec, callback);
                Err(error)
            }
        }
    }

    fn unregister(&self, handle: &HotkeyHandle) -> Result<(), HotkeyError> {
        let id = handle.id as u32;
        lock_callbacks().remove(&id);
        let Ok(manager) = manager() else {
            return Ok(());
        };
        let hotkey = to_backend_hotkey(&handle.spec)?;
        match manager.unregister(hotkey) {
            Ok(()) => Ok(()),
            // 已经不在注册表中不视为错误。
            Err(global_hotkey::Error::FailedToUnRegister(_)) => Ok(()),
            Err(error) => Err(HotkeyError::Other {
                reason: error.to_string(),
            }),
        }
    }
}

/// 把内部规格转换为后端快捷键。
fn to_backend_hotkey(spec: &HotkeySpec) -> Result<HotKey, HotkeyError> {
    let code = to_code(spec.key).ok_or_else(|| HotkeyError::Other {
        reason: format!("后端不支持按键 {}", spec.key.canonical()),
    })?;
    let mut modifiers = Modifiers::empty();
    for modifier in &spec.modifiers {
        modifiers |= match modifier {
            Modifier::Control => Modifiers::CONTROL,
            Modifier::Alt => Modifiers::ALT,
            Modifier::Shift => Modifiers::SHIFT,
            Modifier::Super => Modifiers::SUPER,
        };
    }
    if modifiers.is_empty() {
        return Err(HotkeyError::InvalidSpec(
            crate::hotkey::HotkeySpecError::MissingModifier {
                raw: spec.raw.clone(),
            },
        ));
    }
    Ok(HotKey::new(Some(modifiers), code))
}

fn to_code(key: Key) -> Option<Code> {
    Some(match key {
        Key::Letter(c) => match c.to_ascii_uppercase() {
            'A' => Code::KeyA,
            'B' => Code::KeyB,
            'C' => Code::KeyC,
            'D' => Code::KeyD,
            'E' => Code::KeyE,
            'F' => Code::KeyF,
            'G' => Code::KeyG,
            'H' => Code::KeyH,
            'I' => Code::KeyI,
            'J' => Code::KeyJ,
            'K' => Code::KeyK,
            'L' => Code::KeyL,
            'M' => Code::KeyM,
            'N' => Code::KeyN,
            'O' => Code::KeyO,
            'P' => Code::KeyP,
            'Q' => Code::KeyQ,
            'R' => Code::KeyR,
            'S' => Code::KeyS,
            'T' => Code::KeyT,
            'U' => Code::KeyU,
            'V' => Code::KeyV,
            'W' => Code::KeyW,
            'X' => Code::KeyX,
            'Y' => Code::KeyY,
            'Z' => Code::KeyZ,
            _ => return None,
        },
        Key::Digit(0) => Code::Digit0,
        Key::Digit(1) => Code::Digit1,
        Key::Digit(2) => Code::Digit2,
        Key::Digit(3) => Code::Digit3,
        Key::Digit(4) => Code::Digit4,
        Key::Digit(5) => Code::Digit5,
        Key::Digit(6) => Code::Digit6,
        Key::Digit(7) => Code::Digit7,
        Key::Digit(8) => Code::Digit8,
        Key::Digit(9) => Code::Digit9,
        Key::Digit(_) => return None,
        Key::Space => Code::Space,
        Key::Enter => Code::Enter,
        Key::Escape => Code::Escape,
        Key::Tab => Code::Tab,
        Key::ArrowUp => Code::ArrowUp,
        Key::ArrowDown => Code::ArrowDown,
        Key::ArrowLeft => Code::ArrowLeft,
        Key::ArrowRight => Code::ArrowRight,
        Key::Function(1) => Code::F1,
        Key::Function(2) => Code::F2,
        Key::Function(3) => Code::F3,
        Key::Function(4) => Code::F4,
        Key::Function(5) => Code::F5,
        Key::Function(6) => Code::F6,
        Key::Function(7) => Code::F7,
        Key::Function(8) => Code::F8,
        Key::Function(9) => Code::F9,
        Key::Function(10) => Code::F10,
        Key::Function(11) => Code::F11,
        Key::Function(12) => Code::F12,
        Key::Function(_) => return None,
    })
}

/// 把后端错误分类为可展示的原因。
fn classify(error: global_hotkey::Error, spec: &HotkeySpec) -> HotkeyError {
    match error {
        // X11 下 XGrabKey 返回 BadAccess 就走这里：快捷键被其他应用占用。
        global_hotkey::Error::FailedToRegister(_) => HotkeyError::Conflict {
            spec: spec.canonical(),
        },
        global_hotkey::Error::AlreadyRegistered(_) => HotkeyError::AlreadyRegistered {
            spec: spec.canonical(),
        },
        global_hotkey::Error::OsError(inner) => HotkeyError::BackendUnavailable {
            reason: inner.to_string(),
        },
        global_hotkey::Error::HotKeyParseError(message)
        | global_hotkey::Error::UnrecognizedHotKeyCode(message)
        | global_hotkey::Error::EmptyHotKeyToken(message)
        | global_hotkey::Error::UnexpectedHotKeyFormat(message) => {
            HotkeyError::Other { reason: message }
        }
        other => HotkeyError::Other {
            reason: other.to_string(),
        },
    }
}

/// 句柄上的占位回调。真正的分发走全局表，因此这里不会被执行。
fn dummy_callback(id: u32) -> PressCallback {
    std::sync::Arc::new(move || {
        let callback = lock_callbacks().get(&id).cloned();
        if let Some(callback) = callback {
            callback();
        }
    })
}
