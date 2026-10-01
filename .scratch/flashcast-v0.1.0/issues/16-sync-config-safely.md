# 16: 安全拉取、推送配置并处理同步阻塞

Status: done
Category: enhancement

**What to build:** 用户将已提交配置推送远端，或快进拉取另一台设备的修改；遇到离线、鉴权失败、脏工作区、分叉或冲突时保留内容并看到处理指引。

Blocked by: 14, 15

- [x] 设置显示分支、远端、待提交/待同步状态，支持明确触发拉取与推送。
- [x] 无冲突的快进拉取和推送成功，拉取后的有效设置、主题与备忘录重新加载。
- [x] 复用远端关系与凭证管理，离线和鉴权失败不影响本地搜索、插件和设置使用。
- [x] 未提交修改、分叉、冲突或已有进行中的 Git 操作显示阻塞原因，保留所有内容与现有暂存状态，不强推或自动丢弃。
- [x] 提供外部处理指引与重新检测入口；处理后可恢复同步，不要求内置三方合并编辑器。
- [x] 经宿主入口使用临时真实仓库和 bare 远端验证推送、快进、分叉、脏工作区及错误保护。
- [x] 手动或浏览器交互检查同步状态；适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

同步仅操作配置工作区，剪贴板历史、附件与设备特定信息不会进入同步范围。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-02：实现完成并置为 done。实现提交 `d7b7213`、`f0336fb`，并入 `feat/flashcast-v0.1.0`
  （含 ticket 06 主题插件）的合并提交 `fa27883`（7 个文件冲突，均为「双方各自新增」）。

### 文件地图

- `crates/flashcast-core/src/sync.rs`（新）：`SyncStatus` / `SyncBlock` / `SyncBlockKind` /
  `SyncError`（18 个分类，每个带中文 `hint()`）、`SyncControl` + `SyncProgress`（进度与取消）、
  `inspect()` 只读探测（分支、上游、领先/落后、脏状态三维、冲突路径、进行中操作）、
  `classify()` 错误分类（先 `(code, class)` 再消息文本）、`pull()` / `push()`、
  `advance_branch()`（快进检出与回滚）、`status()`。
- `crates/flashcast-core/src/host.rs`：入口 `sync_status` / `redetect_sync_state` /
  `pull_workspace(_with_control)` / `push_workspace(_with_control)` / `sync_progress` /
  `cancel_sync` / `git_busy`；`set_git_busy` 现在同时保存忙标志。
  拉取成功后走 `reload_from_workspace`（ticket 06 起同时重载设置与主题），
  并重新读取 `memos/` 列表。
- `crates/flashcast-core/src/workspace.rs`：`Workspace::memo_files()`。
- `crates/flashcast-core/src/git.rs` / `clone.rs`：`in_progress_state` / `index_path` 提升为
  `pub(crate)` 供同步复用；`CredentialProvider::credential` 同上（凭证来源与脱敏只有一份）。
- `crates/flashcast-core/src/lib.rs`：导出同步类型。
- `crates/flashcast-core/tests/workspace_sync.rs`（新，16 项）：真实临时仓库 + 本地 bare 远端。
- `crates/flashcast-core/tests/support/mod.rs`：`bare_remote_commit`（在裸远端追加提交）、
  `bare_remote_oid` / `bare_remote_file`、`git_set_upstream` / `git_set_remote_url` /
  `git_remote_add`、`git_merge_other`、`git_resolve_all_conflicts`；
  `workspace_files()` 的 `manifest.json` 改为 `PluginManifestFile::defaults()`
  （原手写的 `{"plugins": []}` 缺 `schemaVersion`，ticket 06 会判为无效清单）。
- `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`：`get_sync_status` / `redetect_sync_state` /
  `pull_workspace` / `push_workspace` / `sync_progress` / `cancel_sync` 六个瘦转发命令。
- `src/components/SettingsScreen.tsx`、`src/App.tsx`、`src/api.ts`、`src/types.ts`：设置页
  「远端同步」区段（分支、远端与上游、领先/落后、未提交改动分类、同步能力、进行中操作、
  阻塞分类 + 指引），拉取/推送/重新检测/取消四个入口，拉取期间轮询进度；浏览器模拟宿主
  补上同语义的状态机与场景切换。
- `tools/ui-check/check.mjs`：新增 7 项同步检查（截图 36–45），640×420 真实窗口检查
  扩展到同步区段。

