# 开发环境

## 依赖

- Node.js 24+ 与 pnpm
- Rust stable（`rustup`）
- Tauri 2 所需的系统库

### Linux

Tauri 2 需要 GTK 3 与 WebKit2GTK 4.1（libsoup 3）的开发包。桌面环境通常已随系统安装这些库的
运行库，缺的只是开发用头文件与 `.pc` 文件；用发行版包管理器装上即可（需要 sudo）：

```bash
sudo apt-get update
sudo apt-get install -y \
  build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev \
  librsvg2-dev libayatana-appindicator3-dev libxdo-dev libssl-dev libdbus-1-dev \
  patchelf xdg-utils
```

- `libdbus-1-dev` 提供 `dbus-1.pc`：缺了会在编译 `libdbus-sys` 时报「Package dbus-1 was not found」。
- `patchelf` 只在 `pnpm tauri build` 打包 AppImage 时需要，`pnpm tauri dev` 用不到。

安装后自查 `pkg-config` 解析到的是 4.1（不是 4.0，误链 4.0 能编译、只在运行时出问题）：

```bash
pkg-config --modversion webkit2gtk-4.1
```

### macOS / Windows

按 Tauri 官方前置条件安装 Xcode Command Line Tools（macOS）或 MSVC + WebView2（Windows）。

## 常用命令

```bash
pnpm install                 # 安装前端依赖
pnpm dev                     # 仅启动 UI（浏览器交互检查）
pnpm tauri dev               # 启动完整桌面应用
pnpm dev:check               # 检查开发产物隔离、源码监听及更新
pnpm build                   # 构建前端产物
pnpm ui-check                # 浏览器交互检查（自动起停 Vite + 真实 Chrome）
cargo test --workspace       # 运行全部无头测试
cargo build -p flashcast     # 编译 Tauri 宿主
cargo run -p flashcast-platform --bin flashcast-platform-check   # 真实平台检查
cargo clippy --workspace --all-targets
```

## 开发产物与清理

Rust workspace 的默认编译目录是根目录 `target/`，其中包含用于加速后续编译的缓存。
多次迭代、切换编译配置或升级工具链后，目录可能明显增大。日常开发保留缓存；需要回收
磁盘空间时，先停止开发服务器和 Cargo 编译，再按需执行：

```bash
du -sh target artifacts                      # 查看本机产物体积
pnpm dev:clean --dry-run                     # 预览清理 workspace 成员的产物
pnpm dev:clean                               # 清理成员产物，保留外部依赖的编译缓存
cargo clean --dry-run                        # 预览完整清理
cargo clean                                  # 完整清理，下次启动需要重新编译依赖
```

`pnpm ui-check` 每次运行会重建 `artifacts/ui/`，只保留当前运行的截图和日志。
`artifacts/` 下的发布记录和平台诊断有独立用途，需要按用途处理。

### Vite 报 ENOSPC

当错误包含 `syscall: 'watch'` 和 `System limit for number of file watchers reached` 时，
先检查监听范围。`.gitignore` 只影响 Git，Vite 的忽略规则由 `vite.config.ts` 的
`server.watch.ignored` 决定。根目录 `target/`、`artifacts/` 和 `src-tauri/` 已显式排除；
`dist/`、`node_modules/` 与 `.git/` 由 Vite 默认排除。新增产物目录时，同步配置监听隔离，
并扩展 `scripts/dev/watch.test.mjs` 的夹具。

`pnpm dev:check` 在临时工作区加载真实 Vite 配置，验证产物目录没有被监听、前端源码修改
仍被监听且能更新开发服务器响应。CI 的浏览器检查任务也运行它，因此无需等本机积累
数万个文件才能发现遗漏。它是开发工具集成检查，不是 UI 单元测试。

排除编译产物后，文件数量增加不会消耗 Vite 的监听额度。若仍报监听额度错误，再检查
其他进程的占用与 `sysctl fs.inotify.max_user_watches fs.inotify.max_user_instances`。

参考：[Cargo 构建缓存](https://doc.rust-lang.org/cargo/reference/build-cache.html)、
[Cargo 清理选项](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)、
[Vite ENOSPC 排障](https://vite.dev/guide/troubleshooting#vite-crashes-with-enospc-error)。

## 测试约定

- 非 UI 集成测试放在 `crates/flashcast-core/tests/`，经由宿主的查询与命令入口验证行为。
- 平台适配层用 `flashcast-platform` 的测试替身；替身通过不证明平台适配通过。
- UI 不写单元测试，通过浏览器交互或真实桌面手动检查。
- 真实平台检查在各平台 runner 上运行，输出「通过 / 失败 / 未覆盖」与原因。
- 候选版本的能力与覆盖情况汇总在 [`docs/platform/capability-report.md`](../platform/capability-report.md)
  （机器可读版本 `docs/platform/capability-report.json`，由 `scripts/ci/capability-report.py`
  从 `docs/platform/capability-report.meta.json` 与 `docs/platform/evidence/*` 生成）。
  替身检查、真实平台检查与未覆盖项分列；编译成功不是行为证据。

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
设置页与工作区操作、设置页侧栏分组（一次只显示一个区块、`↑↓` 切换、会话内记住上次区块、
640×420 真实窗口下 10 个区块一屏内全部可见）、电弧深浅偏好（浅色 / 深色 / 跟随系统）的切换与布局稳定性、
跟随系统在运行时响应 `prefers-color-scheme`、减少动态效果下无动画、
本地主题包的安装 / 选择 / 移除与无效主题包保留外观、200% 系统缩放。
发布关键路径在 `tools/ui-check/release-checks.mjs`，与完整检查一起执行。产物在 `artifacts/ui/`（代表截图、失败截图、`ui-check.log` 与自动启动的 `vite.log`，该目录已被 gitignore）。首个失败立即停止，避免状态污染和重复超时；全部截图使用 `FLASHCAST_UI_SCREENSHOTS=all pnpm ui-check`。

发布入口与 CI 分工见 [发布流程](../agents/release.md)。

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

## Wayland 快捷键

Wayland 使用 XDG GlobalShortcuts 门户，启动时由桌面请求用户授权，最终按键以系统绑定为准。没有实现该门户的桌面可用托盘，或在系统自定义快捷键中运行 `flashcast`。首次运行便携 AppImage／开发二进制时会按需创建 `~/.local/share/applications/dev.flashcast.launcher.desktop`（尊重 `XDG_DATA_HOME`），作为门户身份；该条目不显示在应用菜单。deb 随包提供身份文件。

只读平台检查不触发授权弹窗。真实注册检查使用：

```bash
cargo run -p flashcast-platform --bin flashcast-platform-check -- --allow-hotkey-registration
```

该命令只验证注册和注销，不证明真实按键唤起。`cargo test -p flashcast-platform --test wayland_portal` 在隔离 D-Bus 总线上验证身份、授权响应、事件会话隔离、令牌传递及注销，不向实际桌面注入按键。
