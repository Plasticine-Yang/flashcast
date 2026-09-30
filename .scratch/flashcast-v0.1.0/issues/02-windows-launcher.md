# 02: 在 Windows 搜索并打开软件

Status: done
Category: enhancement

**What to build:** Windows 用户通过同一个主窗口搜索和打开本机软件，并获得符合 Windows 环境的快捷键与错误反馈。

Blocked by: 01

- [ ] 在 Windows 上发现常用安装入口的软件并呈现名称、图标和稳定标识，支持重新扫描。
      （实现完成：开始菜单 `.lnk`、注册表 Uninstall 键、`Get-StartApps` 三个来源与图标抽取都接线完毕；
      纯逻辑由 Linux 夹具实测，**Windows API 的真实行为从未执行过**，见 Comments 未覆盖项 1–5。）
- [ ] 从宿主命令入口启动真实目标，路径中的空格及非 ASCII 字符处理正确，失效入口显示错误。
      （启动计划、路径分类与错误分类由 Linux 夹具实测；`CreateProcessW` / `ShellExecuteW` /
      `explorer.exe shell:AppsFolder` 的真实启动未在 Windows 上执行，见未覆盖项 6。）
- [ ] 接入快捷键和窗口唤起，保存原应用身份；冲突或无法操作时提供备用入口和状态。
      （代码接线完成：`RegisterHotKey` 失败经共享分类映射为可展示的 `HotkeyError::Conflict` /
      `BackendUnavailable`；`src-tauri` 先 capture 再 show 的顺序在 Windows 上同样正确。
      真实注册、`WM_HOTKEY` 派发与焦点恢复未执行，见未覆盖项 7–8。）
- [ ] Windows runner 验证可执行的平台集成行为，区分真实执行、测试替身及桌面环境未覆盖项。
      （`flashcast-platform-check` 的 Windows 分支已实现，含真实启动与失效路径报错检查、焦点恢复
      回读校验、explorer/PowerShell 可用性；本机只能做交叉类型检查，CI 的 windows 腿从未在本实现上
      运行过，见未覆盖项 9。测试替身未参与任何 Windows 检查：`fake` 只在 `flashcast-core` 的测试里启用。）
- [ ] 共用核心查询行为与紧凑 UI，通过可用的浏览器交互检查，不添加 UI 单元测试。
      （本 ticket 未改动 UI 与 `src/api.ts`，也未新跑 `pnpm ui-check`，因此不勾选。
      `AppSource` 的增量取值不参与 UI 渲染：UI 的 `SearchItem.source` 是另一个字段。）
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

