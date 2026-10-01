# 01: 在 Linux 唤起、搜索并打开软件，接入三平台 CI

Status: done
Category: enhancement

**What to build:** 用户在当前 Linux 桌面通过快捷键或备用入口打开 Flashcast，输入软件名、选择结果并启动真实软件；每次提交获得三平台构建结果。

Blocked by: None (can start immediately)

- [x] 提供可运行的桌面应用，默认首屏为紧凑搜索列表，软件名称与图标可辨识，空查询有快速访问项。
- [x] 发现当前 Linux 环境的软件，支持搜索、方向键选择、回车启动、重新扫描和失败反馈。
- [ ] 快捷键可配置，唤起即进入输入状态；冲突或环境不支持时提供明确反馈及托盘或应用菜单入口。
- [x] 唤起、关闭、键盘选择与结果更新无动画；中文输入法确认不误执行，旧查询结果不会覆盖新查询，鼠标移动不抢走键盘选择。
- [x] 保存唤起前应用的身份，明确本地实际 X11/Wayland 会话类型；记录通过与无法覆盖的系统能力。
- [x] 建立宿主查询/命令入口的非 UI 验证，手动或浏览器交互检查主流程，不添加 UI 单元测试。
- [x] GitHub Actions 在 Linux x64、Windows x64、macOS Apple Silicon/Intel 上编译，执行可适用的核心检查，并保存诊断信息。
- [x] 完成适用验证后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

