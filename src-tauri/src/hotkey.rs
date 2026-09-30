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
    // 先注销旧的，避免重新注册时与自身冲突。
    {
        let mut hotkey = lock(&state.hotkey);
        if let Some(handle) = hotkey.handle.take() {
            let _ = state.platform.hotkeys.unregister(&handle);
        }
        hotkey.label = raw.to_string();
        hotkey.error = None;
    }

    let spec = match HotkeySpec::parse(raw) {
        Ok(spec) => spec,
        Err(error) => {
            let message = error.to_string();
            lock(&state.hotkey).error = Some(message.clone());
            push_status(app, state);
            let _ = app.emit("flashcast://hotkey-error", message);
            return;
        }
    };

    let callback: PressCallback = {
        let app = app.clone();
        Arc::new(move || {
            // 回调在非主线程执行，必须转回主线程再操作窗口。
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || summon::summon(&handle));
        })
    };

    match state.platform.hotkeys.register(&spec, callback) {
        Ok(handle) => {
            let mut hotkey = lock(&state.hotkey);
            hotkey.handle = Some(handle);
            hotkey.error = None;
        }
        Err(error) => {
            lock(&state.hotkey).error = Some(error.to_string());
        }
    }
    push_status(app, state);
}

/// 把快捷键状态推送给 UI。
pub fn push_status<R: Runtime>(app: &AppHandle<R>, state: &AppState) {
    let hotkey = lock(&state.hotkey);
    let view = HotkeyStatusView {
        label: hotkey.label.clone(),
        error: hotkey.error.clone(),
        registered: hotkey.handle.is_some(),
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
    }
}

/// 供 setup 阶段使用：注册设置中的快捷键并返回结果。
pub fn apply_from_settings<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let settings = state.host.settings();
    apply(app, &state, &settings.hotkey);
}
