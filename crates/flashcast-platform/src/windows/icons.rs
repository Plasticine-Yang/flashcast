//! 图标像素处理（纯逻辑）与 Windows 的 `IShellItemImageFactory::GetImage` 抽取。
//!
//! 关键事实（研究笔记 §1.6）：`GetImage` 返回的是 **自上而下、预乘 alpha 的 BGRA**
//! 位图。CSS 直接合成预乘数据会在半透明图标上出现暗边，因此交给 webview 之前必须：
//!
//! 1. 除以 alpha 还原直通颜色（`c = c * 255 / a`，四舍五入）；
//! 2. 交换 B/R 得到 RGBA；
//! 3. 若整张图 alpha 全为 0（失效的 `.lnk`、已卸载的应用），判定为空白并回退到
//!    内置占位图标，而不是给用户一张透明图。
//!
//! 1、2、3 都是纯函数，在 Linux 上以真实数值验证；只有取位图那一步需要 Windows。

use std::path::{Path, PathBuf};

/// 启动器网格使用的图标边长（像素）。
pub const ICON_PX: i32 = 256;

/// 还原预乘 alpha 并就地转为 RGBA。
///
/// 输入、输出都是 4 字节一像素，输入顺序为 BGRA（预乘），输出顺序为 RGBA（直通）。
/// 长度不是 4 的倍数时忽略尾部残缺字节。
pub fn unpremultiply_bgra_in_place(buf: &mut [u8]) {
    for pixel in buf.chunks_exact_mut(4) {
        let (b, g, r, a) = (pixel[0], pixel[1], pixel[2], pixel[3]);
        if a == 0 {
            pixel.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        pixel[0] = unpremultiply_component(r, a);
        pixel[1] = unpremultiply_component(g, a);
        pixel[2] = unpremultiply_component(b, a);
        // alpha 不变
    }
}

/// 单个分量的反预乘：`c = c * 255 / a`，四舍五入并夹到 255。
pub fn unpremultiply_component(value: u8, alpha: u8) -> u8 {
    if alpha == 0 {
        return 0;
    }
    let scaled = (u32::from(value) * 255 + u32::from(alpha) / 2) / u32::from(alpha);
    scaled.min(255) as u8
}

/// 位图是否完全透明（无效图标）。
pub fn is_blank(buf: &[u8]) -> bool {
    buf.chunks_exact(4).all(|pixel| pixel[3] == 0)
}

/// 把 RGBA8 像素编码为 PNG 字节。
pub fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let expected = width as usize * height as usize * 4;
    if rgba.len() != expected {
        return Err(format!(
            "像素数据长度不符：期望 {expected} 字节，实际 {}",
            rgba.len()
        ));
    }
    let image = image::RgbaImage::from_raw(width, height, rgba.to_vec())
        .ok_or_else(|| "无法根据给定尺寸构造图像".to_string())?;
    let mut out = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut out, image::ImageFormat::Png)
        .map_err(|error| error.to_string())?;
    Ok(out.into_inner())
}

/// 图标缓存文件名：`来源路径 + 修改时间 + 边长` 的确定性摘要。
///
/// 用 FNV-1a：足够避免碰撞，且不需要为缓存键引入散列依赖。来源文件的修改时间参与
/// 摘要，因此应用升级后图标会自动失效重取。
pub fn icon_cache_key(source: &str, modified_secs: i64, px: i32) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut feed = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for byte in source.as_bytes().iter().copied() {
        feed(byte);
    }
    for byte in modified_secs.to_le_bytes() {
        feed(byte);
    }
    for byte in px.to_le_bytes() {
        feed(byte);
    }
    format!("{hash:016x}.png")
}

/// 图标缓存文件名（不访问磁盘，仅由来源与缓存目录推导）。
pub fn cache_path(cache_dir: &Path, source: &str, modified_secs: i64, px: i32) -> PathBuf {
    cache_dir.join(icon_cache_key(source, modified_secs, px))
}