### 实测命令与结果

```bash
FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/16-native-deps eval "$(scripts/dev/linux-native-deps.sh)"
cargo test --workspace
# passed=197 failed=0（合并 feat/flashcast-v0.1.0 之后；本 ticket 新增 16 项）
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null cargo test --workspace
# passed=197 failed=0（CI 对等形式：不依赖宿主的 ~/.gitconfig）
cargo test -p flashcast-core --test workspace_sync
# passed=16 failed=0
cargo build -p flashcast
# Finished dev profile；1 条告警为 ticket 01 遗留的 dead_code（hotkey::status）
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/16-windows cargo check -p flashcast-platform --target x86_64-pc-windows-msvc --all-targets
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/16-macos   cargo check -p flashcast-platform --target x86_64-apple-darwin --all-targets
# 均 Finished dev profile
pnpm build
# tsc --noEmit 与 vite build 均通过
FLASHCAST_UI_URL=http://127.0.0.1:1433 pnpm ui-check
# 汇总：通过 34，失败 0（共 34 项；其中本 ticket 新增 7 项）
```

浏览器检查新增项与截图（`artifacts/ui/`，已被 gitignore）：

| 检查 | 截图 |
| --- | --- |
| 同步区展示分支、远端、待同步、未提交改动、同步能力与进行中操作 | `36-settings-sync.png` |
| 未提交修改阻塞拉取并给出指引，推送仍可用 | `37-settings-sync-blocked.png` |
| 分叉阻塞拉取且不自动合并 | `38-settings-sync-diverged.png` |
| 鉴权失败与离线分类不同且本地功能可用 | `39-settings-sync-auth.png`、`40-settings-sync-offline.png` |
| 快进拉取展示进行中并重新加载设置、主题与备忘录 | `41-settings-sync-pulling.png`、`42-settings-sync-pulled.png` |
| 推送成功且无需推送时给出独立说明 | `43-settings-sync-pushed.png` |
| 重新检测在外部处理后恢复同步 | `44-settings-sync-in-progress.png`、`45-settings-sync-redetected.png` |
| 640×420 窗口内工作区、快捷键、克隆、同步区段均可见可操作 | `13-settings-compact-window.png` |

集成测试覆盖（`workspace_sync.rs`，全部经宿主入口、在真实临时仓库 + 本地 bare 远端上断言）：

1. 状态：分支 `main`、远端 `origin` 与地址、上游 `origin/main`、领先/落后 0、干净、
   拉取与推送可用、`nothingToPush`；状态查询不写索引（索引字节不变）；未关联工作区是具名阻塞。
2. 推送：宿主提交 → 推送 → **bare 远端 `refs/heads/main` 真的移动到本地提交**，远端树里
   `settings.toml` 内容是新值；推送只更新 1 个引用；推送后状态不再领先（跟踪引用已更新）。
3. 「没有需要推送的提交」是独立分类（协商出的引用两边 oid 相同）。
4. 「未配置上游」是独立分类（有远端、无 `branch.*.remote/merge`、无跟踪引用），
   指引给出 `git push -u`，远端未被改动。
5. 快进拉取：应用新提交 → 工作区 `settings.toml`/`theme.json`/新备忘录落盘，
   `Host::settings().hotkey` 变成拉取进来的值，**生效主题切换**为
   `flashcast.theme.dark`（`Host::theme_state().selected` 与 `appearance` 都断言）,
   `outcome.memos` 列出两篇备忘录。
6. 已是最新：无操作，HEAD/索引/工作区字节不变，`reload.applied == false`。
7. 分叉：`Diverged{ahead:1, behind:1}`，本地 HEAD、索引字节、工作区文件、远端引用
   四者全部不变，远端内容没有被写进本地；状态里呈现分叉。
8. 脏工作区：已暂存 + 未暂存 + 未跟踪三种同时存在 → 拉取被拦下，**索引字节、已暂存 blob、
   工作区文件、HEAD、远端引用逐字节不变**，被阻塞时不写入远端文件；状态里
   拉取不可用但推送仍可用。
9. 进行中的操作（`MERGE_HEAD` 标记）阻塞拉取与推送，索引与 HEAD 不变；移除标记后
   `redetect_sync_state()` 立刻恢复可用。
