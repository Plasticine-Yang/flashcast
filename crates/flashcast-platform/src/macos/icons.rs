//! macOS 图标：`NSWorkspace` → `NSImage` → `TIFFRepresentation` →
//! `NSBitmapImageRep` → PNG。
//!
//! **不解析 `.icns` 作为主路径**：现代应用把图标放在资源目录
//! （`Contents/Resources/Assets.car`）里，磁盘上根本没有可用的 `.icns`，
//! 系统自带的 Finder / Safari / 系统设置都是这样。`NSWorkspace::iconForFile`
//! 是 Finder 自己用的路径，Asset Catalog、`.icns`、以及任何自定义图标都能覆盖。
//!
//! 缓存文件名与渲染逻辑分开：文件名由应用包路径决定（纯函数，可在 Linux 上
//! 测试），渲染才需要 macOS API。

use std::path::{Path, PathBuf};

/// 渲染图标的目标边长（点）。UI 结果列表用的尺寸，再大只是浪费内存。
pub const ICON_POINT_SIZE: f64 = 128.0;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IconError {
    #[error("应用包路径不是有效的 UTF-8：{0}")]
    NonUtf8Path(String),
    #[error("NSWorkspace 未返回图标")]
    NoIcon,
    #[error("NSImage 没有可用的 TIFF 表示")]
    NoTiff,
    #[error("无法从 TIFF 数据构造 NSBitmapImageRep")]
    NoBitmap,
    #[error("无法把位图编码为 PNG")]
    NoPng,
    #[error("无法创建图标缓存目录 {dir}：{reason}")]
    CacheDir { dir: String, reason: String },
    #[error("写入图标缓存 {path} 失败：{reason}")]
    CacheWrite { path: String, reason: String },
}

/// 图标缓存目录：`~/Library/Caches/Flashcast/icons`。
///
/// 可用 `FLASHCAST_ICON_CACHE_DIR` 覆盖（诊断与测试用）；没有 HOME 时回退到
/// 临时目录，保证仍然能工作而不是直接失败。
pub fn icon_cache_dir(home: Option<&Path>) -> PathBuf {
    match home {
        Some(home) => home.join("Library/Caches/Flashcast/icons"),
        None => std::env::temp_dir().join("flashcast-icons"),
    }
}

/// 从环境推断图标缓存目录。
pub fn icon_cache_dir_from_env() -> PathBuf {
    if let Some(dir) = std::env::var_os("FLASHCAST_ICON_CACHE_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir);
        }
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    icon_cache_dir(home.as_deref())
}

/// 应用包对应的缓存文件名。同一个包路径永远得到同一个文件名。
///
/// 包被更新（`.app` 目录被替换）时，缓存文件比包旧，[`icon_file_for_bundle`]
/// 会重新渲染。
pub fn icon_cache_path(bundle: &Path, cache_dir: &Path) -> PathBuf {
    // FNV-1a 64：不引入哈希依赖，输出稳定且跨进程一致。
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bundle.to_string_lossy().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    cache_dir.join(format!("{hash:016x}.png"))
}

/// 把应用包图标渲染为 PNG 字节。
///
/// 必须在主线程调用：AppKit 的图形接口不是线程安全的。宿主首次扫描发生在
/// 主线程，`rescan` 命令在 Tauri 的异步线程上，因此调用方需要自行保证线程；
/// 这一点在 `flashcast-platform-check` 的报告里标注为未覆盖项。
#[cfg(target_os = "macos")]
pub fn render_icon_png(bundle: &Path, point_size: f64) -> Result<Vec<u8>, IconError> {
    use objc2::rc::Retained;
    // 提供 `NSBitmapImageRep::alloc()`。
    use objc2::runtime::AnyObject;
    use objc2::AnyThread;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSBitmapImageRepPropertyKey, NSImage, NSWorkspace,
    };
    use objc2_foundation::{NSDictionary, NSSize, NSString};

    let text = bundle
        .to_str()
        .ok_or_else(|| IconError::NonUtf8Path(bundle.to_string_lossy().into_owned()))?;
    let ns_path = NSString::from_str(text);

    let workspace = NSWorkspace::sharedWorkspace();
    let icon: Retained<NSImage> = workspace.iconForFile(&ns_path);
    icon.setSize(NSSize::new(point_size, point_size));

    let tiff = icon.TIFFRepresentation().ok_or(IconError::NoTiff)?;
    let rep = NSBitmapImageRep::initWithData(NSBitmapImageRep::alloc(), &tiff)
        .ok_or(IconError::NoBitmap)?;
    let properties: Retained<NSDictionary<NSBitmapImageRepPropertyKey, AnyObject>> =
        NSDictionary::new();
    let png =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &properties) }
            .ok_or(IconError::NoPng)?;
    Ok(png.to_vec())
}

/// 取得可直接交给 UI（`<img src="data:image/png;base64,…">`）的 PNG 文件路径。
///
/// 已有且不比应用包旧的缓存直接复用；否则渲染并写盘。
#[cfg(target_os = "macos")]
pub fn icon_file_for_bundle(bundle: &Path) -> Result<PathBuf, IconError> {
    let cache_dir = icon_cache_dir_from_env();
    let target = icon_cache_path(bundle, &cache_dir);
    if is_cache_fresh(&target, bundle) {
        return Ok(target);
    }

    let bytes = render_icon_png(bundle, ICON_POINT_SIZE)?;
    std::fs::create_dir_all(&cache_dir).map_err(|error| IconError::CacheDir {
        dir: cache_dir.display().to_string(),
        reason: error.to_string(),
    })?;
    std::fs::write(&target, &bytes).map_err(|error| IconError::CacheWrite {
        path: target.display().to_string(),
        reason: error.to_string(),
    })?;
    Ok(target)
}

/// 缓存文件存在且不早于应用包的最后修改时间。
#[cfg(target_os = "macos")]
fn is_cache_fresh(cache: &Path, bundle: &Path) -> bool {
    let Ok(cache_metadata) = std::fs::metadata(cache) else {
        return false;
    };
    let Ok(bundle_metadata) = std::fs::metadata(bundle) else {
        return false;
    };
    match (cache_metadata.modified(), bundle_metadata.modified()) {
        (Ok(cache_time), Ok(bundle_time)) => cache_time >= bundle_time,
        _ => true,
    }
}