// ---------------------------------------------------------------------------
// 以下是 Windows 专属的位图抽取。其余部分在所有目标上编译并被测试覆盖。
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub use platform::extract_icon_png;

/// 确保当前线程处于 COM 单元中（供目录扫描在调用 `IShellLinkW` / `GetImage` 前使用）。
#[cfg(target_os = "windows")]
pub fn ensure_com_for_shell() {
    platform::ensure_com();
}

#[cfg(target_os = "windows")]
mod platform {
    use super::{cache_path, encode_png, is_blank, unpremultiply_bgra_in_place, ICON_PX};
    use std::path::{Path, PathBuf};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::SIZE;
    use windows::Win32::Graphics::Gdi::{
        DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO,
        BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
    };
    use windows::Win32::System::Com::{
        CoInitializeEx, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_BIGGERSIZEOK, SIIGBF_ICONONLY,
        SIIGBF_RESIZETOFIT,
    };

    /// 确保当前线程处于 COM 单元中。
    ///
    /// `RPC_E_CHANGED_MODE`（0x8001_0106）表示该线程已经以别的单元模型初始化过 ——
    /// 那是成功而不是失败，Tauri/WebView2 常常已经初始化过了。
    pub(crate) fn ensure_com() {
        const RPC_E_CHANGED_MODE: i32 = 0x8001_0106u32 as i32;
        unsafe {
            let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            if hr.is_err() && hr.0 != RPC_E_CHANGED_MODE {
                // 保持安静：调用方会在后续 API 失败时拿到具体错误。
            }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    /// 取 `source`（`.lnk`、`.exe`、`.ico`、`shell:AppsFolder\<AUMID>` 皆可）的图标，
    /// 反预乘后写成 PNG 到 `cache_dir`。
    ///
    /// 已命中缓存（同名文件存在）时直接复用，不重复调用 Shell。
    pub fn extract_icon_png(
        source: &str,
        cache_dir: &Path,
        px: i32,
    ) -> Result<PathBuf, String> {
        let px = if px > 0 { px } else { ICON_PX };
        let modified = modified_secs(source);
        let target = cache_path(cache_dir, source, modified, px);
        if target.is_file() {
            return Ok(target);
        }
        std::fs::create_dir_all(cache_dir).map_err(|error| format!("创建图标缓存目录失败：{error}"))?;
        let png = render_png(source, px)?;
        // 先写临时文件再改名，避免并发扫描读到半个 PNG。
        let temp = target.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(&temp, &png).map_err(|error| format!("写入图标缓存失败：{error}"))?;
        std::fs::rename(&temp, &target).map_err(|error| format!("提交图标缓存失败：{error}"))?;
        Ok(target)
    }

    fn modified_secs(source: &str) -> i64 {
        // 只有真实文件系统路径才有修改时间；`shell:AppsFolder\…` 等命名空间目标取 0。
        let Ok(metadata) = std::fs::metadata(source) else {
            return 0;
        };
        metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or(0)
    }

    fn render_png(source: &str, px: i32) -> Result<Vec<u8>, String> {
        ensure_com();
        let wide_source = wide(source);
        unsafe {
            let factory: IShellItemImageFactory =
                SHCreateItemFromParsingName(PCWSTR(wide_source.as_ptr()), None)
                    .map_err(|error| format!("无法为 {source} 建立 Shell 项：{error}"))?;
            let bitmap = factory
                .GetImage(
                    SIZE { cx: px, cy: px },
                    SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK | SIIGBF_RESIZETOFIT,
                )
                .map_err(|error| format!("GetImage 失败：{error}"))?;

            // GetImage 可能返回与请求不同的尺寸，必须问 GDI 真实尺寸。
            let mut info = BITMAP::default();
            let copied = GetObjectW(
                bitmap.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut info as *mut _ as *mut core::ffi::c_void),
            );
            if copied == 0 {
                let _ = DeleteObject(bitmap.into());
                return Err("GetObjectW 读取位图信息失败".to_string());
            }
            let width = info.bmWidth.unsigned_abs();
            let height = info.bmHeight.unsigned_abs();

            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: info.bmWidth,
                    // 负高度 = 自上而下（与 GetImage 的返回一致）。
                    biHeight: -info.bmHeight,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0u8; width as usize * height as usize * 4];
            let dc = GetDC(None);
            let scanned = GetDIBits(
                dc,
                bitmap,
                0,
                height,
                Some(pixels.as_mut_ptr() as *mut core::ffi::c_void),
                &mut header,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            let _ = DeleteObject(bitmap.into());
            if scanned == 0 {
                return Err("GetDIBits 读取像素失败".to_string());
            }

            if is_blank(&pixels) {
                return Err(format!("{source} 的图标为全透明位图"));
            }
            unpremultiply_bgra_in_place(&mut pixels);
            encode_png(width, height, &pixels)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_bgra_is_unpremultiplied_to_rgba() {
        // [B, G, R, A] 预乘 → [R, G, B, A] 直通
        let mut buf = vec![64, 32, 16, 128];
        unpremultiply_bgra_in_place(&mut buf);
        assert_eq!(buf, vec![32, 64, 128, 128]);

        // alpha = 255 时不变（只是通道顺序交换）。
        let mut opaque = vec![10, 20, 30, 255];
        unpremultiply_bgra_in_place(&mut opaque);
        assert_eq!(opaque, vec![30, 20, 10, 255]);

        // alpha = 0 的像素必须完全清零，不能留下预乘残留。
        let mut transparent = vec![200, 100, 50, 0];
        unpremultiply_bgra_in_place(&mut transparent);
        assert_eq!(transparent, vec![0, 0, 0, 0]);

        // 多个像素一起处理，顺序不错位。
        let mut two = vec![64, 32, 16, 128, 0, 0, 255, 255];
        unpremultiply_bgra_in_place(&mut two);
        assert_eq!(two, vec![32, 64, 128, 128, 255, 0, 0, 255]);
    }

    #[test]
    fn un_premultiply_component_rounds_and_clamps() {
        assert_eq!(unpremultiply_component(0, 0), 0);
        assert_eq!(unpremultiply_component(255, 255), 255);
        // 预乘值理论上不会超过 alpha，但损坏数据可能导致；必须夹到 255。
        assert_eq!(unpremultiply_component(255, 1), 255);
        // 128 * 255 / 255 = 128
        assert_eq!(unpremultiply_component(128, 255), 128);
    }

    #[test]
    fn fully_transparent_bitmaps_are_blank() {
        assert!(is_blank(&[1, 2, 3, 0, 4, 5, 6, 0]));
        assert!(!is_blank(&[1, 2, 3, 0, 4, 5, 6, 1]));
        assert!(is_blank(&[]));
    }

    #[test]
    fn png_encoding_round_trips_real_dimensions() {
        let pixels = vec![255u8, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255];
        let png = encode_png(2, 2, &pixels).expect("编码成功");
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n", "必须是真实 PNG 签名");
        let decoded = image::load_from_memory(&png).expect("可被解码").to_rgba8();
        assert_eq!(decoded.dimensions(), (2, 2));
        assert_eq!(decoded.into_raw(), pixels, "像素必须无损往返");

        assert!(encode_png(2, 2, &pixels[..8]).is_err(), "长度不符必须报错");
    }

    #[test]
    fn cache_key_depends_on_source_mtime_and_size() {
        let base = icon_cache_key(r"C:\P\a.exe", 100, 256);
        assert_eq!(base, icon_cache_key(r"C:\P\a.exe", 100, 256));
        assert_ne!(base, icon_cache_key(r"C:\P\a.exe", 101, 256), "升级后要失效");
        assert_ne!(base, icon_cache_key(r"C:\P\b.exe", 100, 256));
        assert_ne!(base, icon_cache_key(r"C:\P\a.exe", 100, 32));
        assert!(base.ends_with(".png"));
        assert_eq!(
            cache_path(Path::new("/tmp/cache"), r"C:\P\a.exe", 100, 256),
            Path::new("/tmp/cache").join(&base)
        );
    }
}
