#!/usr/bin/env python3
"""生成 Flashcast 的应用图标（PNG / ICO / ICNS）。

Tauri 的 `generate_context!` 会解码 `tauri.conf.json` 中 `bundle.icon` 列出的文件，
因此这些必须是真实可解码的图片，不能是占位空文件。图标是平面色块，
导出的文件很小。

用法：
    python3 scripts/dev/generate-icons.py
"""

from __future__ import annotations

import io
import pathlib

from PIL import Image, ImageDraw

# 输出目录：仓库根下的 src-tauri/icons。
ICONS_DIR = pathlib.Path(__file__).resolve().parents[2] / "src-tauri" / "icons"

# 主图标按 4 倍超采样后缩放，得到干净的边缘。
MASTER = 1024

BACKGROUND = (24, 27, 36, 255)
BORDER = (47, 54, 70, 255)
BOLT = (255, 201, 75, 255)


def render_master() -> Image.Image:
    """画一个圆角方块 + 闪电符号。"""
    img = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    inset = MASTER * 0.06
    radius = MASTER * 0.22
    draw.rounded_rectangle(
        (inset, inset, MASTER - inset, MASTER - inset),
        radius=radius,
        fill=BACKGROUND,
        outline=BORDER,
        width=max(1, int(MASTER * 0.012)),
    )

    bolt = [
        (0.60, 0.07),
        (0.26, 0.56),
        (0.47, 0.56),
        (0.38, 0.93),
        (0.74, 0.44),
        (0.53, 0.44),
    ]
    draw.polygon([(x * MASTER, y * MASTER) for x, y in bolt], fill=BOLT)
    return img


def scaled(master: Image.Image, size: int) -> Image.Image:
    return master.resize((size, size), Image.LANCZOS)


def main() -> None:
    ICONS_DIR.mkdir(parents=True, exist_ok=True)
    master = render_master()

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
