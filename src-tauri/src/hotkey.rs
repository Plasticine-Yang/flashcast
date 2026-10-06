//! 全局快捷键注册。注册失败（快捷键被占用、会话不支持）必须作为可展示的
//! 错误推送给 UI；托盘入口始终可用，因此注册失败不会让用户失去打开方式。

use std::sync::Arc;

use flashcast_platform::hotkey::HotkeySpec;
use flashcast_platform::shortcut::PressCallback;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::commands::HotkeyStatusView;
use crate::state::{lock, AppState};
use crate::summon;

/// 注册（或重新注册）全局快捷键，并把结果推送给 UI。
pub fn apply<R: Runtime>(app: &AppHandle<R>, state: &AppState, raw: &str) {
    #[cfg(target_os = "linux")]
    if flashcast_platform::linux::detect_session_type() == flashcast_platform::SessionType::Wayland
    {
        let generation = {
            let mut hotkey = lock(&state.hotkey);
            hotkey.generation += 1;
            hotkey.error = None;
            hotkey.pending = true;
            if hotkey.handle.is_none() {
                hotkey.label = raw.to_string();
            }
            hotkey.generation
        };
        push_status(app, state);
        let app = app.clone();
        let raw = raw.to_string();
        // BindShortcuts 会等待用户操作；不能堵住 GTK 主事件循环。
        std::thread::spawn(move || {
            let state = app.state::<AppState>();
            let _registration = lock(&state.hotkey_registration);
            if lock(&state.hotkey).generation != generation {
                return;
            }
            let result = HotkeySpec::parse(&raw)
                .map_err(Into::into)
                .and_then(|spec| state.platform.hotkeys.register(&spec, callback(&app)));
            let mut hotkey = lock(&state.hotkey);
            if hotkey.generation != generation {
                drop(hotkey);
                if let Ok(handle) = result {
                    let _ = state.platform.hotkeys.unregister(&handle);
                }
                return;
            }
            hotkey.pending = false;
            match result {
                Ok(handle) => {
                    hotkey.label = handle.trigger_description.clone().unwrap_or(raw);
                    let previous = hotkey.handle.replace(handle);
                    hotkey.error = None;
                    drop(hotkey);
                    if let Some(previous) = previous {
                        let _ = state.platform.hotkeys.unregister(&previous);
                    }
                }
                Err(error) => {
                    if hotkey.handle.is_none() {
                        hotkey.label = raw;
                    }
                    hotkey.error = Some(error.to_string());
                    drop(hotkey);
                }
            }
            push_status(&app, &state);
        });
        return;
    }
    // 普通后端统一在主线程操作。尤其不能从文件监听线程创建 Windows HWND。
    let generation = {
        let mut hotkey = lock(&state.hotkey);
        hotkey.generation += 1;
        hotkey.pending = true;
        if hotkey.handle.is_none() {
            hotkey.label = raw.to_string();
        }
        hotkey.generation
    };
    push_status(app, state);
    let handle = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        let state = handle.state::<AppState>();
        if lock(&state.hotkey).generation != generation {
            return;
        }
        // 请求排队期间设置可能又有变化，始终应用最新的宿主配置。
        let raw = state.host.settings().hotkey;
        apply_sync(&handle, &state, &raw);
    }) {
        let mut hotkey = lock(&state.hotkey);
        if hotkey.generation == generation {
            hotkey.pending = false;
            hotkey.error = Some(format!("无法在主线程注册快捷键：{error}"));
        }
        drop(hotkey);
        push_status(app, state);
    }
}

fn callback<R: Runtime>(app: &AppHandle<R>) -> PressCallback {
    let app = app.clone();
    Arc::new(move |activation| {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            summon::summon_with_activation(&handle, activation.token.as_deref());
        });
    })
}

