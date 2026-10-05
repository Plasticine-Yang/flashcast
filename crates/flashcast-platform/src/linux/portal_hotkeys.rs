//! XDG GlobalShortcuts：会话一直存活到注销，拒绝/超时不会伪报注册成功。
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ashpd::desktop::global_shortcuts::{GlobalShortcuts, NewShortcut};
use ashpd::desktop::Session;
use futures_util::StreamExt;
use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

use crate::hotkey::{HotkeySpec, Key, Modifier};
use crate::shortcut::{HotkeyActivation, HotkeyError, HotkeyHandle, PressCallback};

const APP_ID: &str = "dev.flashcast.launcher";
const SUMMON: &str = "summon";
const BUS_TIMEOUT: Duration = Duration::from_secs(5);
const CONSENT_TIMEOUT: Duration = Duration::from_secs(120);

struct Registration {
    session: Session<GlobalShortcuts>,
    listener: JoinHandle<()>,
    active: Arc<AtomicBool>,
}

fn runtime() -> Result<&'static Runtime, HotkeyError> {
    static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(unavailable)
}

fn registrations() -> &'static Mutex<HashMap<u64, Registration>> {
    static REGISTRATIONS: OnceLock<Mutex<HashMap<u64, Registration>>> = OnceLock::new();
    REGISTRATIONS.get_or_init(Default::default)
}

fn unavailable(error: impl std::fmt::Display) -> HotkeyError {
    HotkeyError::BackendUnavailable {
        reason: format!("Wayland 快捷键门户不可用或未获授权：{error}。可从托盘打开 Flashcast，或在系统设置中添加命令 flashcast 的自定义快捷键"),
    }
}

/// 只读检查门户接口；有接口不等于用户已经授权或按键唤起已验证。
pub fn available() -> Result<(), HotkeyError> {
    runtime()?.block_on(async {
        tokio::time::timeout(BUS_TIMEOUT, async {
            let portal = GlobalShortcuts::new().await.map_err(unavailable)?;
            portal
                .get_property::<u32>("version")
                .await
                .map_err(unavailable)?;
            Ok(())
        })
        .await
        .map_err(|_| unavailable("检测超时"))?
    })
}

/// 规格采用 XDG shortcuts 格式，而不是 GTK 的 <Control><Alt> 写法。
fn trigger(spec: &HotkeySpec) -> String {
    let mut parts: Vec<String> = spec
        .modifiers
        .iter()
        .map(|m| {
            match m {
                Modifier::Control => "CTRL",
                Modifier::Alt => "ALT",
                Modifier::Shift => "SHIFT",
                Modifier::Super => "LOGO",
            }
            .to_string()
        })
        .collect();
    parts.push(match spec.key {
        Key::Space => "space".to_string(),
        Key::Enter => "Return".to_string(),
        Key::Escape => "Escape".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::ArrowUp => "Up".to_string(),
        Key::ArrowDown => "Down".to_string(),
        Key::ArrowLeft => "Left".to_string(),
        Key::ArrowRight => "Right".to_string(),
        Key::Letter(c) => c.to_ascii_lowercase().to_string(),
        Key::Digit(n) => n.to_string(),
        Key::Function(n) => format!("F{n}"),
    });
    parts.join("+")
}

/// 非沙箱应用必须在同一连接的首次门户调用前声明身份。旧门户没有 Registry
/// 时交给其自动识别机制；拒绝授权等其它错误必须如实返回。
async fn connect() -> Result<GlobalShortcuts, HotkeyError> {
    let connection = ashpd::zbus::Connection::session()
        .await
        .map_err(unavailable)?;
    ensure_desktop_entry()?;
    if let Err(error) = ashpd::register_host_app_with_connection(
        connection.clone(),
        APP_ID.parse().expect("固定应用 ID 合法"),
    )
    .await
    {
        match &error {
            ashpd::Error::PortalNotFound(_) => {}
            ashpd::Error::Zbus(ashpd::zbus::Error::MethodError(name, _, _))
                if matches!(
                    name.as_str(),
                    "org.freedesktop.DBus.Error.UnknownInterface"
                        | "org.freedesktop.DBus.Error.UnknownMethod"
                ) => {}
            _ => return Err(unavailable(error)),
        }
    }
    GlobalShortcuts::with_connection(connection)
        .await
        .map_err(unavailable)
}

