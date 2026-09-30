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
