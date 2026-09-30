# 14: 从远端 Git 仓库克隆并启用配置工作区

Status: done
Category: enhancement

**What to build:** 用户在新设备填写远端仓库与目标目录，克隆配置并切换到校验通过的工作区，恢复设置、主题和备忘录。

Blocked by: 05

- [x] 设置中提供克隆入口，用户选择远端与目标目录，能看到进度、完成和失败状态。
- [x] 目标存在文件时不覆盖；克隆未完成或配置无效时保留当前工作区。
- [x] 克隆成功后校验并关联工作区，已有功能读取其中内容；当相应主题与插件可用时恢复对应选择。
- [x] 建立复用已有凭证管理的远端 Git 操作，鉴权失败有指引，凭证不进入配置或日志。
- [x] 取消、网络失败与重试不会损坏当前配置或混用半完成目录；记录远端和工作区关系。
- [x] 经宿主入口使用临时本地 bare 远端验证克隆和关联；界面用手动或浏览器交互验证。
- [x] 适用验证完成后更新正式 ticket 为 done，与实现一起创建中文 Git commit。

## Comments

提供后续同步使用的远端关系、鉴权与取消行为；不需要依赖具体功能插件完成。

- 2026-10-01：用户确认本 ticket 的粒度、依赖和测试边界；正式发布为 ready-for-agent，尚未开始实现。
- 2026-10-02：实现完成并置为 done。实现提交 `ccc2f51`、`2c34ff4`、`50dc972`、`c3d9ac1`，
  合并集成分支提交 `b47f79c`（并入 ticket 15 / ticket 05b，解决 7 个文件的冲突）。

### git2 feature：启用了什么、为什么

```toml
git2 = { version = "0.21.0", default-features = false, features = ["https", "ssh"] }
```

- git2 0.21 默认 **一个都不开**；`ssh` 不开则 `git@…`/`ssh://` 地址直接失败，`https` 不开则没有 TLS。
- `https` 会同时引入 `openssl-sys` 与 `openssl-probe`：git2 的 TLS 后端**只有** OpenSSL，
  没有 `schannel` / `security-framework` 可选（研究记录已确认）。
- **刻意不启用** `vendored-libgit2` / `vendored-openssl`：两者都要在四条 CI 腿上现场编译 C
  （Windows 还需 Perl 与 NASM），会拖慢全部腿，且本机无法验证。见下方「未覆盖」第 4 条的 CI 风险。
- Linux 构建：`libgit2-sys` 在本机找不到系统 libgit2 时自动回退 vendored 构建（ticket 05 已记录），
  `openssl-sys` 走系统 OpenSSL。本机 `cargo build -p flashcast` / `cargo test --workspace` 全绿。

同时修了本机开发脚本 `scripts/dev/linux-native-deps.sh`：Debian/Ubuntu 的
`opensslconf.h` 在 `usr/include/<multiarch>/openssl/`，而 `openssl.pc` 只导出
`usr/include`，前缀内构建时 gcc 不会搜索多架构目录（系统构建会），因此
`openssl-sys` / `libssh2-sys` 都编译失败。脚本现在把多架构头文件软链到常规 include
目录，并清掉早期误加的 `-I<multiarch>`（`openssl-sys` 只向 `libssh2-sys` 透出**一个**
include 目录，cargo 取最后一个，加多架构目录反而让 `<openssl/macros.h>` 找不到）。

### 文件地图

- `crates/flashcast-core/src/clone.rs`：`CloneControl`（进度 + 取消）、`CloneProgress` / `ClonePhase`、
  `CredentialProvider`（ssh-agent 探测、`~/.ssh` 密钥、`credential_helper`＋`Config::open_default()`、
  设备本地令牌）、`clone_repository`（`RepoBuilder` + `RemoteCallbacks::transfer_progress` /
  `sideband_progress` + `CheckoutBuilder::{progress, notify_on, notify}`）、`ClonedTarget`（回滚）、
  `redact` / `redact_secrets` / `strip_userinfo` / `url_password` / `host_of`、`error_hint`、
  `CloneOutcome`。