/// deb 随包提供身份文件；开发运行及便携 AppImage 按需添加不显示在菜单的身份。
fn ensure_desktop_entry() -> Result<(), HotkeyError> {
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".local/share"))
        })
        .ok_or_else(|| unavailable("无法找到用户应用目录"))?;
    let mut dirs = vec![data_home.clone()];
    dirs.extend(std::env::split_paths(
        &std::env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into()),
    ));
    let filename = format!("applications/{APP_ID}.desktop");
    if dirs.iter().any(|d| d.join(&filename).is_file()) {
        return Ok(());
    }
    let running = std::env::current_exe().map_err(unavailable)?;
    let executable = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            if running.file_name().and_then(|n| n.to_str()) == Some("flashcast") {
                running
            } else {
                std::path::PathBuf::from("flashcast")
            }
        });
    let executable = executable
        .to_str()
        .ok_or_else(|| unavailable("程序路径不是 UTF-8"))?;
    if executable.contains(['\n', '\r']) {
        return Err(unavailable("程序路径包含换行"));
    }
    let exec = executable
        .replace('\\', "\\\\\\\\")
        .replace('"', "\\\\\\\"")
        .replace('`', "\\\\`")
        .replace('$', "\\\\$")
        .replace('%', "%%");
    let target = data_home.join(filename);
    std::fs::create_dir_all(target.parent().expect("应用目录有父路径")).map_err(unavailable)?;
    std::fs::write(target, format!("[Desktop Entry]\nType=Application\nName=Flashcast\nExec=\"{exec}\"\nIcon=flashcast\nNoDisplay=true\nTerminal=false\n")).map_err(unavailable)
}

pub fn register(spec: &HotkeySpec, on_press: PressCallback) -> Result<HotkeyHandle, HotkeyError> {
    let rt = runtime()?;
    let (session, mut activated, description) = rt.block_on(async {
        let portal = tokio::time::timeout(BUS_TIMEOUT, connect())
            .await
            .map_err(|_| unavailable("连接超时"))??;
        let session = tokio::time::timeout(BUS_TIMEOUT, portal.create_session(Default::default()))
            .await
            .map_err(|_| unavailable("创建会话超时"))?
            .map_err(unavailable)?;
        let binding = async {
            // 先订阅，再请求绑定，避免授权完成后的首次按键丢失。
            let activated = portal.receive_activated().await.map_err(unavailable)?;
            let preferred = trigger(spec);
            let shortcut =
                NewShortcut::new(SUMMON, "打开 Flashcast").preferred_trigger(preferred.as_str());
            let response = portal
                .bind_shortcuts(&session, &[shortcut], None, Default::default())
                .await
                .map_err(unavailable)?
                .response()
                .map_err(unavailable)?;
            let bound = response
                .shortcuts()
                .iter()
                .find(|s| s.id() == SUMMON)
                .ok_or_else(|| unavailable("系统未绑定打开 Flashcast 的快捷键"))?;
            if bound.trigger_description().is_empty() {
                return Err(unavailable(
                    "系统未分配按键，请在系统快捷键设置中配置打开 Flashcast",
                ));
            }
            Ok((activated, bound.trigger_description().to_string()))
        };
        match tokio::time::timeout(CONSENT_TIMEOUT, binding).await {
            Ok(Ok((activated, description))) => Ok((session, activated, description)),
            result => {
                let _ = tokio::time::timeout(BUS_TIMEOUT, session.close()).await;
                Err(match result {
                    Ok(Err(error)) => error,
                    _ => unavailable("等待系统快捷键授权超时，请重试"),
                })
            }
        }
    })?;
    let session_path = serde_json::to_value(&session).map_err(unavailable)?;
    let active = Arc::new(AtomicBool::new(true));
    let active_listener = active.clone();
    let callback = on_press.clone();
    let listener = rt.spawn(async move {
        while let Some(event) = activated.next().await {
            if !active_listener.load(Ordering::Acquire) {
                break;
            }
            if event.shortcut_id() == SUMMON
                && Some(event.session_handle().as_str()) == session_path.as_str()
            {
                let token = event
                    .options()
                    .get("activation_token")
                    .and_then(|value| <&str>::try_from(value).ok())
                    .map(ToOwned::to_owned);
                callback(HotkeyActivation { token });
            }
        }
    });
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    registrations()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(
            id,
            Registration {
                session,
                listener,
                active,
            },
        );
    let mut handle = HotkeyHandle::new(id, spec.clone(), on_press);
    handle.trigger_description = Some(description);
    Ok(handle)
}

/// 先获取新授权，失败保留原入口；成功后再关闭旧会话。
pub fn update(handle: &HotkeyHandle, spec: &HotkeySpec) -> Result<HotkeyHandle, HotkeyError> {
    let new = register(spec, handle.callback.clone())?;
    if let Err(error) = unregister(handle) {
        let _ = unregister(&new);
        return Err(error);
    }
    Ok(new)
}

pub fn unregister(handle: &HotkeyHandle) -> Result<(), HotkeyError> {
    let registration = registrations()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(&handle.id);
    if let Some(registration) = registration {
        registration.active.store(false, Ordering::Release);
        registration.listener.abort();
        runtime()?.block_on(async {
            tokio::time::timeout(BUS_TIMEOUT, registration.session.close())
                .await
                .map_err(|_| unavailable("关闭会话超时"))?
                .map_err(unavailable)
        })?;
    }
    Ok(())
}
