//! `global-hotkey` 后端的共享部分，供 Linux 与 macOS 两个平台实现复用。
//!
//! 后端的进程级回调表、按键映射与错误分类与平台无关，只有「当前环境是否允许
//! 注册」这一判断因平台而异（Linux 看 X11/Wayland/Mirror 会话，macOS 看是否存在
//! 可交互的桌面会话）。把共享部分集中在这里，可以保证两个平台对同一个
//! [`HotkeySpec`] 映射到同一个后端按键，并且这段映射能在 Linux 上被真实测试。

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::hotkey::{HotkeySpec, Key, Modifier};
use crate::shortcut::{HotkeyError, HotkeyHandle, PressCallback};

/// 进程级回调表。`global-hotkey` 的事件接收端是进程级单例，因此回调用全局表分发。
static CALLBACKS: OnceLock<Mutex<HashMap<u32, PressCallback>>> = OnceLock::new();

fn callbacks() -> &'static Mutex<HashMap<u32, PressCallback>> {
    CALLBACKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock_callbacks() -> MutexGuard<'static, HashMap<u32, PressCallback>> {
    callbacks()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 进程级后端管理器。初始化失败会被记住并作为可展示原因返回。
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

/// 注册快捷键。调用方必须先自行判断当前环境是否允许注册。
pub fn register(spec: &HotkeySpec, on_press: PressCallback) -> Result<HotkeyHandle, HotkeyError> {
    let manager = manager()?;
    let hotkey = to_backend_hotkey(spec)?;
    let id = hotkey.id();

    manager
        .register(hotkey)
        .map_err(|error| classify(error, spec))?;
    lock_callbacks().insert(id, on_press);
    ensure_listener();
    Ok(HotkeyHandle::new(u64::from(id), spec.clone(), {
        // 句柄不自持回调副本，避免重复持有；触发走全局表。
        dummy_callback(id)
    }))
}

/// 替换已注册的快捷键。失败时恢复原快捷键，避免用户彻底失去入口。
pub fn update(handle: &HotkeyHandle, spec: &HotkeySpec) -> Result<HotkeyHandle, HotkeyError> {
    let callback = lock_callbacks()
        .get(&(handle.id as u32))
        .cloned()
        .unwrap_or_else(|| dummy_callback(handle.id as u32));
    unregister(handle)?;
    match register(spec, callback.clone()) {
        Ok(new_handle) => Ok(new_handle),
        Err(error) => {
            let _ = register(&handle.spec, callback);
            Err(error)
        }
    }
}

/// 注销快捷键。重复注销不视为错误。
pub fn unregister(handle: &HotkeyHandle) -> Result<(), HotkeyError> {
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

/// 把内部规格转换为后端快捷键。
pub fn to_backend_hotkey(spec: &HotkeySpec) -> Result<HotKey, HotkeyError> {
    let code = to_code(spec.key).ok_or_else(|| HotkeyError::Other {
        reason: format!("后端不支持按键 {}", spec.key.canonical()),
    })?;
    let mut modifiers = Modifiers::empty();
    for modifier in &spec.modifiers {
        modifiers |= match modifier {
            // `Modifier::Super` 在 macOS 上即 Command：前端用 `Cmd` / `CmdOrCtrl`
            // 书写，解析层已统一映射到 `Super`。
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

/// 内部主键 → 后端键码。不支持的按键返回 `None`，由调用方给出可展示原因。
pub fn to_code(key: Key) -> Option<Code> {
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
pub fn classify(error: global_hotkey::Error, spec: &HotkeySpec) -> HotkeyError {
    match error {
        // X11 下 XGrabKey 返回 BadAccess、macOS 下 RegisterEventHotKey 返回
        // eventHotKeyExistsErr 都走这里：快捷键被其他应用占用。
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::HotkeySpec;

    /// 默认快捷键必须能映射到后端键码 —— 否则用户一开箱就注册失败。
    #[test]
    fn default_hotkey_maps_to_a_backend_code() {
        let spec = HotkeySpec::parse(crate::hotkey::DEFAULT_HOTKEY).expect("默认快捷键可解析");
        let hotkey = to_backend_hotkey(&spec).expect("默认快捷键可映射");
        assert_eq!(hotkey.mods, Modifiers::CONTROL | Modifiers::ALT);
        assert_eq!(hotkey.key, Code::Space);
    }

    /// 全部字母与数字键都应映射成功，且大小写等价（macOS 与 X11 共用该映射）。
    #[test]
    fn every_letter_and_digit_maps_case_insensitively() {
        for c in 'a'..='z' {
            let lower = to_code(Key::Letter(c)).unwrap_or_else(|| panic!("字母 {c} 未映射"));
            let upper = to_code(Key::Letter(c.to_ascii_uppercase()))
                .unwrap_or_else(|| panic!("大写字母 {c} 未映射"));
            assert_eq!(lower, upper, "字母 {c} 的大小写映射不一致");
        }
        for digit in 0..=9u8 {
            assert!(to_code(Key::Digit(digit)).is_some(), "数字 {digit} 未映射");
        }
        // 超出范围的取值必须如实返回 None，而不是映射到别的键。
        assert!(to_code(Key::Digit(10)).is_none());
        assert!(to_code(Key::Letter('中')).is_none());
    }

    /// F1–F12 已映射，F13 以上不支持。
    #[test]
    fn function_keys_cover_f1_to_f12_only() {
        for n in 1..=12u8 {
            assert!(to_code(Key::Function(n)).is_some(), "F{n} 应已映射");
        }
        for n in 13..=24u8 {
            assert!(to_code(Key::Function(n)).is_none(), "F{n} 不应被映射");
        }
    }

    #[test]
    fn named_keys_are_all_mapped() {
        for key in [
            Key::Space,
            Key::Enter,
            Key::Escape,
            Key::Tab,
            Key::ArrowUp,
            Key::ArrowDown,
            Key::ArrowLeft,
            Key::ArrowRight,
        ] {
            assert!(to_code(key).is_some(), "{key:?} 未映射");
        }
    }

    /// 冲突必须被分类为「被其他应用占用」，而不是笼统的失败。
    #[test]
    fn registration_failure_is_reported_as_conflict() {
        let spec = HotkeySpec::parse("Ctrl+Alt+Space").expect("可解析");
        let error = classify(
            global_hotkey::Error::FailedToRegister("RegisterEventHotKey failed".to_string()),
            &spec,
        );
        assert_eq!(
            error,
            HotkeyError::Conflict {
                spec: "Ctrl+Alt+Space".to_string()
            }
        );
    }
}