10. 冲突：用真实 `Repository::merge` 制造索引冲突后删掉 `MERGE_HEAD`（「有冲突但没有
    进行中标记」）→ 分类为 `conflicts` 而不是 `operationInProgress`，冲突索引不被改动；
    外部解决后重新检测不再报冲突（此时剩下的是真实分叉）。
11. 取消：调用方在开始前预置取消 → `Cancelled`，HEAD/索引/远端/工作区都不变。
12. 鉴权失败（本地 401 HTTP 桩，`http://127.0.0.1:<port>/repo.git`）分类为 `authFailed`；
    令牌不出现在错误文本、`detail()`、`hint()` 或工作区任何文件里（只在设备本地
    `git-credentials.json`）。
13. 离线（本机关闭端口）分类为 `offline`，与 `authFailed` 不同；推送同样是 `offline`。
14. 鉴权失败与离线时本地功能不降级：`query("")` 仍有快速访问项、设置仍可写、
    本地变更仍可读、本地提交仍能创建。
15. 远端缺少上游分支：分类为 `remoteBranchMissing`，指引是「先推送一次」。
16. `redetect_sync_state()`：未提交修改 → 外部提交后恢复；进行中的操作 → 外部中止后恢复。

### 实测发现的两个 `git2` 陷阱（都写进了代码注释）

1. **`checkout_head()` 在分支引用已被移动之后是静默空操作。** 按研究记录里的
   「`set_target` → `set_head` → `checkout_head(force/safe)`」顺序实现后，拉取报告成功、
   `MergeAnalysis` 也判定为快进、分支引用也移动了，但**工作区与索引完全没有更新**
   （`checkout_head` 返回 `Ok(())`，`settings.toml` 还是旧内容）。用临时探针确认为
   git2 0.21 / libgit2 1.9 的真实行为后，改为**先 `checkout_tree(target)`、再移动分支引用**
   （libgit2 `checkout_head` 的文档注释也正是这个意思：先检出目标，再更新 HEAD）。
2. **libgit2 1.9 在「可快进」时同时置位 `FASTFORWARD | NORMAL`。**
   源码 `merge.c` 里 `ancestor == our_head` 时两个位一起给，只有真分叉才是 `NORMAL`
   单独出现。因此**必须先判 `is_fast_forward()` / `is_unborn()`，最后才把
   `contains(ANALYSIS_NORMAL)` 当作分叉**；反过来会把每一次正常快进都误判成分叉
   （本 ticket 的第一版实现就是这样，被测试当场抓住）。ticket 说明里
   「`contains(ANALYSIS_NORMAL)` 是分叉」这句对 libgit2 1.9 不准确。

### 行为决策（实测并刻意选择）

- **拉取判脏比 `git pull` 更严格**：任何未提交改动（已暂存 / 未暂存 / 未跟踪 / 冲突）
  都在网络操作之前阻塞；被忽略的文件不算脏。`git pull` 只在会被覆盖时才拒绝，
  这里选择「宁可不拉，也绝不覆盖」，代价是必须先提交。
- **推送不被未提交修改阻塞**：推送只搬运已提交对象，不会丢弃任何内容，
  因此状态里如实呈现「有未提交修改」，但推送按钮保持可用。
- **分叉在状态里用本地跟踪引用推算**（同时领先又落后），因为分叉只有 `fetch` 之后才能确认；
  这条判断只用于展示与提示，`pull()` 自己仍然只按本地结构状态阻塞、`fetch` 后按真实的
  `MergeAnalysis` 决策，避免陈旧的跟踪引用把一次正常快进永久挡住。
- **推送成功后更新本地跟踪引用**（与 git 一致），否则状态会一直显示「领先 N」。
- **状态查询是纯本地的**：不发网络请求、不写 `.git/index`（`no_refresh`），
  因此鉴权 / 离线不会出现在状态里，只有真的发起操作才会遇到。

### 未覆盖 / 限制

1. **真实网络远端未测试**：本机无法访问真实 https / ssh 远端。本地 bare 远端证明的是
   「走通了 git2 的 `fetch` / `push` 代码路径、分支引用与工作区真实变化」，
   **不**证明 HTTP/2 智能传输、代理、重定向、TLS 与真实服务器行为。
   用本地路径远端时 `transfer_progress` 不触发（ticket 14 已实测），因此**取消窗口**
   只覆盖了「预置取消」这一条路径。
