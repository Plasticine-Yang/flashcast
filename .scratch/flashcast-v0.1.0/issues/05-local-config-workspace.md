# 05: 关联本地 Git 配置工作区并保存设置

Status: done
Category: enhancement

**What to build:** 用户通过设置关联现有本地 Git 仓库，或初始化新工作区；修改快捷键等设置后立即生效、重启保留，并能通过编辑器维护配置。

Blocked by: 01

- [x] 设置页支持选择现有本地仓库和为新目录初始化工作区及 Git 仓库，不覆盖已有用户文件。
- [x] 关联前校验目录与配置，切换失败保留当前工作区和有效设置。
- [x] 设置保存为可读文件，应用修改后立即生效，重启后恢复。
- [x] 外部有效修改能重新加载；错误配置显示原因并保留上次有效状态，文件监听不形成写入循环。
- [x] 剪贴板历史、缓存、设备路径、权限状态、日志和凭证留在本机，不由配置工作区默认同步。
- [x] 经宿主设置入口验证真实工作区内容和重载结果，手动或浏览器交互检查设置流程。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

远端克隆、提交和同步分别交付，当前切片离线可独立使用。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-01：实现完成并置为 done。实现提交 `3c717de`、`88ec853`、`6e1c2fa`、`84c3874`、`0d6f1ce`，合并集成分支提交 `cbf0a4e`。

### 工作区布局（实测）

初始化新目录后，工作区树（去除 `.git` 自带文件）与设置文件内容如下：

```text
<工作区>/
  .git/                 # git2 初始化，默认分支 main
  settings.toml         # 设置（TOML）
```

```toml
hotkey = "Super+Space"
launchAtStartup = false
quickAccessLimit = 6
pluginTimeoutMs = 60
disabledPlugins = []
```

键名与 UI / JSON 一致使用 camelCase。`Settings` 启用 `deny_unknown_fields`：拼错的键名（例如
`quick_access_limit = 3`）会报「配置无效」而不是被静默忽略成默认值。`manifest.json`（插件清单）、
`theme.json`（主题配置）与 `memos/*.md` 的路径已定义并纳入文件监听，语义分别由 ticket 07 / 06 / 13 落地。

设备本地数据（当前工作区路径、剪贴板式记录等）位于应用数据目录，与工作区分离：

```json
{ "values": { "workspacePath": "/…/flashcast-config" } }
```

### 文件地图

- `crates/flashcast-core/src/workspace.rs`：工作区模型、目录与配置校验、原子写入（临时文件 + `rename`）、
  `WorkspaceStatus` / `WorkspaceReload`。
- `crates/flashcast-core/src/device.rs`：设备本地存储（应用数据目录），与工作区分离。
- `crates/flashcast-core/src/watch.rs`：`notify-debouncer-full` 监听、`ChangeFilter`（四层自写抑制 + gitdir 排除）。
- `crates/flashcast-core/src/host.rs`：`select_workspace` / `init_workspace` / `workspace_status` /
  `reload_workspace` / `wait_for_workspace_change` / `set_git_busy` 入口；`update_settings` 先落盘再生效。
- `crates/flashcast-core/tests/workspace.rs`（10 项）、`tests/workspace_watch.rs`（8 项）。
- `src-tauri/src/commands.rs`、`src-tauri/src/watch.rs`、`src-tauri/src/lib.rs`：宿主入口转发、
  后台重载线程、外部改动后重新注册全局快捷键。
- `src/components/SettingsScreen.tsx`、`src/App.tsx`、`src/api.ts`、`src/types.ts`、`src/styles.css`：设置页与浏览器模拟宿主。
- `tools/ui-check/check.mjs`：新增 7 项设置流程检查。

### 实测命令与结果

```bash
eval "$(scripts/dev/linux-native-deps.sh)"
cargo test --workspace
# passed=95 failed=0（合并 feat/flashcast-v0.1.0 之后；其中 ticket 05 新增 18 项）
cargo build -p flashcast
# Finished dev profile；2 条告警为 ticket 01 遗留的 dead_code（commands::host_of、hotkey::status）
pnpm build
# tsc --noEmit 与 vite build 均通过
pnpm ui-check
# 汇总：通过 12，失败 0（共 12 项）
```

浏览器检查新增项的截图（`artifacts/ui/`，已被 gitignore）：