- `crates/flashcast-core/src/device.rs`：`CredentialStore`（`git-credentials.json`，Unix 0600）、
  `StoredToken`、`workspace_remote` / `set_workspace_remote`（工作区 ↔ 远端关系表）。
- `crates/flashcast-core/src/workspace.rs`：`WorkspaceRemote`、`Workspace::remote()`、
  `Workspace::recorded_theme()`、`WorkspaceStatus.remote`、`WorkspaceError::{CloneTargetNotEmpty, Clone, CloneCancelled}`。
- `crates/flashcast-core/src/host.rs`：`clone_workspace` / `clone_workspace_with_control` /
  `clone_progress` / `cancel_clone` / `workspace_remote` / `remember_git_token` / `forget_git_token`；
  `activate_workspace` 与 `reload_from_workspace` 现在把 `disabledPlugins` 应用到插件注册表
  （「已有功能读取其中内容」的落点，此前只有 Tauri 外壳在启动时应用一次）。
- `crates/flashcast-core/tests/workspace_clone.rs`（9 项）、`tests/support/mod.rs`
  （`bare_remote`：用 treebuilder + commit 直接构造临时裸仓库，不经过 push，不依赖网络）。
- `src-tauri/src/commands.rs`、`src-tauri/src/lib.rs`：`clone_workspace`（`spawn_blocking`）/
  `clone_progress` / `cancel_clone` 三个瘦转发命令。
- `src/components/SettingsScreen.tsx`、`src/App.tsx`、`src/api.ts`、`src/types.ts`：克隆区段
  （远端地址、可选令牌、进度、取消）、远端关系展示、浏览器模拟宿主的克隆流程。
- `tools/ui-check/check.mjs`：新增 5 项克隆流程检查（截图 20–25）。

### 实测命令与结果

```bash
FLASHCAST_NATIVE_DEPS_PREFIX=$HOME/.cache/flashcast/14-native-deps eval "$(scripts/dev/linux-native-deps.sh)"
cargo test --workspace
# passed=167 failed=0（合并 feat/flashcast-v0.1.0 之后；其中本 ticket 新增 9 项）
cargo test -p flashcast-core --test workspace_clone
# passed=9 failed=0
cargo build -p flashcast
# Finished dev profile；1 条告警为 ticket 01 遗留的 dead_code（hotkey::status）
cargo build -p flashcast-platform
# Finished dev profile（启用 https 后 Linux 仍可链接）
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/14-windows cargo check -p flashcast-platform --target x86_64-pc-windows-msvc
# Finished dev profile
CARGO_TARGET_DIR=$HOME/.cache/flashcast/xcheck/14-macos   cargo check -p flashcast-platform --target x86_64-apple-darwin
# Finished dev profile
pnpm build
# tsc --noEmit 与 vite build 均通过
FLASHCAST_UI_URL=http://127.0.0.1:1431 pnpm ui-check
# 汇总：通过 22，失败 0（共 22 项；其中本 ticket 新增 5 项）
```

上一项命令里的端口 1431 是本分支自己的 Vite（1420 被另一个 worktree 占用，脚本会复用已有服务，
因此显式指定 `FLASHCAST_UI_URL`）。

浏览器检查新增项与截图（`artifacts/ui/`，已被 gitignore）：

| 检查 | 截图 |
| --- | --- |
| 从远端克隆并显示进度与完成状态 | `20-clone-progress.png`、`21-clone-completed.png` |
| 克隆失败显示中文原因且不切换工作区 | `22-clone-failed.png` |
| 克隆过程中取消并保留当前工作区 | `23-clone-cancelled.png` |
| 目标目录非空时拒绝克隆且不覆盖 | `24-clone-non-empty.png` |
| 克隆地址包含密码时拒绝且不泄露 | `25-clone-secret-refused.png` |
| 640×420 窗口内工作区、快捷键与克隆区段均可见可操作 | `13-settings-compact-window.png` |

集成测试覆盖（全部经 `Host::clone_workspace*` 入口，远端是真实临时裸仓库）：