Windows runner 构建成功单独记录，不推断快捷键和焦点恢复已通过。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01（实现完成，**无 Windows 机器**）：实现落在 `crates/flashcast-platform/src/windows/**`，
  刻意分成「纯逻辑（所有目标编译，Linux 夹具实测）」与「系统调用层（只在 `cfg(windows)` 下编译）」
  两层；所有真实 Windows API 行为一律列为未覆盖 —— 编译通过与 Linux 夹具**不构成**行为证据。

  **提交**
  - `9fa8e6f` 测试(Windows)：补齐纯逻辑层与真实 `.lnk` 夹具
  - `b2ba42c` 功能(Windows)：实现软件目录、启动、焦点与能力探测
  - `3e11d75` 修复(快捷键)：Windows 后端只在创建它的线程上注册
  - `034a491` 功能(平台检查)：Windows 目标报告实测与未覆盖结果
  - `ffaef28` 构建(CI)：校准 upload-artifact 版本并断言 Windows 检查 id 完整
  - `2527bf3` 合并(ticket)：并入 `feat/flashcast-v0.1.0`（macOS 与配置工作区）

  **实现结构**
  - `windows/start_menu.rs`（纯逻辑）：`%APPDATA%` / `%LOCALAPPDATA%` / `%ProgramData%` 下的
    `Microsoft\Windows\Start Menu` 递归扫描，显示名 = 文件名去掉 `.lnk`，按相对路径给分组；
    跳过 `Startup` 目录、`.url`、`desktop.ini`；不跟随目录联接（避免 Windows 11 上
    `%LOCALAPPDATA%` 联接造成重复与死循环）；根目录按解析后路径大小写不敏感去重。
  - `windows/shell_link.rs`（纯逻辑）：`lnk 0.6.4` 解析结果的字段映射（绝对目标、相对目标、
    参数、工作目录、图标、描述）、`%VAR%` 展开、是否需要 `IShellLinkW` 纠正、目标可启动性分类
    （可执行 / 文档 / MSI 广告式 / 不可启动代理 / 缺失）、相对目标按词法解析。
  - `windows/registry.rs`（纯逻辑）：研究笔记 §1.2 的过滤表逐行落地（**`WindowsInstaller == 1`
    单独不是丢弃理由**，只有同时缺 `DisplayIcon` 与 `InstallLocation` 才丢；`SystemComponent`、
    `ParentKeyName`/`ParentDisplayName`、非 `Security Update` 的 `ReleaseType`、KB / `Update for`、
    三无条目丢弃；`NoRemove == 1` 保留），`DisplayIcon` 形态解析（含 `"带 空格 的路径",-101`）、
    `InstallLocation` 下按名称相似度挑 exe（排除 unins/updater/helper 等，无法可信匹配时**不猜**）。
  - `windows/uwp.rs`（纯逻辑）：`Get-StartApps | ConvertTo-Json -Compress` 的对象/数组两种形状、
    `AppID` 字段拼写容错、未解析资源名与 shell 宿主过滤、AUMID → `aumid:` argv 与
    `shell:AppsFolder\<AUMID>`。
  - `windows/identity.rs`、`windows/launch_plan.rs`、`windows/icons.rs`、`windows/version.rs`（纯逻辑）：
    稳定标识与去重（AUMID > 目标路径 + 参数 > `.lnk` 路径）、三种启动计划、预乘 BGRA 反预乘与
    PNG 编码、系统版本拼接。
  - `windows/catalog.rs`：三来源组合 + `IShellLinkW` 纠正 pass（只在缺目标、含未展开变量、
    相对路径或目标不存在时调用：`Resolve(SLR_NO_UI|SLR_NOSEARCH)` 后 `GetPath(fFlags = 0)`）
    + `IShellItemImageFactory::GetImage` 抽取图标（256px，反预乘后写
    `%LOCALAPPDATA%\Flashcast\icons`，缓存键含来源修改时间）。
  - `windows/launcher.rs`：`.exe` → `Command::new`（**不预加引号**），`.lnk`/`.url` →
    `ShellExecuteW`（返回值 `<= 32` 视为错误码，740 映射为「需要管理员身份运行」），
    AUMID → `explorer.exe shell:AppsFolder\…`（pid 如实报 `None`：explorer 只是代理）。
  - `windows/focus.rs`：`GetForegroundWindow` + `GetWindowThreadProcessId` →
    `QueryFullProcessImageNameW`（`PROCESS_QUERY_LIMITED_INFORMATION`）；恢复用
    `ShowWindow(SW_RESTORE)` + `SetForegroundWindow`，失败再 `AttachThreadInput`，最后回读
    `GetForegroundWindow` 校验；前台窗口属于 Flashcast 自身时报错，而不是把自己记成「唤起前应用」。
  - `windows/cap.rs`、`windows/session.rs`：桌面会话事实（`GetForegroundWindow` / `GetShellWindow`）
    与能力结论分离；剪贴板与自动粘贴保持「未覆盖」，只报告环境前提。
  - `src/hotkey_backend.rs`：合并后成为 Linux / Windows / macOS 共用的一份。Windows 侧改为
    作用域回调 `with_manager`：管理器持有隐藏窗口 `HWND`（`!Send + !Sync`，不能进 `static`），
    因此放 `thread_local`，并用进程级 owner 线程记录拒绝在其它线程注册（否则会得到
    「注册成功但永远收不到按键」的假象）。Linux 侧行为不变。
  - `src/catalog.rs` 新增 `AppSource::{StartMenu, Registry, Uwp}`（与 macOS 的 `Bundle` 共存）；
    `src/lib.rs::current()` 增加 Windows 分支；`src/unsupported.rs` 现在只在
    Linux / Windows / macOS 之外的目标上编译；`image` 从 Windows 专属依赖移到通用依赖，
    让像素编码链路能在 Linux 上被往返验证。
  - `.github/workflows/ci.yml`：`upload-artifact@v8` → `@v7`（笔记版本表：最新大版本是 v7），
    Windows leg 新增一步断言平台检查 JSON 含全部 11 个必需 check id（缺 id 直接失败；
    该步只证明 Windows 分支跑到每个检查，不表示桌面行为通过）。

  **实测的命令与结果（全部在本机 Linux 上；合并后重跑）**
  - `cargo test --workspace` → 退出码 0，18 个测试二进制全部 `test result: ok`，0 失败。
    其中新增的 `crates/flashcast-platform/tests/windows_fixture.rs` **8 项全过**：它按
    MS-SHLLINK 规范**逐字节生成真实 `.lnk`**（76 字节 header + 含 VolumeID 的 LinkInfo +
    Unicode StringData），交给 `lnk 0.6.4` 真实解析后断言字段映射（含非 ASCII 名称与参数）、
    含 `%ProgramFiles%` 的目标触发纠正判定、广告式 MSI 目标不可启动、开始菜单目录 →
    快捷方式清单 → 解析 → 稳定标识整链、重名快捷方式按稳定标识折叠（保留根目录那条）、
    注册表过滤表与 AUMID → 启动计划的组合、预乘 BGRA → 直通 RGBA → PNG 往返。
    `flashcast-platform` 单元测试 41 项全过，其中 `windows::*` **36 项**。
  - `CARGO_TARGET_DIR=/tmp/wcheck-02 cargo check -p flashcast-platform --target x86_64-pc-windows-msvc`
    → 退出码 0（每个提交前都跑；最后一次用 `--all-targets` 也为 0，唯一 warning 来自
    macOS 夹具测试里一个未使用的导入，不是本 ticket 的代码）。
  - `CARGO_TARGET_DIR=/tmp/mcheck-02 cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets`
    → 退出码 0（确认合并没有破坏 macOS 目标）。
  - `cargo run -p flashcast-platform --bin flashcast-platform-check`（本机 Wayland/GNOME）
    → 与改动前**完全相同**的 8 个 check id 与状态：实测通过 4（build.compile、session.detect、
    apps.discovery、x11.diagnostics）、实测失败 1（paste.auto，Wayland 限制）、未覆盖 3
    （focus.capture、hotkey.register、clipboard.text）；JSON 合法且 `summary` 一致。
    Linux 路径行为未变（改动只在 `AppSource` 取值、`hotkey_backend` 的 Windows 分支与按平台收窄的
    `#[cfg]`）。
  - `cargo clippy` **未运行**：本机 toolchain 没有安装 `cargo-clippy`
    （`cargo-clippy is not installed for the toolchain 'stable-x86_64-unknown-linux-gnu'`），
    未执行 `rustup component add clippy`。
  - `pnpm ui-check` **未运行**：本 ticket 未改动 UI。

  **未覆盖项（无 Windows 机器，全部未实测）**
  1. 真实开始菜单扫描：目录规模、真实 `.lnk` 变体（二进制/非 Unicode StringData、
     ExtraData 中的 `EnvironmentVariableDataBlock` / `TrackerDataBlock` / 属性存储、
     广告式 MSI 快捷方式、损坏文件）、`%LOCALAPPDATA%` 联接的实际形态。
  2. `IShellLinkW` 纠正 pass：`CLSID_ShellLink = {00021401-0000-0000-C000-000000000046}`
     取自 `shobjidl_core.h` 而**未**在 crate 源码中核对；`SLR_NO_UI|SLR_NOSEARCH` 与
     `GetPath(fFlags = 0)` 的真实展开行为；`S_FALSE`（缓冲区不足）时没有重试逻辑。
  3. `IShellItemImageFactory::GetImage`：真实位图尺寸与预乘 BGRA 字节序、全透明位图判定、
     以 `.lnk` / `shell:AppsFolder\<AUMID>` 作为解析名的表现、图标缓存目录写入与并发改名。
     反预乘与 PNG 编码本身有 Linux 往返测试，但**没有验证它得到的是正确图案**。
  4. `windows-registry`：`wow64_32` / `wow64_64` 视图在 32 位与 ARM64 宿主上的实际行为、
     `ExpandString` 取值、真实 Uninstall 键分布与过滤结果。
  5. `Get-StartApps`：真实 PowerShell 输出与时延（笔记称冷启动 200–500ms）、
     被 AppLocker / 约束语言模式阻止时的失败路径、AUMID 覆盖率。
  6. 真实启动：`Command::new` 对含空格与非 ASCII 路径的行为、系统 PATH 搜索、
     `ShellExecuteW` 的返回值与错误码映射（含 740 提升权限）、AUMID 经 `explorer.exe` 启动、
     控制台程序是否需要额外处理。
  7. 焦点：`GetForegroundWindow` 采集时机、`QueryFullProcessImageNameW`、
     `ShowWindow(SW_RESTORE)` + `SetForegroundWindow` 与 `AttachThreadInput` 兜底的真实成功率
     （微软未把 `AttachThreadInput` 列为受支持的绕过前台锁手段）。
  8. 快捷键：`RegisterHotKey` 的真实注册/注销、`WM_HOTKEY` 是否真的派发到回调、
     组合被占用时是否真的得到 `HotkeyError::Conflict`、托盘备用入口的可用性。
  9. **Windows runner**：`.github/workflows/ci.yml` 的 `windows-latest` 腿从未在本实现上运行
     （本 ticket 不推送）。推送后需核对 `artifacts/platform-check-windows-x64.{log,json}`：
     11 个 check id 齐全（新增的断言步骤会在缺 id 时直接失败）；预期 `build.compile`、
     `session.detect`、`apps.discovery`、`apps.launch`、`win.shell` 为实测通过，
     `focus.capture` / `focus.restore` / `hotkey.register` 在无人值守 runner 上为
     「未覆盖」并写出具体原因；`clipboard.text` / `paste.auto` 预期未覆盖
     （ticket 08/09/10 范围），`x11.diagnostics` 预期未覆盖（不适用）。
  10. 手动交互验证：真实窗口唤起、托盘、输入法、图标目视效果，本机无法进行。
  11. `-p flashcast`（Tauri 外壳）无法在 Linux 上交叉 `cargo check`（Windows 目标需要 `llvm-rc`
      嵌图标），因此外壳侧只做人工核对：`src-tauri/src/summon.rs` 先 `capture_previous_app()`
      再 `window.show()/set_focus()`，这个顺序在 Windows 上同样是正确的（恢复时 Flashcast
      自己是前台进程，满足 `SetForegroundWindow` 的前台条件）。

  **合并**：`git merge feat/flashcast-v0.1.0`（`2527bf3`，两个父提交）解决 9 处冲突：
  来源枚举保留两侧新增值、`lib.rs` 同时保留 Windows 与 macOS 分支并去掉重复的
  `hotkey_backend` 模块声明（Windows 侧与 macOS 侧各声明了一份）、`hotkey_backend` 合并为
  一份三平台共用、`platform_check.rs` 在 macOS 分支之上重新接回 Windows 分支。
  合并后重跑 `cargo test --workspace` 与两个目标的 `cargo check --all-targets` 均退出码 0。
