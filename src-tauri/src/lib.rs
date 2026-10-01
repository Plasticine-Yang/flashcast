//! Flashcast 宿主外壳。
//!
//! 外壳只负责：窗口与唤起、全局快捷键、托盘、唤起前应用身份的采集时机、
//! Tauri command 转发与事件推送。业务判断全部在 `flashcast_core::Host`。

mod commands;
mod hotkey;
mod icon;
mod state;
mod summon;
mod tray;
mod watch;

use std::sync::Arc;

use flashcast_core::{Host, HostDeps, PluginRegistry, Settings};
use tauri::{Manager, WindowEvent};

use crate::state::AppState;
use crate::summon::SUMMON_GRACE;

/// 启动应用。
pub fn run() {
    tauri::Builder::default()
        // 单实例守卫：必须最先注册。第二次启动会唤起已运行的实例。
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            summon::summon(app);
        }))
        .invoke_handler(tauri::generate_handler![
            commands::query,
            commands::execute,
            commands::move_selection,
            commands::set_selection,
            commands::back,
            commands::rescan,
            commands::refresh_state,
            commands::get_capabilities,
            commands::get_settings,
            commands::set_settings,
            commands::get_status,
            commands::get_theme,
            commands::select_theme,
            commands::set_plugin_enabled,
            commands::set_system_appearance,
            commands::install_theme,
            commands::remove_theme,
            commands::get_workspace,
            commands::select_workspace,
            commands::init_workspace,
            commands::clone_workspace,
            commands::clone_progress,
            commands::cancel_clone,
            commands::reload_workspace,
            commands::get_git_changes,
            commands::commit_changes,
            commands::get_sync_status,
            commands::redetect_sync_state,
            commands::pull_workspace,
            commands::push_workspace,
            commands::sync_progress,
            commands::cancel_sync,
            commands::memos,
            commands::memo_problems,
            commands::get_clipboard_state,
            commands::set_clipboard_paused,
            commands::set_clipboard_limits,
            commands::pin_clipboard_entry,
            commands::delete_clipboard_entry,
            commands::clear_clipboard_history,
            commands::save_clipboard_file_copy,
            commands::create_memo,
            commands::update_memo,
            commands::delete_memo,
            commands::preview,
            commands::get_chrome_state,
            commands::associate_chrome_profile,
            commands::refresh_chrome_bookmarks,
            commands::hide_window,
        ])
        .setup(|app| {
            let handle = app.handle().clone();
            let platform = flashcast_platform::current();
            let settings = Settings::default();

            // 插件注册表：ticket 01 只有注册表本身，官方插件在 ticket 07/09/13 加入。
            let plugins = Arc::new(PluginRegistry::new());
            for plugin_id in &settings.disabled_plugins {
                plugins.set_enabled(plugin_id, false);
            }

            // 设备本地数据目录（应用数据目录）：工作区之外的本机数据都放这里。
            // 解析失败时退回到系统临时目录下的固定子目录，宿主仍然可用。
            let device_dir = handle
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::env::temp_dir().join("flashcast"));
            let deps = HostDeps {
                catalog: Arc::clone(&platform.catalog),
                launcher: Arc::clone(&platform.launcher),
                capabilities: Arc::clone(&platform.capabilities),
                clipboard: Arc::clone(&platform.clipboard),
                clipboard_watcher: Arc::clone(&platform.clipboard_watcher),
                chrome: Arc::clone(&platform.chrome),
                focus: Arc::clone(&platform.focus),
                paster: Arc::clone(&platform.paster),
                plugins,
                device_dir,
            };
            let host = Arc::new(Host::new(deps, settings));
            // 随应用提供的官方功能插件（ticket 07 起：备忘录）。实现随应用编译进来，
            // 「有哪些插件、是否启用」以工作区的 manifest.json 为唯一权威。
            host.install_official_plugins();
            // 剪贴板历史默认关闭；用户启用后 `set_plugin_enabled` 会自动启动后台捕获，
            // 这里显式调用一次是为了让「清单里本来就启用」的配置在启动时也生效。
            host.start_clipboard_capture();
            app.manage(AppState::new(host, platform));

            // 托盘是 Linux 上的必需备用入口（Wayland 下快捷键注册会失败）。
            tray::create(&handle)?;
            // 注册全局快捷键；失败只会产生可展示的错误，不影响托盘入口。
            hotkey::apply_from_settings(&handle);
            // 工作区外部修改 → 重载配置 → 推送给 UI。
            watch::spawn(&handle);
            Ok(())
        })
        .on_window_event(|window, event| match event {
            // 失焦即隐藏。刚唤起后的短暂时间内忽略，避免窗口显示后被立刻隐藏。
            WindowEvent::Focused(false) => {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                if !state.is_recent_summon(SUMMON_GRACE) {
                    let _ = window.hide();
                    let _ = tauri::Emitter::emit(app, "flashcast://dismissed", ());
                }
            }
            // 关闭按钮不退出应用，只隐藏；托盘与快捷键仍可唤起。
            WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                // 用户主动关闭：作废待完成的粘贴计划。
                let state = window.app_handle().state::<AppState>();
                state.host.cancel_paste();
                let _ = window.hide();
                let _ = tauri::Emitter::emit(window.app_handle(), "flashcast://dismissed", ());
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("启动 Flashcast 失败");
}