2. **远端拒绝非快进推送（`RemoteRejected`）没有被测试执行**：需要远端在我方推送期间
   已经领先，本地 bare 远端可以做，但本 ticket 用「分叉」与「无上游」两条路径覆盖了
   「不强推」的保护，未构造服务端 reject 的用例。代码里同时检查
   `push_update_reference` 的 status 与 `ErrorCode::NotFastForward` 两条信号。
3. **鉴权失败用本地 401 HTTP 桩构造**（`http://127.0.0.1:<port>`），
   不经过真实 https/ssh 鉴权流程；`CredentialProvider` 里 ssh-agent / `~/.ssh` /
   `credential_helper` 分支未被本 ticket 的测试执行（ticket 14 已记录同一限制）。
   401 桩能证明的是：真实 libgit2 HTTP 错误被分类为 `authFailed`、令牌被登记进
   `secrets` 并完成脱敏、离线与鉴权是不同分类。
4. **`ErrorCode::Certificate`、`Timeout`、`UnrelatedHistories`、`RemoteRejected`
   四个分类只有代码路径，没有测试执行**：需要真实 TLS 故障 / 超时 / 无关历史 /
   服务端拒绝，均无法在本机稳定构造。
5. **跨目标检查只覆盖 `flashcast-platform`**：该 crate 不依赖 git2，因此这两条
   `cargo check` **没有**编译 libgit2 / openssl，不能证明 Windows/macOS 上 git2 的
   https/ssh 能编译。
6. **`git2` feature 与 Windows CI 的已知风险未处理**：`Cargo.toml` 仍是
   `default-features = false, features = ["https", "ssh"]`（ticket 14 的决定），
   本 ticket **未改动**。本 ticket 的 15 项测试只需要**本地路径传输**（bare 远端），
   不需要任何网络 feature；只有第 12 项「鉴权失败」用到了 libgit2 的 **HTTP 传输**
   （本地 401 桩，地址是 `http://127.0.0.1:<port>`）——libgit2 在有 TLS 后端时才注册
   `http`/`https` 传输，因此这一项**可能**依赖 `https` feature。
   **未验证**：去掉 `https` 后 `http://` 是否仍被支持（需要在 CI 或本机重编 libgit2 才能确认）。
   因此**建议**（不擅自改）：若 Windows/macOS 腿因 `openssl-sys` 失败，
   要么把 feature 收窄为 `features = []` 并把 401 鉴权测试标记为「需要 HTTP 传输，未覆盖」，
   要么改用 `features = ["https", "ssh", "vendored-openssl"]` 让 OpenSSL 随包编译；
   两者都必须由 CI 验证。ssh 路径本 ticket 完全没用到。
7. **git2 的 `transfer_progress` / `sideband_progress` 真实数值未验证**：
   本地传输不触发它们，测试只覆盖「回调已挂上、同步进度结构会更新、推送进度回调触发」
   （`SyncProgress.phase` 与 `updates` 在浏览器模拟里可见，但那是模拟宿主）。
8. **备忘录重新加载的语义是「重新列出 `memos/` 下的 Markdown 文件」**：
   备忘录索引与解析由 ticket 07 落地，本 ticket 只证明拉取后工作区里的备忘录文件
   随新提交更新、并如实列出（`PullOutcome.memos`）。主题则是**真的**重载到
   `Host::theme_state()`（ticket 06 已并入）。
9. **真实桌面交互未覆盖**：浏览器检查只驱动 React UI + 浏览器模拟宿主，
   不代表 Tauri webview、托盘或全局快捷键；真实仓库上的同步由 Rust 集成测试覆盖。
10. **clippy 未运行**：该 toolchain 未安装 `cargo-clippy`。

### 后继 ticket 可用的接缝

- `Host::sync_status()` / `redetect_sync_state()`：纯本地、只读的同步状态，
  含 `blocking` / `push_blocking`（分类 + 中文指引），可直接用于任何需要「现在能不能同步」的地方。
- `SyncError::hint()` / `SyncBlock`：新增阻塞分类时只加一个枚举分支与指引即可，
  界面按 `code` 分支、不解析中文。
- `Workspace::memo_files()`：ticket 07 落地备忘录索引后，这里应改为重新索引而不是列文件。
- 拉取后的重载统一走 `Host::reload_from_workspace`：ticket 06 的主题、ticket 07 的插件清单
  都在这一步生效，后续新增的工作区配置类型也应当接在这里。