1. 克隆成功后成为当前工作区、`settings.toml` 与 `memos/*.md` 内容被读取、检出进度来自真实回调
   （`checkout_total=4`、`checkout_completed=4`）、远端名/默认分支/上游为 `origin` / `main` / `origin/main`，
   重启同一设备目录后远端关系仍在。
2. 目标目录非空时拒绝：已有文件逐字节保留、不建 `.git`、当前工作区与设置不变、进度阶段为失败。
3. 克隆出来的 `settings.toml` 无效时整体回滚（目标目录被删除）、当前工作区与设置不变，
   随后对同一路径重试成功。
4. 远端地址不存在时同样回滚，不留目录。
5. 已请求取消的克隆（`clone_workspace_with_control` + 预置取消）不创建目录、设置不变，之后重试成功。
6. **克隆进行中**取消（300 个文件的裸远端，等 `checkout_notified>=5` 再取消）→ `CloneCancelled`、
   目录被删除、当前工作区不变、重试后检出全部 300 个文件。
7. 地址里带口令（`https://alice:sekret@…`）被拒绝：错误文本与设备目录、工作区目录里都不含口令；
   设备本地令牌只出现在 `git-credentials.json`，工作区里任何文件都不含它；
   人为把仓库 origin 地址改成带 userinfo 后，`Host::workspace_remote()` 返回的地址已脱敏。
8. ssh 端口不可达时给出可操作的中文指引（网络 / ssh-agent / `~/.ssh`），不留目录、设置不变。
9. 克隆成功后插件选择生效（注册表里 `memo` 按记录停用）、本机没有的 `ghost-plugin` 出现在
   `unavailablePlugins`、`theme.json` 记录的主题名在 `recordedTheme` 里如实返回。

### 实现中发现并修复的两个 `git2` 陷阱

1. **检出被取消时 `git_clone` 仍返回成功。** 用 `CheckoutBuilder::notify` 返回 `false` 中断检出后，
   `RepoBuilder::clone` 可能返回 `Ok`，而目标目录是**空的**（连 `.git` 都没有）。
   `Workspace::open` 接受普通目录（工作区可以不是仓库），因此必须自己校验克隆结果：
   现在要求目标目录能打开为仓库且带远端，否则报「克隆结果不完整」并回滚；取消标志已置位时报「已取消」。
2. **取消窗口只在检出通知阶段。** 实测 libgit2 先跑完整一轮 `notify`（plan）再逐个写文件；
   `CheckoutBuilder::progress` 的签名是 `FnMut(Option<&Path>, usize, usize)`（没有返回值），
   无法用它取消。因此进度结构里单列 `checkoutNotified`（本轮已通知的文件数），
   取消在这一轮里生效；纯本地传输不会触发 `transfer_progress` / `sideband_progress`
   （实测 `received_objects=0`、`received_bytes=0`），网络远端的取消窗口更大。

### 未覆盖 / 限制

1. **真实网络远端**：本机无法访问真实 https / ssh 远端，因此 `transfer_progress`、
   `sideband_progress` 两个网络回调**没有被测试覆盖**（本地传输不触发它们，实测
   `received_objects=0`）。测试证明的是：回调已挂上、进度结构会更新、检出通知的取消真正生效、
   失败与取消都会回滚。对真实远端只验证到「本地 bare 远端 + 真实 git2 克隆代码路径」。
2. **真正的鉴权被拒绝**（https 401 / ssh `Permission denied (publickey)`）无法在本机复现：
   `error_hint` 里针对 `ErrorCode::Auth` / `ErrorClass::Http` 的令牌指引分支只有代码、没有被测试执行。
   测试覆盖到的是「地址里写口令被拒」与「ssh 端口不可达」两类指引。