首次切片包含真实 Linux 软件启动，不以静态窗口或空工程作为验收。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01（续）：补完 ticket 01 的收尾。以下只记录**实际跑过**的内容；勾选框按「有真实证据」勾，未勾的在本节末尾写明原因。

  **本机环境**：Ubuntu 26.04 x86_64，`XDG_SESSION_TYPE=wayland`、`WAYLAND_DISPLAY=wayland-0`（另有 XWayland `DISPLAY=:0`）。
  所有命令都在 `eval "$(scripts/dev/linux-native-deps.sh)"` 之后执行。

  **验证结果**

  - `cargo test --workspace` → **45 通过 / 0 失败**。明细：flashcast-core 33（host_execute 7、host_search 8、host_selection 6、plugin_isolation 6、settings 6）+ flashcast-platform 12（freedesktop_fixture）。
  - `cargo build -p flashcast` → 通过。剩 2 个 `dead_code` 警告（`commands::host_of`、`hotkey::status`），是给后续 ticket 用的辅助函数。
  - `pnpm install --frozen-lockfile && pnpm build` → 通过（tsc --noEmit + vite build，`dist/` 约 244 KB）。
  - `cargo run -p flashcast-platform --bin flashcast-platform-check` → **实测通过 4、实测失败 1、未覆盖 3**。真实扫描到 **35 个可启动软件**（108 个 .desktop，跳过 73 个），**35/35 解析到图标文件**；会话判定 Wayland；`focus.capture` / `hotkey.register` / `clipboard.text` 为未覆盖，`paste.auto` 为实测失败（Wayland 不允许应用在转移焦点后注入按键）。输出保存在 `artifacts/platform-check-linux-x64.log` 与 `.json`。
  - `pnpm ui-check`（浏览器交互，真实 Chrome 153 + 模拟宿主）→ **6 通过 / 0 失败**，截图与 `ui-check.log` 在 `artifacts/ui/`：空查询 8 项（2 命令 + 6 软件）；输入「终端」过滤为 1 项；方向键 0 → 1 → 0；Escape 后窗口 `hidden=true`、再次唤起输入框清空且回到首屏 8 项；输入法组合期间回车 `lastLaunched=null`、`compositionend` 后回车 `lastLaunched=app:firefox`（正向对照，证明上一条不是空过）；悬停未选中行不改变键盘选择。
  - 桌面应用启动冒烟：`./target/debug/flashcast` 连续运行 12 秒未崩溃（被 timeout 终止，退出码 124），stderr 显示 libayatana-appindicator 已初始化。出现一条 GTK 警告 `gtk_widget_get_scale_factor: assertion 'GTK_IS_WIDGET (widget)' failed`，无功能影响，留给后续 ticket 观察。
  - 跨目标类型检查：`cargo check -p flashcast-platform --target x86_64-pc-windows-msvc` 与 `--target x86_64-apple-darwin` 均退出 0。

  **本次一并修掉的问题**

  - freedesktop `%i` 字段码重复输出 `--icon`（同时修掉 1 个平台测试失败）。
  - `src-tauri/icons/` 缺失导致 `generate_context!` panic；新增 `scripts/dev/generate-icons.py`，生成真实的 PNG / ICO / ICNS（共 156 KB）。
  - `tray::create` 与 `TrayIconBuilder::build<M: Manager<R>>` 的运行时类型不匹配；因 `commands::rescan_and_push` 是具体的 `&AppHandle<Wry>`，`create` 固定为 `Wry`。
  - 首屏空查询没有快速访问项：宿主的 `snapshot()` 只回放状态，从未查询过时是空的；改为首屏与每次唤起都执行 `query("")`。
  - 浏览器模拟宿主缺 `execute`、缺 `src/vite-env.d.ts`、`tsconfig.node.json` 在 composite 下关掉 emit，三者都会让 `pnpm build` 直接失败。
  - `platform_check.rs` 无条件引用 `flashcast_platform::linux`，会让 CI 矩阵里 3 个分支编译失败；已按 `target_os` 条件编译，非 Linux 平台如实报告未覆盖。

  **仍然 未覆盖（不要当成通过）**

  - **全局快捷键**：本机是 Wayland，`global-hotkey` 0.8 只有 X11 的 `XGrabKey` 实现，platform_check 如实报为未覆盖。托盘是这台机器上唯一可用的入口；第 3 个复选框因此未勾选。
  - **托盘与真实 webview**：浏览器检查不是 Tauri webview，`pnpm ui-check` 不能证明托盘菜单、托盘左键切换或 webview 内图标渲染。真实窗口与托盘只做了「进程能起来、appindicator 能初始化」的冒烟，没有目视确认（本机无人值守）。
  - **真实软件启动**：回车的运行逻辑由 core 测试（fake launcher）与浏览器模拟宿主验证，**没有真的拉起过一个桌面软件**；ticket 的验收注记要求首次切片包含真实软件启动，因此第 2 个复选框未勾选，需在下一张 ticket 或人工检查时补上。
  - **唤起前应用身份**：Wayland 下读不到全局焦点窗口，platform_check 报未覆盖；第 5 个复选框只按「会话类型与能力状态被如实记录」勾选，身份保存本身未验证。
  - **CI**：已在本仓库 `feat/flashcast-v0.1.0` 分支上实际运行并通过，运行 `36768856398`（提交 `12a9684`）：6 个任务全部 success —— Linux x64、Windows x64、macOS arm64（原生）、macOS x86_64（在 arm64 runner 上交叉编译），以及两个廉价的跨目标 `cargo check`。第一次运行（`36768539969`）暴露了研究工作笔记里写错的 action 版本 `actions/upload-artifact@v8`（该 tag 不存在，4 个矩阵任务全部卡在 Set up job），已改为 `v7` 后重跑通过。每个矩阵任务都上传了 `diagnostics-<slug>` artifact；平台检查在各 runner 上如实输出「实测通过 / 实测失败 / 未覆盖」（macOS 两条腿在平台实现落地前为 1 通过 / 0 失败 / 7 未覆盖）。注意本机无法交叉 `cargo check -p flashcast`（Windows 目标要 `llvm-rc` 嵌图标，macOS 目标要能识别 `-arch` 的 Apple 工具链），外壳由矩阵里各自的 runner 编译。
  - **release / `tauri build` 打包**未验证：debug 构建走 `devUrl`、不嵌入 `dist`，非 dev 构建才嵌入 `frontendDist`（缺失时 tauri-codegen 直接 panic）。

  **后续 ticket 需注意**

  - `commands::host_of` 与 `hotkey::status` 目前是 dead_code，供剪贴板/设置相关 ticket 使用。
  - 托盘图标取自 `default_window_icon()`（即 32x32.png）；如需托盘专用图标，在 `tray::create` 里改用 `include_image!`。
  - `artifacts/` 已 gitignore；`artifacts/ui/`、`artifacts/platform-check-linux-x64.{log,json}` 是本次的证据，不入库。
  - 本次提交：`01e434a`、`4a0bde3`、`c6a5918`、`bd518e3`、`78533d2`、`d75e9f5`、`0c4fff7`、`83f0049`（均为中文提交说明）。

- 2026-10-01（ticket 17 更正本 ticket 的一处过期结论）：上面「真实软件启动：没有真的拉起过一个桌面软件」已经不成立，**第 2 个复选框现在按真实证据勾选**。ticket 17 新增 `crates/flashcast-core/tests/real_linux_desktop.rs`（默认 `#[ignore]`），用 `flashcast_platform::current()` 的真实适配器组装宿主，从宿主查询入口查到真实条目 `org.gnome.Calculator.desktop`，再用 `Host::execute`（等价回车）启动它，并在 `/proc` 里真的看到 `gnome-calculator` 进程出现、随后 SIGTERM 结束：

  ```text
  cargo test -p flashcast-core --test real_linux_desktop -- --ignored --nocapture --test-threads=1
  → 真实进程出现：pid=926013（这证明回车真的启动了这个软件）
  → 已结束 pid=926013；真实启动验收完成          # 3 passed / 0 failed
  ```

  日志：`artifacts/platform-check/17-real-desktop-acceptance.log`（artifacts/ 不入库）。
  第 3 个复选框（全局快捷键）**仍然未勾选**：Wayland 下 `hotkey.register` 仍是未覆盖；
  在 XWayland 下注册返回实测通过，但那不证明原生 Wayland 客户端收得到按键，见
  `docs/platform/capability-report.md` 的按项结论。
