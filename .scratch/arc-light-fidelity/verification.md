# 电弧浅色校正

用户通过 `pnpm tauri dev` 提供原生 Linux 设置页截图。`src-tauri/src/material.rs` 的 Linux 分支把透明材质降级为实底；原设计稿在浏览器中使用背景与 CSS 模糊。因此即使选中“液态玻璃”，截图也没有桌面透色与模糊。此次未接入 Linux 原生玻璃。

## 已完成

- 按原始电弧设计同步 Rust 与浏览器的冷白底、淡蓝选中态、表面色、正文、分隔线和冷色阴影。
- 保留较深的辅助文字，以满足透明背景极值下的对比度契约。保留现有透明度、深色设计及插件规范。
- 原生降级提示改为明确说明 Linux 版暂未提供桌面玻璃；保留用户选择的风格。
- 截图等待有限动画结束，避免将窗口淡入中间帧误当成稳定颜色。

## 验证

- `pnpm exec tsc --noEmit`：通过。
- `cargo test -p flashcast-core --test appearance_contract`：8 项通过。
- `pnpm ui-check`：74 项浏览器交互验收通过，无 UI 单元测试。
- oil-ui `shoot.mjs`：100% 首屏、设置页及 200% 设置区域，均无控制台错误、横向溢出或图片加载失败；实际查看图像完成自检。
- `git diff --check`：通过。

## 对照证据

- 原设计：`../product-redesign-2026-10-05/round-03/arc-frosted.html`。
- 修改前浏览器首屏：`before/page.png`。
- 修改后浏览器首屏：`after-home/page.png`。
- 修改后实底色层：`after-solid/page.png`、`after-solid-2x/page-@2x.png`。
- 原设计浅色内容页：`reference/light-memos.png`。

以上新截图来自浏览器模拟宿主，不是 Linux 原生窗口验收。实底截图只验证不透明色层；它无法证明桌面玻璃、原生外侧阴影或折射效果。新宿主配色位于 Rust 中，重新运行 `pnpm tauri dev` 后加载。完整玻璃质感与设计稿的差距仍然存在，需要后续实现 Linux 材质。
