# 03: 在 macOS 搜索并打开软件

Status: done
Category: enhancement

**What to build:** Apple Silicon 和 Intel Mac 用户通过同一个主窗口搜索、启动本机应用，获得准确的快捷键及权限状态。

Blocked by: 01

- [ ] 发现系统与用户应用目录中的应用，名称、图标和标识可用，支持刷新与失效反馈。
      （实现完成；遍历与 `Info.plist` 字段映射已由 Linux 夹具实测，但**真实 macOS 目录扫描与图标字节从未执行过**，见 Comments 未覆盖项 1–3。）
- [ ] 宿主通过 macOS 适配启动目标应用，正确处理应用包与非 ASCII 名称。
      （参数拼装已由 Linux 夹具实测；`open -a <路径>` 的真实启动、Gatekeeper 与 `--args` 丢弃行为未在 macOS 上执行，见未覆盖项 4。）
- [ ] 快捷键与唤起支持 macOS，保存唤起前应用身份；需要系统权限时给出对应状态和设置入口。
      （代码接线完成，能力结论由 Linux 夹具实测；Carbon 注册、`frontmostApplication` 采集、焦点恢复与 AX 权限查询的真实行为未执行，见未覆盖项 5–7。）
- [ ] macOS runner 验证可执行的平台行为，Apple Silicon/Intel 的构建结果分别记录。
      （本机只能做类型检查；两条 runner 腿由 CI 矩阵提供，本 ticket 无法推送触发，见未覆盖项 8。）
- [x] 交互环境或权限限制明确列为未覆盖，不把命令调用成功写成用户操作成功。
      （`flashcast-platform-check` 对无桌面会话、未授予辅助功能权限、适配未实现的能力一律输出「未覆盖」并写明原因；本 Comments 逐条列出。）
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