| 检查 | 截图 |
| --- | --- |
| 设置页可关联本地 Git 仓库 | `07-settings-unlinked.png`、`08-settings-workspace-linked.png` |
| 无效配置显示原因且不切换工作区 | `09-settings-validation-error.png` |
| 初始化新目录并拒绝覆盖非空目录 | `10-settings-initialised.png` |
| 修改快捷键立即生效，无效写法被拒绝 | `11-settings-hotkey.png` |
| 外部修改重新加载，错误配置保留上次有效状态 | `12-settings-external-reload.png` |
| 设置页在 640×420 窗口内可操作 | `13-settings-compact-window.png` |

集成测试覆盖：选择现有仓库并跨「重启」恢复设置、初始化新目录（含真实 `git2` 仓库）、
非空目录拒绝初始化且不覆盖已有文件、目标工作区配置无效时保留当前工作区与有效设置、
手工编辑设置文件后重启读取文件内容、设备本地数据不出现在工作区目录树、
外部有效修改重载、外部无效修改保留上次有效设置并给出中文原因、自身写入不触发重载循环、
gitdir 变动不触发重载、Git 操作忙标志丢弃事件、工作区被删除时如实报告、切换工作区后监听跟随新工作区。

### 实现中发现并修复的问题

1. **inotify 只读事件造成的自伤事件流。** notify 的 inotify 后端把 `IN_ACCESS` / `IN_OPEN` /
   `IN_CLOSE` 一并上报为 `EventKind::Access`；第 2 层自写抑制要读文件算哈希，而「读」本身又产生
   `Access` 事件，实测形成一次写入后每隔约 500ms 再来一次、永不收敛的事件流。现在只接受
   `Create` / `Modify` / `Remove` / `Any`（并在 `workspace_watch.rs` 中留了回归断言）。
2. **外部修改快捷键后没有重新注册。** 宿主重载了设置，但外壳没有重新注册全局快捷键，
   「外部修改立即生效」不成立。`src-tauri/src/watch.rs` 现记录已注册的写法并在变化时重新注册。
3. **拼错的设置键名被静默忽略。** 见上文 `deny_unknown_fields`。

### 未覆盖 / 限制

- **真实进程重启**：用同一设备目录重建 `Host` 模拟重启（同一测试进程内），未做真实二进制重启后
  的托盘与全局快捷键恢复；真实桌面上的重启恢复需人工检查。
- **原生目录选择对话框**：未接入 `tauri-plugin-dialog`，设置页用路径输入框选择目录。
  `src/api.ts` 的浏览器模拟宿主只模拟 UI 需要区分的几种结果（不存在 / 配置无效 / 非空目录），
  真实校验由 `flashcast-core` 的集成测试覆盖。
- **跨目标编译**：本机只验证了 Linux x64。`flashcast-core` 新增了 `git2` 与
  `notify-debouncer-full`，需要为 Windows / macOS 目标编译 `libgit2-sys` 的 C 代码，本机没有对应
  C 工具链；已读 `libgit2-sys 0.18.8` 的 `build.rs` 确认：找不到系统 libgit2 时会自动回退到
  vendored 构建，因此 CI 上可编译，但**未在 runner 上实测**。
- **`git2` feature**：当前 `default-features = false`（无 https / ssh）。ticket 14/16 需要远端操作时
  必须改为 `features = ["https", "ssh", "vendored-libgit2", "vendored-openssl"]`（`Cargo.toml` 中已留注释）。
- **Git 操作忙标志**：目前只由测试直接置位验证；真实的 status / commit / pull 由 ticket 15/16 接入。
- **链接工作区**（linked worktree，gitdir 在工作区之外）未构造真实用例；代码路径按 `repo.path()` 处理。
- **网络/虚拟文件系统兜底**：WSL `/mnt/c`、NFS 等不投递可靠 inotify 事件，研究记录建议的
  `PollWatcher` 慢轮询兜底未实现。
- **clippy 未运行**：该 toolchain 未安装 `cargo-clippy`。
- macOS FSEvents / Windows file-id 行为未在本机验证（本机为 Linux）。

### 后继 ticket 可用的接缝

