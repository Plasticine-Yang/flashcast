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
pnpm ui-check                # 浏览器交互检查（自动起停 Vite + 真实 Chrome）
cargo test --workspace       # 运行全部无头测试
cargo build -p flashcast     # 编译 Tauri 宿主
cargo run -p flashcast-platform --bin flashcast-platform-check   # 真实平台检查
cargo clippy --workspace --all-targets
```

## 测试约定

- 非 UI 集成测试放在 `crates/flashcast-core/tests/`，经由宿主的查询与命令入口验证行为。
- 平台适配层用 `flashcast-platform` 的测试替身；替身通过不证明平台适配通过。
- UI 不写单元测试，通过浏览器交互或真实桌面手动检查。
- 真实平台检查在各平台 runner 上运行，输出「通过 / 失败 / 未覆盖」与原因。

### 在开发机上模拟 CI 环境

CI runner 与开发机的差异已经造成过多次「本机绿、CI 红」。提交前请用下面两条命令复核：

```bash
# 1) 不要依赖开发机的 Git 全局配置（CI 上没有 user.name / init.defaultBranch=main）。
#    必须换掉 HOME：`GIT_CONFIG_GLOBAL=/dev/null` 对 libgit2 **无效**，
#    `git2::Repository::signature()` 仍会读到开发机的 ~/.gitconfig。
#    用临时 HOME 运行，就能在不改动真实 ~/.gitconfig 的前提下复现 runner 环境。
mkdir -p "$HOME/.cache/flashcast/fakehome"
HOME="$HOME/.cache/flashcast/fakehome" GIT_CONFIG_NOSYSTEM=1 cargo test --workspace

# 2) 非 Linux 目标只做类型检查，不需要目标平台工具链。
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/windows \
  cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/macos \
  cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets
```

注意：

- 测试夹具必须自己固定仓库初始分支（`real_git_repo` 会写 `HEAD -> refs/heads/main`）。`git2::Repository::init`
  遵循宿主机的 `init.defaultBranch`，开发机是 `main`、runner 上是 `master`，直接断言分支名会在 CI 上失败。
- 提交同理：夹具要保证仓库自带 `user.name` / `user.email`（`git_commit_all` 会在缺失时补上）。
  这一条曾让 `workspace_sync` 的用例在本地全绿、在四条 CI 腿上全部失败。
- 提交身份同理：测试仓库要在仓库本地写入 `user.name` / `user.email`。
- `/tmp` 是 16 GB 的 tmpfs，交叉检查的 target 目录请放在 `$HOME/.cache/flashcast/xcheck/` 下，否则会以
  `Disk quota exceeded (os error 122)` 的形式在无关 crate 上失败。
- 平台原生依赖前缀 `~/.local/share/flashcast/linux-native-deps` 由多个 worktree 共享。同时只用一个版本时无碍；
  并行开发时用 `FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/<ticket>-native-deps` 隔离。


## 浏览器交互检查（UI）

UI 没有单元测试，主流程靠浏览器交互检查：`tools/ui-check/` 是**独立**的 npm 项目
（自己的 `package.json` 与 `node_modules`，不进入应用依赖图），用 `playwright-core`
驱动系统 Chrome。

```bash
pnpm ui-check                                        # 仓库根目录，等价于下一行
pnpm --dir tools/ui-check run check
FLASHCAST_UI_URL=http://localhost:1420 pnpm ui-check # 复用已启动的 Vite
CHROME_PATH=/usr/bin/google-chrome pnpm ui-check     # 默认就是这个路径
```

脚本自己启动 Vite（端口 1420；已在监听则复用）、轮询到可用后再驱动页面，结束时关掉
自己启动的进程。检查项：空查询的快速访问项、输入过滤、方向键选择、Escape 关闭并在唤起后
清空查询、输入法组合期间回车不执行（组合结束后回车才执行）、鼠标悬停不改变键盘选择、
设置页与工作区操作、三个默认主题（浅色 / 深色 / 跟随系统）的切换与布局稳定性、
跟随系统在运行时响应 `prefers-color-scheme`、减少动态效果下无动画、
本地主题包的安装 / 选择 / 移除与无效主题包保留外观、200% 系统缩放。
产物在 `artifacts/ui/`（截图 + `ui-check.log`，该目录已被 gitignore）。

**范围**：只覆盖浏览器里的 React UI 与 `src/api.ts` 中的模拟宿主。它不是 Tauri webview，
因此**不能**证明托盘、全局快捷键、自动粘贴或真实软件启动可用。

## 跨目标类型检查

`cargo check` 不链接，所以可以在 Linux 上检查另外两个目标，不需要 MSVC 或 macOS 工具链：

```bash
rustup target add x86_64-pc-windows-msvc x86_64-apple-darwin
CARGO_TARGET_DIR=/tmp/wcheck cargo check -p flashcast-platform --target x86_64-pc-windows-msvc
CARGO_TARGET_DIR=/tmp/mcheck cargo check -p flashcast-platform --target x86_64-apple-darwin
```

用独立的 `CARGO_TARGET_DIR`，避免与主目标目录互相干扰。CI 的 `cross-check` 任务跑的就是这两条。

只检查 `flashcast-platform`：Tauri 外壳（`-p flashcast`）无法在 Linux 上交叉 `cargo check`——
Windows 目标需要 `llvm-rc` 嵌入图标，macOS 目标需要能识别 `-arch` 的 Apple 工具链。
外壳在三平台上的编译由 CI 主矩阵各自的 runner 覆盖。

## 本地环境记录

当前开发机：Ubuntu 26.04 (resolute) x86_64，桌面会话为 **Wayland**（`XDG_SESSION_TYPE=wayland`，`WAYLAND_DISPLAY=wayland-0`），同时存在 XWayland 显示 `:0`。
平台能力报告必须区分 X11 与 Wayland 检查结果，不能用 X11 检查推断 Wayland 支持。
