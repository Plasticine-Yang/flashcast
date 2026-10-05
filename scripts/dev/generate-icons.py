#!/usr/bin/env python3
"""生成 Flashcast 的应用图标（PNG / ICO / ICNS）。

Tauri 的 `generate_context!` 会解码 `tauri.conf.json` 中 `bundle.icon` 列出的文件，
因此这些必须是真实可解码的图片，不能是占位空文件。图标是平面色块，
导出的文件很小。

用法：
    python3 scripts/dev/generate-icons.py
"""

from __future__ import annotations

import pathlib

from PIL import Image, ImageDraw

# 输出目录：仓库根下的 src-tauri/icons。
ICONS_DIR = pathlib.Path(__file__).resolve().parents[2] / "src-tauri" / "icons"

# 使用 1024px 母版超采样，再导出各尺寸。
MASTER = 1024

BACKGROUND = (23, 39, 52, 255)
BOLT = (102, 220, 241, 255)
SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 128 128"><rect x="6" y="6" width="116" height="116" rx="27" fill="#172734"/><path d="M77 25 42 59h23Z M88 65H66l-11 38Z" fill="#66dcf1"/></svg>'


def render_master() -> Image.Image:
    """按已选定的 01「切光」矢量坐标画深墨底与两段电光。"""
    img = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    scale = MASTER / 128
    inset = 6 * scale
    radius = 27 * scale
    draw.rounded_rectangle(
        (inset, inset, MASTER - inset, MASTER - inset),
        radius=radius,
        fill=BACKGROUND,
    )

    for triangle in [[(77, 25), (42, 59), (65, 59)], [(88, 65), (66, 65), (55, 103)]]:
        draw.polygon([(x * scale, y * scale) for x, y in triangle], fill=BOLT)
    return img


def scaled(master: Image.Image, size: int) -> Image.Image:
    return master.resize((size, size), Image.LANCZOS)


def main() -> None:
    ICONS_DIR.mkdir(parents=True, exist_ok=True)
    master = render_master()
    (ICONS_DIR / "icon.svg").write_text(SVG + "\n")
    (ICONS_DIR.parents[1] / "public" / "flashcast.svg").write_text(SVG + "\n")

    png_sizes = {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
    }
    for name, size in png_sizes.items():
        scaled(master, size).save(ICONS_DIR / name, format="PNG", optimize=True)

    # Windows 图标：多尺寸 ICO。
    ico_sizes = [16, 24, 32, 48, 64, 128, 256]
    master.save(
        ICONS_DIR / "icon.ico",
        format="ICO",
        sizes=[(s, s) for s in ico_sizes],
        append_images=[scaled(master, s) for s in ico_sizes],
    )

    # macOS 图标：ICNS 内部是若干 PNG 载荷。
    icns_sizes = [16, 32, 64, 128, 256, 512, 1024]
    master.save(
        ICONS_DIR / "icon.icns",
        format="ICNS",
        append_images=[scaled(master, s) for s in icns_sizes],
    )

    for path in sorted(ICONS_DIR.iterdir()):
        print(f"{path.name}: {path.stat().st_size} bytes")


if __name__ == "__main__":
    main()
