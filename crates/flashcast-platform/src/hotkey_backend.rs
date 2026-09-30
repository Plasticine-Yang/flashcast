//! `global-hotkey` 后端的共享实现（Linux X11 与 Windows）。
//!
//! `global-hotkey` 在 X11（`XGrabKey`）与 Windows（`RegisterHotKey`）上共用同一套
//! Rust API，注册、注销、更新与事件分发的代码在两个平台上完全一致；只有
//! 「当前会话是否允许注册」的判断不同（Linux 看 X11/Wayland，Windows 看是否有可
//! 交互桌面）。因此把这部分放在这里，两个平台各自只保留自己的准入判断与错误文案。
//!
//! 后端的错误必须被分类成面向用户的 [`HotkeyError`]：`FailedToRegister` 在两个平台
//! 上都表示「该组合已被其他应用占用」，而不是内部故障。
//!
//! 唯一的平台差异是管理器的生命周期约束（见 [`with_manager`]）：Linux 上是进程级
//! 单例，Windows 上必须留在创建它的那个线程。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::hotkey::{HotkeySpec, Key, Modifier};
use crate::shortcut::{HotkeyError, HotkeyHandle, PressCallback};

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

/// 在全局快捷键后端上执行一次操作，并统一处理「后端初始化失败」。
///
/// 两个平台的管理器生命周期约束不同，这是本文件里唯一按平台分支的地方：
///
/// - **Linux（X11）**：`GlobalHotKeyManager` 可以从任意线程注册，按键事件由后端
///   自己的线程分发，因此保存为进程级单例。
/// - **Windows**：管理器持有一个隐藏窗口的 `HWND`，因此既是 `!Send + !Sync`，也
///   意味着 `WM_HOTKEY` 只会投递到**创建该窗口的线程**的消息队列。所以它必须留在
///   调用线程（Tauri 主线程，其消息循环负责派发），用 `thread_local` 保存；并且当
///   注册发生在另一个线程时**明确报错**，而不是在无人派发消息的线程上悄悄再建一个
///   管理器——那会得到「注册成功但永远收不到按键」的假象。
#[cfg(target_os = "linux")]
fn with_manager<T>(f: impl FnOnce(&GlobalHotKeyManager) -> T) -> Result<T, HotkeyError> {
    static MANAGER: OnceLock<Result<GlobalHotKeyManager, String>> = OnceLock::new();
    let manager = MANAGER.get_or_init(|| GlobalHotKeyManager::new().map_err(|error| error.to_string()));
    match manager {
        Ok(manager) => Ok(f(manager)),
        Err(reason) => Err(backend_unavailable(reason)),
    }
}

#[cfg(target_os = "windows")]
fn with_manager<T>(f: impl FnOnce(&GlobalHotKeyManager) -> T) -> Result<T, HotkeyError> {
    use std::cell::RefCell;

    struct Slot {
        thread: std::thread::ThreadId,
        manager: Result<GlobalHotKeyManager, String>,
    }

    thread_local! {
        static MANAGER: RefCell<Option<Slot>> = const { RefCell::new(None) };
    }

    MANAGER.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(Slot {
                thread: std::thread::current().id(),
                manager: GlobalHotKeyManager::new().map_err(|error| error.to_string()),
            });
        }
        let slot = slot.as_ref().expect("上面刚写入");
        if slot.thread != std::thread::current().id() {
            return Err(HotkeyError::BackendUnavailable {
                reason: "全局快捷键只能在创建后端的线程上操作（Windows 的 WM_HOTKEY 只投递到创建\
                         后端窗口的线程）；请在应用主线程注册快捷键"
                    .to_string(),
            });
        }
        match &slot.manager {
            Ok(manager) => Ok(f(manager)),
            Err(reason) => Err(backend_unavailable(reason)),
        }
    })
}

