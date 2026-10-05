//! 原生窗口材质边界。Linux 或应用失败时由前端使用不透明表面。
use flashcast_core::SurfaceRenderer;
use serde::Serialize;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialStatus {
    pub supported: bool,
    pub reason: Option<String>,
}

pub fn apply(app: &tauri::AppHandle, renderer: SurfaceRenderer) -> MaterialStatus {
    let Some(window) = app.get_webview_window("search") else {
        return MaterialStatus {
            supported: false,
            reason: Some("窗口暂不可用，使用实底".into()),
        };
    };
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use tauri::window::{Effect, EffectsBuilder};
        let effects = if renderer == SurfaceRenderer::Solid {
            None
        } else {
            #[cfg(target_os = "macos")]
            let effect = Effect::HudWindow;
            #[cfg(target_os = "windows")]
            let effect = Effect::Acrylic;
            Some(EffectsBuilder::new().effect(effect).build())
        };
        return match window.set_effects(effects) {
            Ok(()) => MaterialStatus {
                supported: true,
                reason: None,
            },
            Err(_) => MaterialStatus {
                supported: false,
                reason: Some("当前系统无法启用透明材质，已使用实底；保留所选风格".into()),
            },
        };
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = window;
        MaterialStatus {
            supported: renderer == SurfaceRenderer::Solid,
            reason: (renderer != SurfaceRenderer::Solid)
                .then(|| "当前平台使用实底；所选透明风格会在支持的设备上恢复".into()),
        }
    }
}