fn apply_sync<R: Runtime>(app: &AppHandle<R>, state: &AppState, raw: &str) {
    let _registration = lock(&state.hotkey_registration);
    let spec = match HotkeySpec::parse(raw) {
        Ok(spec) => spec,
        Err(error) => {
            let message = error.to_string();
            let mut hotkey = lock(&state.hotkey);
            hotkey.pending = false;
            hotkey.error = Some(message.clone());
            drop(hotkey);
            push_status(app, state);
            let _ = app.emit("flashcast://hotkey-error", message);
            return;
        }
    };

    let previous = lock(&state.hotkey).handle.clone();
    let result = match previous.as_ref() {
        Some(handle) => state.platform.hotkeys.update(handle, &spec),
        None => state.platform.hotkeys.register(&spec, callback(app)),
    };
    let mut hotkey = lock(&state.hotkey);
    hotkey.pending = false;
    match result {
        Ok(handle) => {
            hotkey.label = handle
                .trigger_description
                .clone()
                .unwrap_or_else(|| spec.canonical());
            hotkey.handle = Some(handle);
            hotkey.error = None;
        }
        Err(error) => {
            if hotkey.handle.is_none() {
                hotkey.label = raw.to_string();
            }
            hotkey.error = Some(error.to_string());
        }
    }
    drop(hotkey);
    push_status(app, state);
}

/// 把快捷键状态推送给 UI。
pub fn push_status<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let hotkey = lock(&state.hotkey);
    let view = HotkeyStatusView {
        label: hotkey.label.clone(),
        error: hotkey.error.clone(),
        registered: hotkey.handle.is_some(),
        pending: hotkey.pending,
    };
    drop(hotkey);
    let _ = app.emit("flashcast://hotkey-status", view);
}

/// 读取当前快捷键状态。
pub fn status(state: &AppState) -> HotkeyStatusView {
    let hotkey = lock(&state.hotkey);
    HotkeyStatusView {
        label: hotkey.label.clone(),
        error: hotkey.error.clone(),
        registered: hotkey.handle.is_some(),
        pending: hotkey.pending,
    }
}

/// 供 setup 阶段使用：注册设置中的快捷键并返回结果。
pub fn apply_from_settings<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let settings = state.host.settings();
    apply(app, &state, &settings.hotkey);
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandStatusView {
    #[serde(flatten)]
    pub command: flashcast_core::PluginCommandView,
    pub registered: bool,
    pub pending: bool,
    pub effective_shortcut: Option<String>,
    pub error: Option<String>,
}

pub fn command_status(state: &AppState) -> Vec<CommandStatusView> {
    let commands = state.host.plugin_commands();
    let registrations = lock(&state.command_hotkeys);
    commands
        .into_iter()
        .map(|command| {
            let live = registrations.get(&command.id);
            CommandStatusView {
                registered: live.is_some_and(|r| r.enabled && r.hotkey.handle.is_some()),
                pending: live.is_some_and(|r| r.hotkey.pending),
                effective_shortcut: live
                    .filter(|r| r.hotkey.handle.is_some())
                    .map(|r| r.hotkey.label.clone()),
                error: live.and_then(|r| r.hotkey.error.clone()),
                command,
            }
        })
        .collect()
}

pub fn sync_commands<R: Runtime>(app: &AppHandle<R>) {
    // Windows 的 RegisterHotKey 绑定调用线程，所有非门户注册必须回到主线程。
    let app = app.clone();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let state = handle.state::<AppState>();
        for command in state.host.plugin_commands() {
            let enabled = command.enabled && !command.shortcut.is_empty();
            let generation = {
                let mut all = lock(&state.command_hotkeys);
                let entry = all.entry(command.id.clone()).or_default();
                if entry.enabled == enabled && entry.requested == command.shortcut {
                    continue;
                }
                entry.enabled = enabled;
                entry.requested = command.shortcut.clone();
                entry.hotkey.generation += 1;
                entry.hotkey.pending = enabled;
                entry.hotkey.error = None;
                if !enabled {
                    if let Some(old) = entry.hotkey.handle.as_ref() {
                        match state.platform.hotkeys.unregister(old) {
                            Ok(()) => entry.hotkey.handle = None,
                            Err(error) => entry.hotkey.error = Some(error.to_string()),
                        }
                    }
                }
                entry.hotkey.generation
            };
            if !enabled {
                continue;
            }
            let app = handle.clone();
            #[cfg(target_os = "linux")]
            if flashcast_platform::linux::detect_session_type()
                == flashcast_platform::SessionType::Wayland
            {
                std::thread::spawn(move || register_command(&app, command, generation));
                continue;
            }
            register_command(&app, command, generation);
        }
        let _ = handle.emit("flashcast://plugin-commands", command_status(&state));
    });
}

