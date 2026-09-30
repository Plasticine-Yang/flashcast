//! 窗口唤起与隐藏。这是外壳职责：只做窗口、焦点与事件推送，
//! 不解释业务结果。

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

/// 刚唤起后的保护窗口：在此期间忽略失焦事件，避免窗口刚显示就被立刻隐藏。
pub const SUMMON_GRACE: Duration = Duration::from_millis(250);

/// 搜索窗口的标签。
pub const SEARCH_WINDOW: &str = "search";

/// 唤起：先记录唤起前的前台应用，再显示并聚焦窗口，最后通知 UI 聚焦输入框。
pub fn summon<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let previous = match state.capture_previous_app() {
        Ok(previous) => Some(previous),
        Err(reason) => {
            // Wayland 等会话下拿不到焦点窗口：如实推送给 UI，不伪造身份。
            let _ = app.emit("flashcast://focus-unavailable", reason);
            None
        }
    };
    state.mark_summoned();

    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = window.center();
        let _ = window.show();
        let _ = window.set_focus();
    }
    let _ = app.emit(
        "flashcast://summoned",
        SummonedPayload {
            previous_app: previous,
        },
    );
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SummonedPayload {
    pub previous_app: Option<flashcast_platform::FocusedApp>,
}

/// 隐藏搜索窗口。
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = window.hide();
    }
}

/// 切换显示状态，供托盘左键使用。
pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        if window.is_visible().unwrap_or(false) {
            let _ = window.hide();
            let _ = app.emit("flashcast://dismissed", ());
            return;
        }
    }
    summon(app);
}