- `Workspace::git_dir()` 与 `Repository::open` 检测已就绪，ticket 15/16 可直接做 status / diff。
- `Host::set_git_busy(bool)` 包住整个 Git 操作；操作结束后由调用方按 Git 状态显式重建界面。
- `workspace::write_atomic` / `Workspace::write_settings_bytes` 可复用于其它工作区文件写入。
- 文件监听已覆盖 `settings.toml`、`manifest.json`、`theme.json`、`memos/*.md`；
  ticket 06/07/13 只需扩展 `Host::reload_from_workspace` 的应用步骤。
- `DeviceStore`（应用数据目录）供凭证、缓存与索引使用；工作区只放可迁移偏好。
- 远端克隆（ticket 14）应把「校验目标目录 → 克隆 → 激活」串起来，激活沿用
  `Host::init_workspace` 的「创建后回滚」模式，避免在失败时留下半成品目录。

### 2026-10-01：跨平台文件监听缺陷修复（CI 36772554598）

CI run `36772554598`（提交 `4b6f76b`）在 **Windows x64** 与 **两条 macOS 腿** 上失败，
Linux 通过，失败全部在 `crates/flashcast-core/tests/workspace_watch.rs`：

- macOS arm64：`external_edit_of_settings_is_reloaded` 在 62 行断言
  「一次外部修改不得产生持续的事件流」处 panic。
- macOS x86_64：同一用例 62 行 panic，且 `the_applications_own_write_does_not_form_a_reload_loop`
  在 136 行「自身写入 Ctrl+Shift+F2 触发了重载」处失败。
- Windows x64：`the_applications_own_write_does_not_form_a_reload_loop` 136 行失败，测试二进制 abort。

两个缺陷（都在 `crates/flashcast-core/src/watch.rs` 的 `ChangeFilter`）：

1. **账本条目命中即删。** `accept` 在第一条记录命中内容哈希后 `ledger.remove(path)`。
   inotify 对一次 `rename` 基本只上报一条记录，而 FSEvents / `ReadDirectoryChangesW` 常把
   同一次逻辑写入拆成多条，相邻记录可能相隔数百毫秒到 1 秒；后续记录只能落到 600ms 的
   每路径静默窗口上，晚于它到达时被当成外部修改，产生一次内容并未变化的多余重载。
   **修复**：条目保留到 `LEDGER_TTL`（10s）到期，命中只刷新静默时间戳；不在命中时续期，
   TTL 从记录内容那一刻起算，保证抑制不会无限延长、内存有界。
2. **重载自己的读被当成修改。** macOS 的 FSEvents 经常不给更细的事件类型，`is_modification`
   必须接受 `EventKind::Any`，于是重载时读 `settings.toml` 会被上报成一次「修改」，
   形成「重载 → 读 → 事件 → 重载」。**修复**：宿主先读原始字节再解析
   （`Workspace::read_settings_bytes` / `parse_settings`），把读到的字节哈希记进同一账本
   （`ChangeFilter::record_own_read`、`Host::record_own_read`），内容字节相同的后续事件一律
   被吞掉，与平台上报的事件类型无关。记账**不**开静默窗口：重载是一次读，不该顺带屏蔽内容
   确实变了的紧随修改。解析失败的（无效配置）字节同样记账。

`QUIET_WINDOW` 保持 600ms，**未**做平台化：它只是第 3 层兜底，真正的判据是内容哈希；放宽到
1 秒以上会把「自身写入后紧接着的第二次真实外部修改」也一起吞掉。理由已写进 `watch.rs` 模块文档。

**回归测试**（`crates/flashcast-core/tests/workspace_watch.rs`，均经由宿主入口，不碰内部 API）：

- `a_duplicate_event_with_unchanged_content_is_suppressed`：自身写入 → 等第一条记录被处理且
  静默窗口过期 → 用完全相同的字节再写一次 → 断言无重载。Linux 的 inotify 只上报一条记录，
  用例手工补齐 macOS/Windows 上那条重复记录。
- `a_reload_does_not_repeat_itself_from_its_own_read`：外部修改触发重载 → 用相同字节再写一次
  → 断言仍然只有一次重载。

两处修复各自在 Linux 上临时还原后，对应用例均失败（红→绿验证过），因此这两个用例在 Linux 上
就能挡住这两个缺陷。

**验证结果（Linux x64，本机）**