应用发布签名在候选安装包 ticket 中处理；本 ticket 验证软件发现与启动行为。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01（实现完成，**无 macOS 机器**）：实现落在 `crates/flashcast-platform/src/macos/**`，
  纯逻辑与 Apple 集成分离，使遍历、`Info.plist` 映射、启动参数、焦点策略与能力结论可以在
  Linux 上被真实夹具执行；其余 macOS 专有行为一律标记为未覆盖。

  **实现结构**
  - `src/macos/bundle.rs`（不依赖 Apple API）：扫描根目录、最多 3 层遍历、`Info.plist` 解析、
    `CFBundleIconFile` 解析、重复 bundle id 消歧、稳定条目 id。
  - `src/macos/icons.rs`：`NSWorkspace::iconForFile` → `NSImage` → `TIFFRepresentation` →
    `NSBitmapImageRep::initWithData` → `representationUsingType:properties:`(PNG)，
    写入 `~/Library/Caches/Flashcast/icons`；**不解析 `.icns`**（现代应用只有 `Assets.car`）。
  - `src/macos/launcher.rs`：一律 `open -a <应用包路径>`，**不使用 `open -b <bundleid>`**。
  - `src/macos/focus.rs`：`NSWorkspace.frontmostApplication` 采集；`activateWithOptions`(空 options)
    恢复，macOS 14+ 回退 `activateFromApplication:options:`；`AXIsProcessTrusted` /
    `AXIsProcessTrustedWithOptions` 与辅助功能设置面板 URL。
  - `src/macos/hotkeys.rs` + `src/hotkey_backend.rs`：共享 `global-hotkey` 后端的按键映射、
    回调分发与错误分类（Linux 侧 `linux/hotkeys.rs` 改为只判断会话，行为不变）。
  - `src/macos/cap.rs`：`MacosEnvironment`（事实）与 `capabilities_for`（结论）分离，结论可测。
  - `src/macos/catalog.rs`：组合扫描与图标渲染，渲染失败时清掉 `.icns` 提示路径，让 UI 回退占位图标。
  - `src/catalog.rs` 新增 `AppSource::Bundle`；`src/lib.rs::current()` 增加 macOS 分支；
    `src/unsupported.rs` 的桩现在只服务 Windows 及未知目标。

  **实测的命令与结果**
  - `cargo test --workspace` → 退出码 0；15 个测试二进制全部 `test result: ok`，
    其中新增的 `tests/macos_bundle_fixture.rs` **27 项全过**（真实临时目录 + 真实 XML `Info.plist`
    驱动真实实现）：根目录集合、1–2 层嵌套发现、深度上限、不递归进入 `.app`、
    重叠根目录去重、非应用包/损坏包/缺 `Info.plist`/`LSBackgroundOnly` 的跳过原因、
    `CFBundleDisplayName`→`CFBundleName`→目录名 回退、`CFBundleIconFile` 省略扩展名、
    非 ASCII 名称、重复 bundle id 保留两份且名称/id 消歧、id 与输入顺序无关、按 id 稳定排序、
    启动计划（按路径、`--args` 分隔、终端包装、终端+参数报错而不是静默丢参）、
    焦点恢复依据（pid 优先、bundle id 兜底）、能力结论（未授权辅助功能时自动粘贴必须报「不支持」
    并给出设置入口）、图标缓存路径稳定性。
  - `CARGO_TARGET_DIR=/tmp/mcheck-03 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets`
    → 退出码 0，0 warning（Intel 交叉类型检查）。
  - `CARGO_TARGET_DIR=/tmp/mcheck-03-arm cargo check -p flashcast-platform --target aarch64-apple-darwin --all-targets`
    → 退出码 0，0 warning（Apple Silicon 交叉类型检查）。
  - 交叉检查本身已被证伪测试验证过：在 `src/macos/cap.rs` 临时插入一处类型错误后该命令报 2 个 error，
    移除后回到 0 —— 它确实在检查 macOS 代码，而不是被 `cfg` 跳过。
  - `cargo run -p flashcast-platform --bin flashcast-platform-check`（本机 Wayland/GNOME）
    → 与改动前相同的 8 个 check id 与相同 statuses：实测通过 4、实测失败 1、未覆盖 3；
    JSON 输出合法，新增 `"testDoubles": []`，人类可读输出新增一行「真实执行…未使用任何测试替身」。
    Linux 侧检查逻辑与结果未变。
  - 已知限制（**产品缺陷，不是环境限制**）：macOS 下 `open` 成功只表示 LaunchServices 接受了请求，
    应用已在运行时 `--args` 会被丢弃 —— 无法在同步调用里察觉，已写入模块文档与回执 `argv`。
  - IPC 名/入口未改：`src-tauri` 一行未动。已核对平台无关的唤起顺序：`src-tauri/src/summon.rs`
    先 `capture_previous_app()` 再 `window.show()/set_focus()`，在 macOS 上同样是正确顺序；
    `hotkey::apply_from_settings` 在 `.setup()`（主线程）调用，满足 Carbon 事件目标的前提。
    `-p flashcast` 无法在 Linux 上交叉 `cargo check`，因此外壳侧只做人工核对。

  **未覆盖项（无 macOS 机器，全部未实测；编译通过与 Linux 夹具不构成行为证据）**
  1. 真实 `/Applications`、`/System/Applications(+Utilities)`、`~/Applications` 的扫描结果、
     目录规模与真实 `Info.plist` 样本（二进制 plist 变体、`CFBundlePackageType` 缺失等）。
  2. `NSWorkspace::iconForFile` → PNG 的真实渲染，尤其是只有 `Assets.car` 的应用
     （Finder / Safari / 系统设置）。`apps.icon_render` 检查只校验 PNG 魔数，即「AppKit 产出了 PNG」，
     **不证明图标图案正确**，也不证明在无 Aqua 会话的 runner 上一定成功。
  3. 图标缓存目录 `~/Library/Caches/Flashcast/icons` 的真实写入与 `FLASHCAST_ICON_CACHE_DIR` 覆盖。
     另：从 Tauri 异步线程执行 `rescan` 时调用 AppKit 的线程安全性未验证（首次扫描在主线程，
     `NSWorkspace` 文档称 `runningApplications` 线程安全，但 `iconForFile` 未明确）。
  4. `open -a <路径>` 的真实启动：应用包、非 ASCII 名称、未注册/隔离（quarantine）包被 Gatekeeper
     拦下的表现，以及 `--args` 在实例已运行时被丢弃的实际行为。
  5. `NSWorkspace.frontmostApplication` 的真实采集结果，以及 `activateWithOptions(0)` /
     `activateFromApplication:options:` 在 macOS 14+ 上是否真的把焦点交还。
  6. `AXIsProcessTrusted()` 的真实返回值、`AXIsProcessTrustedWithOptions` 提示流程，
     以及 `x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility`
     是否真的打开「辅助功能」面板（该 URL 方案未由一手来源验证）。
  7. `global-hotkey` 的 Carbon `RegisterEventHotKey` 在真实 Cocoa 事件循环中的注册与注销、
     `Ctrl+Alt+Space` 是否与系统或其他应用冲突、按键事件是否真的送达回调。
     （在无 NSApplication 的 CLI 里注册可能失败，此时检查会如实报「未覆盖」并给出后端原因。）
  8. Apple Silicon (arm64) 与 Intel (x86_64) 的**构建与运行**结果：本地只有类型检查；
     真正的验证是 `.github/workflows/ci.yml` 的两条 macOS 腿（`macos-arm64` 原生、
     `macos-x64` 交叉并借 Rosetta 运行测试与平台检查）。集成分支上一次成功运行（run 36768856398）
     发生在本实现合并之前，因此**本实现从未在任何 macOS runner 上运行过**。需要有人在
     `ticket/03-macos-launcher` 或合并后的 `feat/flashcast-v0.1.0` 上推送一次，并核对
     `artifacts/platform-check-macos-arm64.{log,json}` 与 `...-macos-x64.{log,json}`：
     预期 `apps.discovery`、`apps.icon_render`、`focus.capture`、`hotkey.register`、
     `accessibility.permission` 在本实现下变为实测通过或带原因的未覆盖（runner 无人值守时
     辅助功能权限必然未授权，应报未覆盖），而不是「由后续平台 ticket 提供」。
  9. 手动交互验证（真实窗口、托盘、输入法、图标目视效果）本机无法进行。
  10. 发布签名 / 公证（ad-hoc 或正式）**不是本 ticket 的范围**，由候选安装包 ticket 04 处理。

  **合并**：`git merge feat/flashcast-v0.1.0`（aa77a63）无冲突；合并后重跑
  `cargo test --workspace` 与两个 macOS 目标的 `cargo check` 均退出码 0。
