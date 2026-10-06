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

/// 缩略图的长边像素上限（结果列表里的图片历史）。
///
/// 这个尺寸在 640px 宽、系统缩放下仍然清晰，编码后的 PNG 通常只有几 KB：
/// 一次查询返回几十条结果也不会把几十 MB 的字符串送进 webview。
pub const THUMBNAIL_MAX_EDGE: u32 = 96;

/// 原始图片预览的大小上限（按需展开的完整图片）。
///
/// 与平台层的单张图片上限一致：超过这个大小的历史根本不会被保存，因此这里只是
/// 兜住「附件被外部替换成超大文件」的情况，避免一次 IPC 拖垮 UI。
const MAX_PREVIEW_BYTES: u64 = 16 * 1024 * 1024;

/// 图片附件 → 结果列表用的缩略图 data URL。
///
/// 解码失败、文件不存在或超过上限时返回 `None`，UI 回退到占位图标：
/// 一条读不出来的历史不应该让整个列表崩塌。
pub fn thumbnail_data_url(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let thumbnail =
        flashcast_platform::clipboard::png_thumbnail(&bytes, THUMBNAIL_MAX_EDGE).ok()?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(thumbnail);
    Some(format!("data:image/png;base64,{encoded}"))
}

/// 图片附件 → 完整预览用的 data URL（按需展开时才调用）。
pub fn image_data_url(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_PREVIEW_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let mime = match path
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        _ => "image/png",
    };
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    Some(format!("data:{mime};base64,{encoded}"))
}
