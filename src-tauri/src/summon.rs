//! 窗口唤起与隐藏。这是外壳职责：只做窗口、焦点与事件推送，
//! 不解释业务结果。

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

/// 刚唤起后的保护窗口：在此期间忽略失焦事件，避免窗口刚显示就被立刻隐藏。
pub const SUMMON_GRACE: Duration = Duration::from_millis(250);

/// 关闭浮窗与注入粘贴之间的等待。
///
/// 隐藏窗口只是发起请求：窗口管理器把焦点交还给原来的应用需要一点时间（X11 还要一次
/// 往返，Windows/macOS 的激活也是异步的）。研究笔记给出的经验区间是 50–150 ms；
/// 这里取上限，随后宿主还会回读前台核对，核对不通过就退回手动粘贴。
pub const PASTE_SETTLE: Duration = Duration::from_millis(150);

/// 搜索窗口的标签。
pub const SEARCH_WINDOW: &str = "search";

/// 唤起：先记录唤起前的前台应用，再显示并聚焦窗口，最后通知 UI 聚焦输入框。
pub fn summon<R: Runtime>(app: &AppHandle<R>) {
    summon_with_activation(app, None);
}

/// 门户签发的激活令牌随这次按键传入，不写环境变量或持久化。
pub fn summon_with_activation<R: Runtime>(app: &AppHandle<R>, token: Option<&str>) {
    let state = app.state::<AppState>();
    let previous = match state.capture_previous_app() {
        Ok(previous) => Some(previous),
        Err(reason) => {
            // Wayland 等会话下拿不到焦点窗口：如实推送给 UI，不伪造身份。
            let _ = app.emit("flashcast://focus-unavailable", reason);
            None
        }
    };
    // 把唤起前的应用交给宿主作为**本次**粘贴目标：每次都覆盖，并作废上一次未完成的
    // 粘贴计划。拿不到就交给它 `None`，绝不使用上一次唤起的旧身份。
    state.host.set_paste_target(previous.clone());
    state.mark_summoned();

    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = window.center();
        #[cfg(target_os = "linux")]
        if let (Some(token), Ok(native)) = (token, window.gtk_window()) {
            use gtk::prelude::GtkWindowExt;
            native.set_startup_id(token);
        }
        #[cfg(not(target_os = "linux"))]
        let _ = token;
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
///
/// 只做窗口操作：自动粘贴流程**必须**用它（不能顺手作废粘贴计划），因为关窗正是粘贴
/// 流程的一步。用户主动关闭浮窗请用 [`hide_and_cancel`]。
pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = window.hide();
    }
}

/// 用户主动关闭浮窗（Escape、托盘、关闭按钮）：隐藏窗口并作废待完成的粘贴计划。
///
/// 「快速关闭」之后即使有一次迟到的完成请求，也不会有任何按键被注入。
pub fn hide_and_cancel<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    state.host.cancel_paste();
    hide(app);
}

/// 重新显示搜索窗口：只显示并聚焦，**不**重新采集唤起前的应用。
///
/// 用在「关窗之后才发现无法自动粘贴」的降级路径：必须让用户看到提示，但绝不能把当前
/// 前台（很可能正是目标应用）记成新的粘贴目标。
pub fn reshow<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// 切换显示状态，供托盘左键使用。
pub fn toggle<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(SEARCH_WINDOW) {
        if window.is_visible().unwrap_or(false) {
            hide_and_cancel(app);
            let _ = app.emit("flashcast://dismissed", ());
            return;
        }
    }
    summon(app);
}
