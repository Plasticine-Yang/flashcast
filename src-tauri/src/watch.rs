//! 工作区文件监听的后台线程。
//!
//! `Host` 提供带超时的阻塞入口 `wait_for_workspace_change`；这里在独立线程里循环
//! 调用它，把结果推送给 UI。业务判断（配置是否有效、保留哪一份设置）全部在宿主里，
//! 这里只做转发。
//!
//! 线程在应用生命周期内常驻；进程退出即结束，因此不做额外的停止握手。

use std::time::Duration;

use flashcast_core::{Settings, WorkspaceReload, WorkspaceStatus};
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::state::AppState;

/// 等待一次变更的时间片。足够短，使退出与工作区切换不会卡住。
const SLICE: Duration = Duration::from_millis(400);

/// 推送给 UI 的工作区事件。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEvent {
    pub status: WorkspaceStatus,
    pub settings: Settings,
    /// 触发本次事件的变更；`None` 表示由 UI 主动请求的刷新。
    pub reload: Option<WorkspaceReload>,
}

/// 启动监听线程。
pub fn spawn<R: Runtime>(app: &AppHandle<R>) {
    let app = app.clone();
    std::thread::spawn(move || {
        // 记录已注册的快捷键，外部修改设置后需要重新注册才能「立即生效」。
        let mut registered_hotkey = app.state::<AppState>().host.settings().hotkey;
        loop {
            let state = app.state::<AppState>();
            let reload = state.host.wait_for_workspace_change(SLICE);
            let Some(reload) = reload else { continue };
            let settings = state.host.settings();
            if settings.hotkey != registered_hotkey {
                // 外部把 settings.toml 的快捷键改掉后，外壳必须重新注册才会生效。
                registered_hotkey = settings.hotkey.clone();
                crate::hotkey::apply(&app, &state, &settings.hotkey);
            }
            let payload = WorkspaceEvent {
                status: state.host.workspace_status(),
                settings,
                reload: Some(reload),
            };
            drop(state);
            let _ = app.emit("flashcast://workspace", payload);
        }
    });
}