fn register_command<R: Runtime>(
    app: &AppHandle<R>,
    command: flashcast_core::PluginCommandView,
    generation: u64,
) {
    let state = app.state::<AppState>();
    let _registration = lock(&state.hotkey_registration);
    if !lock(&state.command_hotkeys)
        .get(&command.id)
        .is_some_and(|r| r.enabled && r.hotkey.generation == generation)
    {
        return;
    }
    let callback_app = app.clone();
    let id = command.id.clone();
    let binding = command.shortcut.clone();
    let callback: PressCallback = Arc::new(move |activation| {
        let app = callback_app.clone();
        let id = id.clone();
        let binding = binding.clone();
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let state = handle.state::<AppState>();
            if !lock(&state.command_hotkeys).get(&id).is_some_and(|r| {
                r.enabled
                    && r.hotkey.handle.as_ref().is_some_and(|h| {
                        h.spec.canonical()
                            == HotkeySpec::parse(&binding)
                                .map(|s| s.canonical())
                                .unwrap_or_default()
                    })
            }) {
                return;
            }
            summon::summon_command(&handle, activation.token.as_deref(), &id);
        });
    });
    let previous = lock(&state.command_hotkeys)
        .get(&command.id)
        .and_then(|entry| entry.hotkey.handle.clone());
    let result = HotkeySpec::parse(&command.shortcut)
        .map_err(Into::into)
        .and_then(|spec| {
            if let Some(old) = previous.as_ref() {
                if old.spec.canonical() == spec.canonical() {
                    return Ok(old.clone());
                }
            }
            let new_handle = state.platform.hotkeys.register_command(
                &spec,
                &command.id,
                &command.title,
                callback,
            )?;
            // 门户成功后在下面交换句柄；普通后端不能吞掉旧组合的注销失败。
            #[cfg(target_os = "linux")]
            let portal = flashcast_platform::linux::detect_session_type()
                == flashcast_platform::SessionType::Wayland;
            #[cfg(not(target_os = "linux"))]
            let portal = false;
            if !portal {
                if let Some(old) = previous.as_ref() {
                    if let Err(error) = state.platform.hotkeys.unregister(old) {
                        state.platform.hotkeys.unregister(&new_handle)?;
                        return Err(error);
                    }
                }
            }
            Ok(new_handle)
        });
    let mut all = lock(&state.command_hotkeys);
    let Some(entry) = all
        .get_mut(&command.id)
        .filter(|r| r.enabled && r.hotkey.generation == generation)
    else {
        drop(all);
        if let Ok(handle) = result {
            let _ = state.platform.hotkeys.unregister(&handle);
        }
        return;
    };
    entry.hotkey.pending = false;
    match result {
        Ok(handle) => {
            entry.hotkey.label = handle
                .trigger_description
                .clone()
                .unwrap_or(command.shortcut);
            let unchanged = entry
                .hotkey
                .handle
                .as_ref()
                .is_some_and(|old| old.id == handle.id);
            let old = entry.hotkey.handle.replace(handle);
            drop(all);
            if let Some(old) = old.filter(|_| !unchanged) {
                let _ = state.platform.hotkeys.unregister(&old);
            }
        }
        Err(error) => {
            entry.hotkey.error = Some(error.to_string());
            drop(all);
        }
    }
    let _ = app.emit("flashcast://plugin-commands", command_status(&state));
}