fn backend_unavailable(reason: &str) -> HotkeyError {
    HotkeyError::BackendUnavailable {
        reason: format!("无法初始化全局快捷键后端：{reason}"),
    }
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

/// 注册 `spec`。调用方负责先做平台准入判断。
pub(crate) fn register(
    spec: &HotkeySpec,
    on_press: PressCallback,
) -> Result<HotkeyHandle, HotkeyError> {
    let hotkey = to_backend_hotkey(spec)?;
    let id = hotkey.id();

    with_manager(|manager| manager.register(hotkey))
        .and_then(|result| result.map_err(|error| classify(error, spec)))?;
    lock_callbacks().insert(id, on_press);
    ensure_listener();
    Ok(HotkeyHandle::new(u64::from(id), spec.clone(), {
        // 句柄不自持回调副本，避免重复持有；触发走全局表。
        dummy_callback(id)
    }))
}

/// 用新规格替换已注册的快捷键。失败时恢复原快捷键，避免用户彻底失去入口。
pub(crate) fn update(
    handle: &HotkeyHandle,
    spec: &HotkeySpec,
) -> Result<HotkeyHandle, HotkeyError> {
    // 保持回调不变：先取出旧回调，注销后重新注册。
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

/// 注销已注册的快捷键。重复注销不视为错误。
pub(crate) fn unregister(handle: &HotkeyHandle) -> Result<(), HotkeyError> {
    let id = handle.id as u32;
    lock_callbacks().remove(&id);
    let hotkey = to_backend_hotkey(&handle.spec)?;
    match with_manager(|manager| manager.unregister(hotkey)) {
        Ok(Ok(())) => Ok(()),
        // 已经不在注册表中不视为错误。
        Ok(Err(global_hotkey::Error::FailedToUnRegister(_))) => Ok(()),
        Ok(Err(error)) => Err(HotkeyError::Other {
            reason: error.to_string(),
        }),
        // 后端不可初始化（或不是创建它的线程）时也视为已注销：清理路径不应报错。
        Err(_) => Ok(()),
    }
}

/// 把内部规格转换为后端快捷键。
pub(crate) fn to_backend_hotkey(spec: &HotkeySpec) -> Result<HotKey, HotkeyError> {
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

/// 内部主键到后端键码的映射。`None` 表示后端没有对应键位。
pub(crate) fn to_code(key: Key) -> Option<Code> {
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
pub(crate) fn classify(error: global_hotkey::Error, spec: &HotkeySpec) -> HotkeyError {
    match error {
        // X11 下 XGrabKey 返回 BadAccess、Windows 下 RegisterHotKey 失败都走这里：
        // 快捷键已被其他应用占用。
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

    /// 每个内部主键都必须映射到后端键码，且字母键大小写不敏感。
    #[test]
    fn every_supported_key_maps_to_a_backend_code() {
        for c in 'a'..='z' {
            assert!(
                to_code(Key::Letter(c)).is_some(),
                "小写字母 {c} 必须有映射"
            );
            assert_eq!(
                to_code(Key::Letter(c)),
                to_code(Key::Letter(c.to_ascii_uppercase())),
                "字母 {c} 的大小写必须映射到同一键码"
            );
        }
        for d in 0..=9u8 {
            assert!(to_code(Key::Digit(d)).is_some(), "数字 {d} 必须有映射");
        }
        for n in 1..=12u8 {
            assert!(to_code(Key::Function(n)).is_some(), "F{n} 必须有映射");
        }
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
            assert!(to_code(key).is_some(), "{} 必须有映射", key.canonical());
        }
        // 明确的「后端不支持」：F13 与非法数字不静默映射到别的键。
        assert_eq!(to_code(Key::Function(13)), None);
        assert_eq!(to_code(Key::Digit(10)), None);
        assert_eq!(to_code(Key::Letter('中')), None);
    }

    /// 注册失败必须区分「被占用」与「后端不可用」：前者用户改键即可解决。
    #[test]
    fn backend_errors_are_classified_for_the_user() {
        let spec = HotkeySpec::parse("Ctrl+Alt+Space").expect("规格可解析");
        assert_eq!(
            classify(global_hotkey::Error::FailedToRegister("x".to_string()), &spec),
            HotkeyError::Conflict {
                spec: "Ctrl+Alt+Space".to_string()
            }
        );
        assert_eq!(
            classify(
                global_hotkey::Error::AlreadyRegistered(HotKey::new(
                    Some(Modifiers::CONTROL),
                    Code::KeyA
                )),
                &spec
            ),
            HotkeyError::AlreadyRegistered {
                spec: "Ctrl+Alt+Space".to_string()
            }
        );
        let io_error = std::io::Error::new(std::io::ErrorKind::Other, "nope");
        match classify(global_hotkey::Error::OsError(io_error), &spec) {
            HotkeyError::BackendUnavailable { reason } => assert!(reason.contains("nope")),
            other => panic!("OsError 应归类为后端不可用，实际为 {other:?}"),
        }
    }

    /// 规格到后端的转换保留修饰键，并且拒绝没有修饰键的规格。
    #[test]
    fn backend_hotkey_conversion_keeps_modifiers() {
        let spec = HotkeySpec::parse("Ctrl+Shift+K").expect("规格可解析");
        let hotkey = to_backend_hotkey(&spec).expect("转换成功");
        assert!(hotkey.mods.contains(Modifiers::CONTROL));
        assert!(hotkey.mods.contains(Modifiers::SHIFT));
        assert_eq!(hotkey.key, Code::KeyK);

        let bare = HotkeySpec {
            raw: "K".to_string(),
            modifiers: Vec::new(),
            key: Key::Letter('k'),
        };
        assert!(matches!(
            to_backend_hotkey(&bare),
            Err(HotkeyError::InvalidSpec(_))
        ));
    }
}
