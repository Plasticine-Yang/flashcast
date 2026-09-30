# 开发环境

## 依赖

- Node.js 24+ 与 pnpm
- Rust stable（`rustup`）
- Tauri 2 所需的系统库

### Linux（无 sudo 的开发机）

本机通常已随桌面环境安装 GTK 3 与 WebKit2GTK 4.1 的运行库，缺少的只是开发用头文件与 `.pc` 文件。
`scripts/dev/linux-native-deps.sh` 会用非 root 权限把这些 `-dev` 软件包下载并解压到本地前缀，
并改写前缀内的 `.pc` 路径，使 `pkg-config` 与链接器都能正常工作。

```bash
eval "$(scripts/dev/linux-native-deps.sh)"   # 首次会下载并解压约 110 个 .deb
```

脚本是幂等的；重复执行只做检查。若 Rust 安装在 `~/.local/share/cargo`，脚本同时会把 `cargo` 加入 `PATH`。

有 root 权限的机器（含 CI runner）直接使用发行版包管理器：

```bash
sudo apt-get update
sudo apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev \
  libayatana-appindicator3-dev libxdo-dev libssl-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev
```

### macOS / Windows

按 Tauri 官方前置条件安装 Xcode Command Line Tools（macOS）或 MSVC + WebView2（Windows）。

## 常用命令

```bash
pnpm install                 # 安装前端依赖
pnpm dev                     # 仅启动 UI（浏览器交互检查）
pnpm tauri dev               # 启动完整桌面应用
pnpm build                   # 构建前端产物
cargo test --workspace       # 运行全部无头测试
cargo build -p flashcast     # 编译 Tauri 宿主
cargo clippy --workspace --all-targets
```

## 测试约定

- 非 UI 集成测试放在 `crates/flashcast-core/tests/`，经由宿主的查询与命令入口验证行为。
- 平台适配层用 `flashcast-platform` 的测试替身；替身通过不证明平台适配通过。
- UI 不写单元测试，通过浏览器交互或真实桌面手动检查。
- 真实平台检查在各平台 runner 上运行，输出「通过 / 失败 / 未覆盖」与原因。

## 本地环境记录

当前开发机：Ubuntu 26.04 (resolute) x86_64，桌面会话为 **Wayland**（`XDG_SESSION_TYPE=wayland`，`WAYLAND_DISPLAY=wayland-0`），同时存在 XWayland 显示 `:0`。
平台能力报告必须区分 X11 与 Wayland 检查结果，不能用 X11 检查推断 Wayland 支持。