3. **平台特定的 ssh-agent 探测未实现**：macOS 上 GUI 启动的应用常拿不到 `SSH_AUTH_SOCK`，
   研究记录建议的 `launchctl asuser … getenv SSH_AUTH_SOCK` 未接入（Windows OpenSSH 命名管道同样不支持）。
   当前只探测 `$SSH_AUTH_SOCK`、`/run/user/*/keyring/ssh`、`/tmp/ssh-*/agent.*`、`~/.ssh/agent/*`；
   这三个文件系统探测路径在本机（Linux）也未构造真实 agent 用例，只走通了「探测不到就回退」的分支。
4. **CI 的 Windows / macOS 腿未验证，且 Windows 有已知风险**：`https` 会在**所有**平台拉入
   `openssl-sys`，而 `.github/workflows/ci.yml` 只给 Linux 装了 `libssl-dev`，Windows/macOS 没有安装
   OpenSSL。macOS runner 的 Homebrew 通常有 `openssl@3`，`openssl-sys` 会自动探测到；
   **Windows MSVC 没有 OpenSSL 也没有 vcpkg openssl**，`openssl-sys` 会直接以
   「could not find OpenSSL」失败。本机无法运行 CI，因此**没有验证**，需要后续处理：
   要么给 Windows 装 OpenSSL/vcpkg 并设 `OPENSSL_DIR`，要么改用
   `features = ["https", "ssh", "vendored-openssl"]`（Windows 需要 Perl 与 NASM，均在 runner 镜像里，
   未验证）。本 ticket 按任务要求先取 `["https", "ssh"]` 并如实记录，未擅自改动 CI 配置。
5. **跨目标 `cargo check` 只覆盖 `flashcast-platform`**：该 crate 不依赖 git2，因此这两个检查
   **没有**编译 libgit2 / openssl，不能证明 Windows/macOS 上 git2 的 https/ssh 可编译。
6. **主题恢复未实现**：`theme.json` 的语义由 ticket 06 落地，本 ticket 只宽容读取记录的主题名并
   在完成说明里如实呈现（「工作区记录的主题：dark」），不做应用、不发明 schema。
   插件侧只按 `settings.toml` 的 `disabledPlugins` 恢复；`manifest.json` 里的插件清单语义属 ticket 07，
   `unavailablePlugins` 目前只看 `disabledPlugins`。
7. **Windows / macOS 的真实克隆行为**（路径、ssh-agent、凭据管理器）未在本机验证。
8. **共享开发前缀**：`scripts/dev/linux-native-deps.sh` 的修复作用于 `FLASHCAST_NATIVE_DEPS_PREFIX`
   指定的前缀。默认前缀是跨 worktree 共享的，且另一个 ticket 正在同时改这个脚本；
   如果合并后脚本修复丢失，本机 `openssl-sys` 会重新编译失败（复现方式见上文）。

### 后继 ticket 16 可用的接缝

- `Host::workspace_remote()`：重新读取并脱敏当前工作区的远端关系（远端名、地址、当前分支、上游），
  同时写回设备本地表；`WorkspaceStatus.remote` 每次状态查询都会带上它（重启后仍在）。
  同步的 push / ff-only pull 据此确定 `refs/heads/<branch>:refs/heads/<branch>` 与
  `refs/heads/<branch>:refs/remotes/<remote>/<branch>`。
- `CredentialProvider` / `CredentialStore` 可直接复用：fetch/push 只要把同一个 provider 挂到
  `RemoteCallbacks::credentials`，并把用过的口令登记进 `secrets`，错误文本用 `redact_secrets` 过一遍。
- `Host::set_git_busy(true/false)` 包住整个网络操作（ticket 05 已就绪），操作结束后按 Git 状态重建界面。
- 取消模式可照搬：`CloneControl` 的做法是「回调返回 `false` + 置位后统一转成中文取消原因」。
  对 push 可用 `push_negotiation` 返回 `Err` 中止；对 fetch 用 `transfer_progress` 返回 `false`。
- 已知需要在 ticket 16 处理的 `git2` 行为：`MergeAnalysis` 是 bitflags（用
  `is_fast_forward()` / `is_up_to_date()` / `is_unborn()` 与 `contains(ANALYSIS_NORMAL)` 判分叉）；
  `checkout_head().force()` 会覆盖未提交修改，必须先做脏工作区检查。
