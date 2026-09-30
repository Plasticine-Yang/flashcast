//! 托盘图标。在 Linux 上这是**必需的备用入口**：Wayland 会话下全局快捷键
//! 无法注册，托盘是唯一可靠的打开方式。
//!
//! Linux 不支持「左键弹出菜单」，因此菜单走右键，左键由
//! `on_tray_icon_event` 自行处理为显示/隐藏。

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Runtime};

use crate::summon;

/// 创建托盘图标与菜单。
pub fn create<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "打开 Flashcast", true, None::<&str>)?;
    let rescan = MenuItem::with_id(app, "rescan", "重新扫描软件", true, None::<&str>)?;
    let hide = MenuItem::with_id(app, "hide", "隐藏窗口", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出 Flashcast", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &rescan, &separator, &hide, &quit])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Flashcast")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => summon::summon(app),
            "rescan" => crate::commands::rescan_and_push(app),
            "hide" => {
                summon::hide(app);
                let _ = tauri::Emitter::emit(app, "flashcast://dismissed", ());
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                summon::toggle(tray.app_handle());
            }
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        builder = builder.icon(icon);
    }
    builder.build(app)?;
    Ok(())
}