- `cargo test -p flashcast-core --test workspace_watch`：8 → 10 通过。
- `cargo test --workspace`：95 → **97 通过，0 失败**。
- `cargo build -p flashcast`：成功（只有既有的 2 条 `dead_code` 警告）。
- `CARGO_TARGET_DIR=/tmp/wcheck-wf cargo check -p flashcast-platform --target x86_64-pc-windows-msvc`：通过。
- `CARGO_TARGET_DIR=/tmp/mcheck-wf cargo check -p flashcast-platform --target x86_64-apple-darwin`：通过。

**仍未验证（只能由真实 CI 确认）**

- Windows / macOS 上的**运行时**行为：本机是 Linux，无法运行 FSEvents / `ReadDirectoryChangesW`
  路径。多记录上报与「读被上报为 `Any`」的推理来自 CI 失败现象与平台文档，未在本机实测。
- `flashcast-core` 本身的跨目标**类型检查**做不了：`-p flashcast-core` 会触发 `libz-sys` /
  `libgit2-sys` 的 C 构建（本机没有 Windows/macOS C 工具链，与第 115-118 行的既有说明一致；
  该失败发生在第三方 build script 里，与本次改动无关）。任务要求的 `-p flashcast-platform`
  两条交叉检查已通过。本次改动只用 `std`（`fs::read` / `str::from_utf8` / `Vec<u8>`），
  没有任何 `#[cfg]` 分支，因此跨平台类型风险很低，但仍需 CI 实测为准。

### 2026-10-01：第二次 macOS 失败；判据改为「已应用内容相等」（CI 36777205788）

CI run `36777205788`（提交 `e039e95`）Linux 与 Windows x64 已经通过，**两条 macOS 腿仍然失败**，
失败位置与上一轮相同，都在 `crates/flashcast-core/tests/workspace_watch.rs`：

- `a_reload_does_not_repeat_itself_from_its_own_read`：240 行 panic，消息「重载读到的内容不得再触发一次重载」。
- `external_edit_of_settings_is_reloaded`：80 行 panic，消息「一次外部修改不得产生持续的事件流」。

即：上一轮的「账本条目保留整个 TTL + 重载自读记账」在 Linux 上成立，在 macOS 上仍然不够。
根因是这套抑制**按事件**做判断，而 macOS 的 FSEvents 让事件的三样元数据都不可靠——上报路径未必
等于写入路径；事件类型常只有 `EventKind::Any`；到达时间可能晚于 600ms 静默窗口。继续在
「路径 + 时间 + 事件类型」上加补丁，只会不断在某个平台上找出新的漏口。

#### 设计变更：从「按路径 / 时间抑制」改为「已应用内容相等」

- `HostInner` 新增 `applied_settings_hash`：**当前已应用设置内容**的哈希。
- `Host::reload_from_workspace` 先读 `settings.toml` 的**原始字节**并算哈希，与 `applied_settings_hash`
  比较：**相同则返回 `None`**——不计 `reloads`、不重新生效，`wait_for_workspace_change` 也不对外发出
  任何事件；只有真实内容变化才算一次重载。文本变了但解析后与生效设置相同同样返回 `None`（同时刷新
  哈希，避免重复解析），文件被删除、解析失败等仍如实向外报告。
- 该判据与事件路径、事件类型、FSEvents 延迟、重复 / 合并记录、读文件引发的自伤事件、静默窗口全都无关。
  账本 / 静默窗口 / `git_busy` 保留为廉价的第一道过滤，但**删掉它们正确性仍然成立**（见下文红绿验证）。
- `applied_settings_hash` 的刷新路径（每条都能改变设置，漏一条就会静默吞掉真实修改）：
  1. 应用自身写入：`update_settings` → `persist_settings` 返回落盘字节哈希，与 `inner.settings` 在同一把锁内生效；
  2. 外部重载生效：`reload_from_workspace` 应用新设置时；
  3. 切换 / 首次关联工作区：`activate_workspace`（同时覆盖启动恢复 `restore_workspace`）。

实现提交 `cf95ae2`（宿主机判据与诊断）、`9b203b2`（测试与断言消息）。

#### 可诊断性（上一轮失败消息只有一句话，这次刻意补上）

