//! 把平台图标文件转换为 UI 可直接 `<img src>` 的 data URL。
//!
//! 只支持 webview 能渲染的格式（PNG / SVG）；`.xpm` 图标无法在 webview 中显示，
//! 返回 `None`，由 UI 回退到内置占位图标。

use std::path::Path;

use base64::Engine;

/// 单个图标文件的大小上限。超过则视为异常，回退到占位图标。
const MAX_ICON_BYTES: u64 = 512 * 1024;

/// 读取图标并编码为 data URL。失败或格式不支持时返回 `None`。
pub fn icon_data_url(path: &Path) -> Option<String> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())?;
    let mime = match extension.as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        // webview 不能渲染 XPM。
        _ => return None,
    };
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_ICON_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}