- `ChangeFilter` 记录最后一次被接受的事件（路径、`EventKind`、过滤决策）与最后一次过滤决策
  （含 `ledger-hit` / `quiet-window` / `git-busy` / `gitdir` / `noise` / `read-only` 等拒绝原因）；
  宿主处理后再补上处理结果（`content-unchanged-noop` / `applied` / `invalid-config` / `settings-file-removed` …）。
- 宿主入口：`Host::last_watch_event()` / `Host::last_watch_decision()`，类型为 `WatchEventTrace`
  （已从 `flashcast_core` 导出）。只保留最近一条，事件本身已过去 500ms 去抖，开销可忽略。
- `workspace_watch.rs` 的负向断言消息现在带上这些信息，例如
  「一次外部修改不得产生持续的事件流（最后接受的事件：path=… kind=… decision=… outcome=…；最后的事件决策：…；已计重载 N）」。
  红绿实验里这些消息直接指出了漏出的事件类型（`Access(Close(Write))`）与分支，可读性达到了目的。

#### 新增测试（`crates/flashcast-core/tests/workspace_watch.rs`，用例数 8 → 12）

- `reload_is_a_noop_while_the_disk_content_equals_the_applied_content`：**绕过文件监听**直接调用
  `reload_workspace`（UI 的「重新检测」入口），证明「内容相等 → 不计重载、不生效；内容变化 →
  恰好一次；同一内容再来一次 → 仍不计」。这是新判据本身的证明，与监听实现无关。
- `every_path_that_changes_settings_refreshes_the_applied_content`：覆盖首次关联、应用自身写入、
  外部重载生效、切换工作区四条刷新路径，最后用一次真实改动证明已应用哈希不会过期到吞掉修改。
- 两个既有回归用例的消息加上事件轨迹，未来失败时能直接定位事件与分支。

#### 验证结果（Linux x64，本机）

- `cargo test --workspace`：**160 通过，0 失败**（集成基线 158 + 本次新增 2）。
- `GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null cargo test --workspace`：同样 **160 通过**
  （CI 与本机的差异是宿主 git 身份与 `init.defaultBranch`，ticket 14 的夹具已固定为 `main`）。
- `cargo build -p flashcast`：成功（仅既有的 2 条 `dead_code` 警告）。
- `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/05b-windows cargo check -p flashcast-platform
  --target x86_64-pc-windows-msvc --all-targets`：通过。
- `CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/05b-darwin cargo check -p flashcast-platform
  --target x86_64-apple-darwin --all-targets`：通过。
- **红 → 绿**（全部为临时改动，验证后已 `git checkout` 还原到 `9b203b2`）：
  1. 设计标准：临时禁用第一道过滤的三层（`is_modification` 一律为真、跳过账本与静默窗口），
     `workspace_watch` 的 **12 个用例仍然全部通过**——正确性只依赖「已应用内容相等」，不依赖那三层。
  2. 在（1）的基础上再临时还原主判据（回到修复前「内容没变也对外发一次事件」的行为），
     `a_reload_does_not_repeat_itself_from_its_own_read` 与 `external_edit_of_settings_is_reloaded`
     在 Linux 上**确实失败**，且失败消息里能看到最后接受的事件与宿主处理结果，说明用例不是空转。

#### 仍未验证（只能由真实 macOS / Windows CI 确认）

- 两条 macOS 腿上的**运行时**行为：本机是 Linux，FSEvents 的路径 / 类型 / 延迟特性无法在本地复现。
  本次把判据从「事件元数据」挪到「文件内容」，正是为了不再依赖这些特性，但**没有** macOS 实机或
  runner 证据；`36777205788` 的两个用例是否转绿只能由下一次 CI 判定，本报告不作此断言。
- `cargo check -p flashcast-core --target x86_64-pc-windows-msvc / x86_64-apple-darwin` 仍做不了：
  会触发 `libz-sys` / `libgit2-sys` 为对应目标编译 C 代码，本机没有 Windows / macOS C 工具链
  （与上文第 192–196 行同因；实测失败发生在第三方 build script，与本次改动无关）。本次新增代码只用
  `std`（`std::fs::read`、`DefaultHasher`、`Option<u64>`），没有平台 `#[cfg]` 分支。
- 真实编辑器造成的**迟到**事件（晚于 600ms 静默窗口）：Linux 上以「相同字节重写文件」模拟，
  未在 macOS 上实测 FSEvents 的真实延迟分布。
